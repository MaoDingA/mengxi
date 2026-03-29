// consistency.rs — Cross-project color consistency analysis

use crate::grading_features::GradingFeatures;
use rusqlite::Connection;

/// Error type for consistency operations.
#[derive(Debug)]
pub enum ConsistencyError {
    /// One or more projects not found.
    ProjectNotFound(String),
    /// No fingerprints found in the specified projects.
    NoFingerprints,
    /// Database error.
    DbError(String),
}

impl std::fmt::Display for ConsistencyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConsistencyError::ProjectNotFound(name) => {
                write!(f, "CONSISTENCY_PROJECT_NOT_FOUND -- project '{}' not found", name)
            }
            ConsistencyError::NoFingerprints => {
                write!(f, "CONSISTENCY_NO_FINGERPRINTS -- no fingerprints found in specified projects")
            }
            ConsistencyError::DbError(msg) => write!(f, "CONSISTENCY_DB_ERROR -- {}", msg),
        }
    }
}

impl std::error::Error for ConsistencyError {}

/// Summary statistics for a single project's fingerprints.
#[derive(Debug, Clone)]
pub struct ProjectSummary {
    /// Project name.
    pub name: String,
    /// Number of fingerprints.
    pub fingerprint_count: usize,
    /// Mean L-channel histogram centroid.
    pub l_centroid: f64,
    /// Mean a-channel histogram centroid.
    pub a_centroid: f64,
    /// Mean b-channel histogram centroid.
    pub b_centroid: f64,
}

/// Distance between two projects' average feature vectors.
#[derive(Debug, Clone)]
pub struct ProjectPairDistance {
    /// First project name.
    pub project_a: String,
    /// Second project name.
    pub project_b: String,
    /// L1 distance between average histograms.
    pub histogram_distance: f64,
    /// Luminance mean difference.
    pub luminance_diff: f64,
}

/// An outlier fingerprint that deviates significantly from the project mean.
#[derive(Debug, Clone)]
pub struct OutlierFingerprint {
    /// Fingerprint ID.
    pub id: i64,
    /// Project name.
    pub project: String,
    /// File path.
    pub file: String,
    /// Distance from project mean.
    pub distance_from_mean: f64,
}

/// Complete consistency report across projects.
#[derive(Debug, Clone)]
pub struct ConsistencyReport {
    /// Per-project summaries.
    pub project_summaries: Vec<ProjectSummary>,
    /// Pairwise distances between all project pairs.
    pub pair_distances: Vec<ProjectPairDistance>,
    /// Outlier fingerprints (distance > 2 * average intra-project distance).
    pub outliers: Vec<OutlierFingerprint>,
    /// Overall cross-project consistency (0.0 = identical, 1.0 = very different).
    pub overall_consistency: f64,
}

/// Load average features for a project.
fn load_project_features(
    conn: &Connection,
    project_name: &str,
) -> Result<Vec<(i64, String, GradingFeatures)>, ConsistencyError> {
    // Check project exists
    let exists: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM projects WHERE name = ?1",
            [project_name],
            |row| row.get(0),
        )
        .map_err(|e| ConsistencyError::DbError(e.to_string()))?;

    if !exists {
        return Err(ConsistencyError::ProjectNotFound(project_name.to_string()));
    }

    let mut stmt = conn
        .prepare(
            "SELECT fp.id, p.path || '/' || f.filename, fp.hist_bins, \
                    fp.oklab_hist_l, fp.oklab_hist_a, fp.oklab_hist_b, fp.color_moments \
             FROM fingerprints fp \
             JOIN files f ON fp.file_id = f.id \
             JOIN projects p ON f.project_id = p.id \
             WHERE p.name = ?1 AND fp.oklab_hist_l IS NOT NULL \
             ORDER BY fp.id",
        )
        .map_err(|e| ConsistencyError::DbError(e.to_string()))?;

    let mut results = Vec::new();
    let mut rows = stmt
        .query_map(rusqlite::params![project_name], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)? as usize,
                row.get::<_, Vec<u8>>(3)?,
                row.get::<_, Vec<u8>>(4)?,
                row.get::<_, Vec<u8>>(5)?,
                row.get::<_, Vec<u8>>(6)?,
            ))
        })
        .map_err(|e| ConsistencyError::DbError(e.to_string()))?;

    while let Some(row) = rows.next().transpose().map_err(|e| ConsistencyError::DbError(e.to_string()))? {
        let (id, file, hist_bins, hl, ha, hb, moments) = row;
        match GradingFeatures::from_separate_blobs(&hl, &ha, &hb, &moments, hist_bins) {
            Ok(features) => results.push((id, file, features)),
            Err(_) => continue, // skip fingerprints with corrupt data
        }
    }

    Ok(results)
}

