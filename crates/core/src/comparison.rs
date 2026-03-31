// comparison.rs — Side-by-side fingerprint feature comparison

use crate::grading_features::GradingFeatures;
use rusqlite::Connection;

type FingerprintFeatureRow = (String, String, String, usize, Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>);

/// Error type for comparison operations.
#[derive(Debug, thiserror::Error)]
pub enum CompareError {
    /// Fingerprint not found in database.
    #[error("COMPARE_NOT_FOUND -- fingerprint {0} not found")]
    NotFound(i64),
    /// Database error.
    #[error("COMPARE_DB_ERROR -- {0}")]
    DbError(String),
}

/// Per-channel histogram delta (difference between two fingerprints).
#[derive(Debug, Clone)]
pub struct HistogramDelta {
    /// Mean absolute difference across bins.
    pub mean_abs_diff: f64,
    /// Maximum absolute difference in any single bin.
    pub max_abs_diff: f64,
    /// Bin index with the largest difference.
    pub max_diff_bin: usize,
    /// L1 norm (sum of absolute differences).
    pub l1_norm: f64,
}

/// Per-channel color moment delta.
#[derive(Debug, Clone)]
pub struct MomentDelta {
    /// Difference in mean.
    pub mean_delta: f64,
    /// Difference in stddev.
    pub stddev_delta: f64,
}

/// Complete comparison result between two fingerprints.
#[derive(Debug, Clone)]
pub struct CompareResult {
    /// First fingerprint ID.
    pub id_a: i64,
    /// Second fingerprint ID.
    pub id_b: i64,
    /// Project name of fingerprint A.
    pub project_a: String,
    /// Project name of fingerprint B.
    pub project_b: String,
    /// File path of fingerprint A.
    pub file_a: String,
    /// File path of fingerprint B.
    pub file_b: String,
    /// Color space of fingerprint A.
    pub color_space_a: String,
    /// Color space of fingerprint B.
    pub color_space_b: String,
    /// Color space match flag.
    pub color_space_match: bool,
    /// Histogram L channel delta.
    pub hist_l_delta: HistogramDelta,
    /// Histogram a channel delta.
    pub hist_a_delta: HistogramDelta,
    /// Histogram b channel delta.
    pub hist_b_delta: HistogramDelta,
    /// Luminance moment delta.
    pub luminance_delta: MomentDelta,
    /// Overall similarity score (0.0 = identical, 1.0 = completely different).
    pub overall_distance: f64,
}

/// Load grading features for a fingerprint from the database.
fn load_features(
    conn: &Connection,
    fp_id: i64,
) -> Result<(String, String, String, GradingFeatures), CompareError> {
    let (project, file, color_tag, hist_bins, hist_l, hist_a, hist_b, moments): FingerprintFeatureRow = conn
        .query_row(
            "SELECT p.name, p.path || '/' || f.filename, fp.color_space_tag, \
                    fp.hist_bins, fp.oklab_hist_l, fp.oklab_hist_a, fp.oklab_hist_b, fp.color_moments \
             FROM fingerprints fp \
             JOIN files f ON fp.file_id = f.id \
             JOIN projects p ON f.project_id = p.id \
             WHERE fp.id = ?1",
            rusqlite::params![fp_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get::<_, i64>(3)? as usize,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                ))
            },
        )
        .map_err(|e| {
            if e.to_string().contains("no rows") {
                CompareError::NotFound(fp_id)
            } else {
                CompareError::DbError(e.to_string())
            }
        })?;

    let features = GradingFeatures::from_separate_blobs(&hist_l, &hist_a, &hist_b, &moments, hist_bins)
        .map_err(|e| CompareError::DbError(e.to_string()))?;

    Ok((project, file, color_tag, features))
}

/// Compute histogram delta between two equal-length histogram slices.
fn compute_histogram_delta(hist_a: &[f64], hist_b: &[f64]) -> HistogramDelta {
    let mut sum_abs = 0.0_f64;
    let mut max_abs = 0.0_f64;
    let mut max_bin = 0usize;

    for (i, (a, b)) in hist_a.iter().zip(hist_b.iter()).enumerate() {
        let diff = (a - b).abs();
        sum_abs += diff;
        if diff > max_abs {
            max_abs = diff;
            max_bin = i;
        }
    }

    let n = hist_a.len().max(1) as f64;
    HistogramDelta {
        mean_abs_diff: sum_abs / n,
        max_abs_diff: max_abs,
        max_diff_bin: max_bin,
        l1_norm: sum_abs,
    }
}

/// Compare two fingerprints by their grading features.
///
/// Returns a detailed comparison including per-channel histogram deltas,
/// moment deltas, and color space compatibility.
pub fn compare_fingerprints(
    conn: &Connection,
    id_a: i64,
    id_b: i64,
) -> Result<CompareResult, CompareError> {
    let (project_a, file_a, cs_a, features_a) = load_features(conn, id_a)?;
    let (project_b, file_b, cs_b, features_b) = load_features(conn, id_b)?;

    let hist_l_delta = compute_histogram_delta(&features_a.hist_l, &features_b.hist_l);
    let hist_a_delta = compute_histogram_delta(&features_a.hist_a, &features_b.hist_a);
    let hist_b_delta = compute_histogram_delta(&features_a.hist_b, &features_b.hist_b);

    let luminance_delta = MomentDelta {
        mean_delta: features_a.moments[0] - features_b.moments[0],
        stddev_delta: features_a.moments[1] - features_b.moments[1],
    };

    // Overall distance: average of mean_abs_diff across all 3 channels
    let overall_distance = (hist_l_delta.mean_abs_diff + hist_a_delta.mean_abs_diff + hist_b_delta.mean_abs_diff) / 3.0;

    let color_space_match = cs_a == cs_b;

    Ok(CompareResult {
        id_a,
        id_b,
        project_a,
        project_b,
        file_a,
        file_b,
        color_space_a: cs_a,
        color_space_b: cs_b,
        color_space_match,
        hist_l_delta,
        hist_a_delta,
        hist_b_delta,
        luminance_delta,
        overall_distance,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_histogram_delta_identical() {
        let hist = vec![0.1, 0.2, 0.3, 0.4];
        let delta = compute_histogram_delta(&hist, &hist);
        assert!((delta.mean_abs_diff).abs() < 1e-10);
        assert!((delta.max_abs_diff).abs() < 1e-10);
        assert!((delta.l1_norm).abs() < 1e-10);
    }

    #[test]
    fn test_compute_histogram_delta_different() {
        let a = vec![0.0, 0.0, 0.0];
        let b = vec![1.0, 1.0, 1.0];
        let delta = compute_histogram_delta(&a, &b);
        assert!((delta.mean_abs_diff - 1.0).abs() < 1e-10);
        assert!((delta.max_abs_diff - 1.0).abs() < 1e-10);
        assert!((delta.l1_norm - 3.0).abs() < 1e-10);
    }

    #[test]
    fn test_compute_histogram_delta_partial() {
        let a = vec![0.5, 0.3];
        let b = vec![0.3, 0.5];
        let delta = compute_histogram_delta(&a, &b);
        assert!((delta.mean_abs_diff - 0.2).abs() < 1e-10);
        assert!((delta.max_abs_diff - 0.2).abs() < 1e-10);
    }
}
