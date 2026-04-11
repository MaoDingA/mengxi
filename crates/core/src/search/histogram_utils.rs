// histogram_utils.rs — Histogram parsing and similarity utilities

use crate::fingerprint::BINS_PER_CHANNEL;

use super::types::SearchError;

/// Parse a comma-separated f64 string into a Vec of histogram bin values.
/// Expects exactly `BINS_PER_CHANNEL` (64) elements.
/// Rejects NaN and infinity values.
pub fn parse_histogram(text: &str) -> Result<Vec<f64>, SearchError> {
    let values: Vec<f64> = text
        .split(',')
        .filter(|s| !s.trim().is_empty())
        .map(|s| {
            s.trim()
                .parse::<f64>()
                .map_err(|_| SearchError::DatabaseError(format!("invalid histogram value: '{}'", s.trim())))
        })
        .collect::<Result<_, _>>()?;

    // Reject NaN and infinity values
    for v in &values {
        if !v.is_finite() {
            return Err(SearchError::DatabaseError(format!(
                "histogram contains non-finite value: {}",
                v
            )));
        }
    }

    if values.len() != BINS_PER_CHANNEL {
        return Err(SearchError::DatabaseError(format!(
            "expected {} histogram bins, got {}",
            BINS_PER_CHANNEL,
            values.len()
        )));
    }

    Ok(values)
}

/// Compute histogram intersection similarity between two normalized histograms.
/// Returns a value in [0.0, 1.0] where 1.0 = identical.
pub fn histogram_intersection(a: &[f64], b: &[f64]) -> f64 {
    let len = a.len().min(b.len());
    if len == 0 {
        return 0.0;
    }
    let sum: f64 = (0..len).map(|i| a[i].min(b[i])).sum();
    sum
}

/// Compute cosine similarity between two vectors.
/// Returns a value in [-1.0, 1.0] where 1.0 = identical, 0.0 = orthogonal.
pub fn cosine_similarity(a: &[f64], b: &[f64]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f64 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let norm_a: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt();
    let norm_b: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_histogram_valid() {
        let hist = "0.0,0.001,0.002,0.003,0.004,0.005,0.006,0.007,0.008,0.009,0.01,0.011,0.012,0.013,0.014,0.015,0.016,0.017,0.018,0.019,0.02,0.021,0.022,0.023,0.024,0.025,0.026,0.027,0.028,0.029,0.03,0.031,0.032,0.033,0.034,0.035,0.036,0.037,0.038,0.039,0.04,0.041,0.042,0.043,0.044,0.045,0.046,0.047,0.048,0.049,0.05,0.051,0.052,0.053,0.054,0.055,0.056,0.057,0.058,0.059,0.06,0.061,0.062,0.063";
        let result = parse_histogram(hist).unwrap();
        assert_eq!(result.len(), 64);
        assert_eq!(result[0], 0.0);
        assert_eq!(result[63], 0.063);
    }

    #[test]
    fn test_parse_histogram_with_spaces() {
        let hist = "0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0, 0.1, 0.2, 0.3, 0.4";
        let result = parse_histogram(hist).unwrap();
        assert_eq!(result.len(), 64);
    }

    #[test]
    fn test_parse_histogram_wrong_count() {
        let hist = "0.1,0.2,0.3";
        let result = parse_histogram(hist);
        assert!(result.is_err());
        assert!(format!("{}", result.unwrap_err()).contains("expected 64"));
    }

    #[test]
    fn test_parse_histogram_invalid_value() {
        let hist = "0.1,abc,0.3,0.4,0.5,0.6,0.7,0.8,0.9,1.0,0.1,0.2,0.3,0.4,0.5,0.6,0.7,0.8,0.9,1.0,0.1,0.2,0.3,0.4,0.5,0.6,0.7,0.8,0.9,1.0,0.1,0.2,0.3,0.4,0.5,0.6,0.7,0.8,0.9,1.0,0.1,0.2,0.3,0.4,0.5,0.6,0.7,0.8,0.9,1.0,0.1,0.2,0.3,0.4,0.5,0.6,0.7,0.8,0.9,1.0,0.1,0.2,0.3,0.4,0.5";
        let result = parse_histogram(hist);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_histogram_empty_string() {
        let result = parse_histogram("");
        assert!(result.is_err());
    }

    #[test]
    fn test_histogram_intersection() {
        let a = vec![0.1, 0.2, 0.3, 0.4];
        let b = vec![0.1, 0.2, 0.3, 0.4];
        assert!((histogram_intersection(&a, &b) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![1.0, 2.0, 3.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 1e-9);
    }
}