/// Compute the centroid (average) of a set of histogram vectors.
fn histogram_centroid<'a, I>(features: I) -> (Vec<f64>, Vec<f64>, Vec<f64>)
where
    I: Iterator<Item = &'a GradingFeatures>,
{
    let mut features_vec: Vec<&GradingFeatures> = features.collect();
    let n = features_vec.len().max(1) as f64;
    let bins = features_vec.first().map(|f| f.hist_l.len()).unwrap_or(0);

    let mut l_centroid = vec![0.0; bins];
    let mut a_centroid = vec![0.0; bins];
    let mut b_centroid = vec![0.0; bins];

    for f in &features_vec {
        for (i, v) in f.hist_l.iter().enumerate() {
            l_centroid[i] += v;
        }
        for (i, v) in f.hist_a.iter().enumerate() {
            a_centroid[i] += v;
        }
        for (i, v) in f.hist_b.iter().enumerate() {
            b_centroid[i] += v;
        }
    }

    for v in &mut l_centroid { *v /= n; }
    for v in &mut a_centroid { *v /= n; }
    for v in &mut b_centroid { *v /= n; }

    (l_centroid, a_centroid, b_centroid)
}

/// Compute L1 distance between two histogram vectors.
fn l1_distance(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b.iter()).map(|(x, y)| (x - y).abs()).sum()
}

/// Generate a cross-project consistency report.
pub fn generate_consistency_report(
    conn: &Connection,
    project_names: &[String],
) -> Result<ConsistencyReport, ConsistencyError> {
    if project_names.is_empty() {
        return Err(ConsistencyError::DbError("no projects specified".to_string()));
    }

    // Load features for each project
    let mut project_features: Vec<(String, Vec<(i64, String, GradingFeatures)>)> = Vec::new();
    for name in project_names {
        let features = load_project_features(conn, name)?;
        if features.is_empty() {
            continue;
        }
        project_features.push((name.clone(), features));
    }

    if project_features.is_empty() {
        return Err(ConsistencyError::NoFingerprints);
    }

    // Compute per-project summaries
    let mut project_summaries = Vec::new();
    let mut centroids: Vec<(String, Vec<f64>, Vec<f64>, Vec<f64>)> = Vec::new();

    for (name, features) in &project_features {
        let (l_c, a_c, b_c) = histogram_centroid(features.iter().map(|(_, _, f)| f));
        let l_mean = l_c.iter().sum::<f64>() / l_c.len().max(1) as f64;
        let a_mean = a_c.iter().sum::<f64>() / a_c.len().max(1) as f64;
        let b_mean = b_c.iter().sum::<f64>() / b_c.len().max(1) as f64;

        project_summaries.push(ProjectSummary {
            name: name.clone(),
            fingerprint_count: features.len(),
            l_centroid: l_mean,
            a_centroid: a_mean,
            b_centroid: b_mean,
        });
        centroids.push((name.clone(), l_c, a_c, b_c));
    }

    // Compute pairwise distances
    let mut pair_distances = Vec::new();
    for i in 0..centroids.len() {
        for j in (i + 1)..centroids.len() {
            let (name_a, l_a, a_a, b_a) = &centroids[i];
            let (name_b, l_b, a_b, b_b) = &centroids[j];

            let hist_dist = (l1_distance(l_a, l_b) + l1_distance(a_a, a_b) + l1_distance(b_a, b_b)) / 3.0;

            // Find luminance means
            let lum_a = project_summaries.iter().find(|s| s.name == *name_a).map(|s| s.l_centroid).unwrap_or(0.0);
            let lum_b = project_summaries.iter().find(|s| s.name == *name_b).map(|s| s.l_centroid).unwrap_or(0.0);

            pair_distances.push(ProjectPairDistance {
                project_a: name_a.clone(),
                project_b: name_b.clone(),
                histogram_distance: hist_dist,
                luminance_diff: (lum_a - lum_b).abs(),
            });
        }
    }

    // Find outliers within each project
    let mut outliers = Vec::new();
    for (name, features) in &project_features {
        if features.len() < 2 {
            continue;
        }
        let (l_c, a_c, b_c) = histogram_centroid(features.iter().map(|(_, _, f)| f));

        let distances: Vec<f64> = features
            .iter()
            .map(|(_, _, f)| {
                (l1_distance(&f.hist_l, &l_c) + l1_distance(&f.hist_a, &a_c) + l1_distance(&f.hist_b, &b_c)) / 3.0
            })
            .collect();

        let avg_dist = distances.iter().sum::<f64>() / distances.len() as f64;
        let threshold = avg_dist * 2.0;

        for (i, (id, file, _)) in features.iter().enumerate() {
            if distances[i] > threshold {
                outliers.push(OutlierFingerprint {
                    id: *id,
                    project: name.clone(),
                    file: file.clone(),
                    distance_from_mean: distances[i],
                });
            }
        }
    }

    // Sort outliers by distance (descending)
    outliers.sort_by(|a, b| b.distance_from_mean.partial_cmp(&a.distance_from_mean).unwrap_or(std::cmp::Ordering::Equal));

    // Overall consistency: average pairwise distance
    let overall_consistency = if pair_distances.is_empty() {
        0.0
    } else {
        pair_distances.iter().map(|d| d.histogram_distance).sum::<f64>() / pair_distances.len() as f64
    };

    Ok(ConsistencyReport {
        project_summaries,
        pair_distances,
        outliers,
        overall_consistency,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_l1_distance_identical() {
        let v = vec![0.1, 0.2, 0.3];
        assert!((l1_distance(&v, &v)).abs() < 1e-10);
    }

    #[test]
    fn test_l1_distance_different() {
        let a = vec![0.0, 0.0, 0.0];
        let b = vec![1.0, 1.0, 1.0];
        assert!((l1_distance(&a, &b) - 3.0).abs() < 1e-10);
    }

    #[test]
    fn test_histogram_centroid_single() {
        let f = GradingFeatures {
            hist_l: vec![0.2, 0.8],
            hist_a: vec![0.1, 0.9],
            hist_b: vec![0.3, 0.7],
            moments: [0.5, 0.1, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        };
        let (l, a, b) = histogram_centroid(std::iter::once(&f));
        assert!((l[0] - 0.2).abs() < 1e-10);
        assert!((l[1] - 0.8).abs() < 1e-10);
        assert!((a[0] - 0.1).abs() < 1e-10);
        assert!((b[1] - 0.7).abs() < 1e-10);
    }

    #[test]
    fn test_histogram_centroid_average() {
        let f1 = GradingFeatures {
            hist_l: vec![0.0, 1.0],
            hist_a: vec![0.0, 0.0],
            hist_b: vec![0.0, 0.0],
            moments: [0.0; 12],
        };
        let f2 = GradingFeatures {
            hist_l: vec![1.0, 0.0],
            hist_a: vec![0.0, 0.0],
            hist_b: vec![0.0, 0.0],
            moments: [0.0; 12],
        };
        let (l, _, _) = histogram_centroid([&f1, &f2].into_iter());
        assert!((l[0] - 0.5).abs() < 1e-10);
        assert!((l[1] - 0.5).abs() < 1e-10);
    }
}
