// hybrid_scoring.rs — Weighted multi-signal scoring engine (ADR-v2-3)

use std::collections::HashSet;

use crate::search::cosine_similarity;

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Errors from hybrid scoring operations.
#[derive(Debug)]
pub enum HybridScoringError {
    /// Weight validation failed.
    WeightError(String),
    /// No signals available for scoring.
    NoSignalsAvailable,
}

impl std::fmt::Display for HybridScoringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HybridScoringError::WeightError(msg) => {
                write!(f, "HYBRID_SCORING_WEIGHT_ERROR -- {}", msg)
            }
            HybridScoringError::NoSignalsAvailable => {
                write!(f, "HYBRID_SCORING_NO_SIGNALS -- no signals available for scoring")
            }
        }
    }
}

impl std::error::Error for HybridScoringError {}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Weight configuration for the three search signals.
/// Must validate: sum ~= 1.0 (tolerance 1e-6), each >= 0.1
#[derive(Debug, Clone, PartialEq)]
pub struct SignalWeights {
    /// Bhattacharyya similarity weight.
    pub grading: f64,
    /// CLIP cosine similarity weight.
    pub clip: f64,
    /// Tag match weight.
    pub tag: f64,
}

/// Per-candidate weights after degradation.
/// Missing signals have weight = 0.0; remaining are renormalized to sum 1.0.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedWeights {
    pub grading: f64,
    pub clip: f64,
    pub tag: f64,
}

/// Per-signal score breakdown. Missing signals are omitted (not Some(0.0)).
#[derive(Debug, Clone, PartialEq)]
pub struct ScoreBreakdown {
    pub grading: f64,
    pub clip: Option<f64>,
    pub tag: Option<f64>,
}

/// Combined hybrid score with per-signal breakdown.
#[derive(Debug, Clone, PartialEq)]
pub struct HybridScore {
    pub final_score: f64,
    pub breakdown: ScoreBreakdown,
    /// Metadata warnings (e.g., color space gap). Does not affect final_score.
    pub warnings: Vec<String>,
}

/// Search result with hybrid scoring details.
#[derive(Debug, Clone)]
pub struct HybridSearchResult {
    pub rank: usize,
    pub project_name: String,
    pub file_path: String,
    pub file_format: String,
    pub score: f64,
    pub score_breakdown: ScoreBreakdown,
    /// Match-level warnings (e.g., color space gap between reference and candidate).
    pub match_warnings: Vec<String>,
}

// ---------------------------------------------------------------------------
// SignalWeights validation
// ---------------------------------------------------------------------------

const SUM_TOLERANCE: f64 = 1e-6;
const MIN_WEIGHT: f64 = 0.1;

impl SignalWeights {
    /// Validate that weights sum to 1.0 (within tolerance) and each >= 0.1.
    pub fn validate(&self) -> Result<(), HybridScoringError> {
        let sum = self.grading + self.clip + self.tag;

        if self.grading < 0.0 || self.clip < 0.0 || self.tag < 0.0 {
            return Err(HybridScoringError::WeightError(
                "weights must be non-negative".to_string(),
            ));
        }

        if (sum - 1.0).abs() > SUM_TOLERANCE {
            return Err(HybridScoringError::WeightError(format!(
                "weights must sum to 1.0, got {:.10}",
                sum
            )));
        }

        if self.grading < MIN_WEIGHT || self.clip < MIN_WEIGHT || self.tag < MIN_WEIGHT {
            return Err(HybridScoringError::WeightError(format!(
                "each weight must be >= {}, got grading={}, clip={}, tag={}",
                MIN_WEIGHT, self.grading, self.clip, self.tag
            )));
        }

        Ok(())
    }

    /// Create default grading-first weights (grading=0.6, clip=0.3, tag=0.1).
    pub fn grading_first() -> Self {
        Self {
            grading: 0.6,
            clip: 0.3,
            tag: 0.1,
        }
    }

    /// Create default balanced weights (grading=0.4, clip=0.4, tag=0.2).
    pub fn balanced() -> Self {
        Self {
            grading: 0.4,
            clip: 0.4,
            tag: 0.2,
        }
    }
}

// ---------------------------------------------------------------------------
// Weight resolution with graceful degradation
// ---------------------------------------------------------------------------

/// Resolve weights based on available signals.
/// Missing signals have weight set to 0.0; remaining weights are renormalized to sum 1.0.
pub fn resolve_weights(
    base: &SignalWeights,
    has_grading: bool,
    has_clip: bool,
    has_tag: bool,
) -> Result<ResolvedWeights, HybridScoringError> {
    let g = if has_grading { base.grading } else { 0.0 };
    let c = if has_clip { base.clip } else { 0.0 };
    let t = if has_tag { base.tag } else { 0.0 };

    let total = g + c + t;
    if total <= 0.0 {
        return Err(HybridScoringError::NoSignalsAvailable);
    }

    Ok(ResolvedWeights {
        grading: g / total,
        clip: c / total,
        tag: t / total,
    })
}

// ---------------------------------------------------------------------------
// CLIP embedding similarity
// ---------------------------------------------------------------------------

/// Compute CLIP similarity between two embeddings.
/// Uses cosine similarity normalized to [0.0, 1.0].
/// Returns error on dimension mismatch or empty embeddings.
pub fn clip_similarity(
    query_embedding: &[f64],
    candidate_embedding: &[f64],
) -> Result<f64, HybridScoringError> {
    if query_embedding.is_empty() || candidate_embedding.is_empty() {
        return Err(HybridScoringError::WeightError(
            "empty embedding".to_string(),
        ));
    }

    if query_embedding.len() != candidate_embedding.len() {
        return Err(HybridScoringError::WeightError(format!(
            "embedding dimension mismatch: query={} candidate={}",
            query_embedding.len(),
            candidate_embedding.len()
        )));
    }

    // cosine_similarity returns [-1.0, 1.0]; normalize to [0.0, 1.0]
    let cos_sim = cosine_similarity(query_embedding, candidate_embedding);
    Ok((cos_sim + 1.0) / 2.0)
}

// ---------------------------------------------------------------------------
// Tag similarity (Jaccard index)
// ---------------------------------------------------------------------------

/// Compute tag similarity using Jaccard index: |intersection| / |union|.
/// Returns 0.0 when both sets are empty.
pub fn tag_similarity(query_tags: &[String], candidate_tags: &[String]) -> f64 {
    if query_tags.is_empty() && candidate_tags.is_empty() {
        return 0.0;
    }
    if query_tags.is_empty() || candidate_tags.is_empty() {
        return 0.0;
    }

    let query_set: HashSet<&str> = query_tags.iter().map(|s| s.as_str()).collect();
    let candidate_set: HashSet<&str> = candidate_tags.iter().map(|s| s.as_str()).collect();

    let intersection_count = query_set.intersection(&candidate_set).count();
    let union_count = query_set.union(&candidate_set).count();

    if union_count == 0 {
        return 0.0;
    }

    intersection_count as f64 / union_count as f64
}

// ---------------------------------------------------------------------------
// Hybrid score computation
// ---------------------------------------------------------------------------

/// Compute the hybrid score from available signals.
///
/// Missing signals (None) cause their weights to degrade to 0.0 with renormalization.
/// The final score is clamped to [0.0, 1.0].
/// Color space gap warnings are produced as metadata and do NOT affect the score (FR17).
pub fn compute_hybrid_score(
    grading_sim: Option<f64>,
    clip_sim: Option<f64>,
    tag_sim: Option<f64>,
    weights: &SignalWeights,
    query_cs: &str,
    candidate_cs: &str,
) -> Result<HybridScore, HybridScoringError> {
    // Note: weights.validate() is intentionally NOT called here.
    // Per-query --weights (FR15) allows weight=0.0 with warning.
    // CLI layer validates sum ~= 1.0 and non-negativity before reaching here.

    let has_grading = grading_sim.is_some();
    let has_clip = clip_sim.is_some();
    let has_tag = tag_sim.is_some();

    let resolved = resolve_weights(weights, has_grading, has_clip, has_tag)?;

    let mut final_score: f64 = 0.0;

    if let Some(gs) = grading_sim {
        final_score += resolved.grading * gs;
    }
    if let Some(cs) = clip_sim {
        final_score += resolved.clip * cs;
    }
    if let Some(ts) = tag_sim {
        final_score += resolved.tag * ts;
    }

    // Clamp to [0.0, 1.0] to handle floating point drift
    final_score = final_score.clamp(0.0, 1.0);

    let breakdown = ScoreBreakdown {
        grading: grading_sim.unwrap_or(0.0),
        clip: clip_sim,
        tag: tag_sim,
    };

    let warnings = match check_color_space_gap(query_cs, candidate_cs) {
        Some(w) => vec![w],
        None => vec![],
    };

    Ok(HybridScore {
        final_score,
        breakdown,
        warnings,
    })
}

// ---------------------------------------------------------------------------
// Color space gap detection
// ---------------------------------------------------------------------------

/// Map raw DB color_space_tag to human-readable display name.
fn display_name(cs_tag: &str) -> std::borrow::Cow<'_, str> {
    match cs_tag.to_lowercase().as_str() {
        "video" | "srgb" | "rec709" => std::borrow::Cow::Borrowed("sRGB"),
        "acescct" => std::borrow::Cow::Borrowed("ACEScct"),
        "acescg" => std::borrow::Cow::Borrowed("ACEScg"),
        "linear" => std::borrow::Cow::Borrowed("Linear"),
        "log" => std::borrow::Cow::Borrowed("Log"),
        _ => std::borrow::Cow::Borrowed(cs_tag),
    }
}

/// Determine the color space family for a given tag.
fn color_space_family(cs_tag: &str) -> &'static str {
    match cs_tag.to_lowercase().as_str() {
        "video" | "srgb" | "rec709" => "video",
        "acescct" | "acescg" | "log" => "log",
        "linear" => "linear",
        _ => "unknown",
    }
}

/// Check for a color space gap between query and candidate.
/// Returns `Some(warning_message)` if they belong to different families, `None` otherwise.
pub fn check_color_space_gap(query_cs: &str, candidate_cs: &str) -> Option<String> {
    let q_family = color_space_family(query_cs);
    let c_family = color_space_family(candidate_cs);

    if q_family == c_family {
        return None;
    }

    Some(format!(
        "reference ({}) ↔ candidate ({}) — large color space gap",
        display_name(query_cs),
        display_name(candidate_cs),
    ))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- SignalWeights validation tests ---

    #[test]
    fn test_signal_weights_valid() {
        let w = SignalWeights {
            grading: 0.6,
            clip: 0.3,
            tag: 0.1,
        };
        assert!(w.validate().is_ok());
    }

    #[test]
    fn test_signal_weights_balanced() {
        let w = SignalWeights::balanced();
        assert!(w.validate().is_ok());
        assert!((w.grading - 0.4).abs() < 1e-10);
        assert!((w.clip - 0.4).abs() < 1e-10);
        assert!((w.tag - 0.2).abs() < 1e-10);
    }

    #[test]
    fn test_signal_weights_grading_first() {
        let w = SignalWeights::grading_first();
        assert!(w.validate().is_ok());
        assert!((w.grading - 0.6).abs() < 1e-10);
    }

    #[test]
    fn test_signal_weights_sum_not_one() {
        let w = SignalWeights {
            grading: 0.5,
            clip: 0.5,
            tag: 0.0,
        };
        let result = w.validate();
        assert!(result.is_err());
        assert!(format!("{}", result.unwrap_err()).contains("each weight must be >= 0.1"));
    }

    #[test]
    fn test_signal_weights_below_minimum() {
        let w = SignalWeights {
            grading: 0.8,
            clip: 0.05,
            tag: 0.15,
        };
        let result = w.validate();
        assert!(result.is_err());
        assert!(format!("{}", result.unwrap_err()).contains("each weight must be >= 0.1"));
    }

    #[test]
    fn test_signal_weights_negative() {
        let w = SignalWeights {
            grading: -0.1,
            clip: 0.6,
            tag: 0.5,
        };
        let result = w.validate();
        assert!(result.is_err());
        assert!(format!("{}", result.unwrap_err()).contains("non-negative"));
    }

    #[test]
    fn test_signal_weights_sum_slightly_off() {
        // 0.6 + 0.3 + 0.1 = 1.0, but with floating point
        let w = SignalWeights {
            grading: 0.6,
            clip: 0.3,
            tag: 0.1,
        };
        assert!(w.validate().is_ok());
    }

    // --- resolve_weights tests ---

    #[test]
    fn test_resolve_weights_all_signals() {
        let w = SignalWeights {
            grading: 0.6,
            clip: 0.3,
            tag: 0.1,
        };
        let resolved = resolve_weights(&w, true, true, true).unwrap();
        assert!((resolved.grading - 0.6).abs() < 1e-10);
        assert!((resolved.clip - 0.3).abs() < 1e-10);
        assert!((resolved.tag - 0.1).abs() < 1e-10);
    }

    #[test]
    fn test_resolve_weights_missing_clip() {
        let w = SignalWeights {
            grading: 0.6,
            clip: 0.3,
            tag: 0.1,
        };
        let resolved = resolve_weights(&w, true, false, true).unwrap();
        let total = 0.6 + 0.1; // 0.7
        assert!((resolved.grading - 0.6 / total).abs() < 1e-10);
        assert!((resolved.clip - 0.0).abs() < 1e-10);
        assert!((resolved.tag - 0.1 / total).abs() < 1e-10);
        // Verify sum = 1.0
        assert!((resolved.grading + resolved.clip + resolved.tag - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_resolve_weights_missing_tag() {
        let w = SignalWeights {
            grading: 0.6,
            clip: 0.3,
            tag: 0.1,
        };
        let resolved = resolve_weights(&w, true, true, false).unwrap();
        let total = 0.6 + 0.3; // 0.9
        assert!((resolved.grading - 0.6 / total).abs() < 1e-10);
        assert!((resolved.clip - 0.3 / total).abs() < 1e-10);
        assert!((resolved.tag - 0.0).abs() < 1e-10);
        assert!((resolved.grading + resolved.clip + resolved.tag - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_resolve_weights_missing_both() {
        let w = SignalWeights {
            grading: 0.6,
            clip: 0.3,
            tag: 0.1,
        };
        let resolved = resolve_weights(&w, true, false, false).unwrap();
        assert!((resolved.grading - 1.0).abs() < 1e-10);
        assert!((resolved.clip - 0.0).abs() < 1e-10);
        assert!((resolved.tag - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_resolve_weights_only_clip() {
        let w = SignalWeights {
            grading: 0.6,
            clip: 0.3,
            tag: 0.1,
        };
        let resolved = resolve_weights(&w, false, true, false).unwrap();
        assert!((resolved.grading - 0.0).abs() < 1e-10);
        assert!((resolved.clip - 1.0).abs() < 1e-10);
        assert!((resolved.tag - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_resolve_weights_no_signals() {
        let w = SignalWeights {
            grading: 0.6,
            clip: 0.3,
            tag: 0.1,
        };
        let result = resolve_weights(&w, false, false, false);
        assert!(result.is_err());
        match result.unwrap_err() {
            HybridScoringError::NoSignalsAvailable => {}
            other => panic!("Expected NoSignalsAvailable, got: {:?}", other),
        }
    }

    // --- clip_similarity tests ---

    #[test]
    fn test_clip_similarity_identical() {
        let a: Vec<f64> = vec![0.1, 0.2, 0.3, 0.4];
        let score = clip_similarity(&a, &a).unwrap();
        assert!((score - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_clip_similarity_orthogonal() {
        let a: Vec<f64> = vec![1.0, 0.0];
        let b: Vec<f64> = vec![0.0, 1.0];
        // cos_sim = 0.0, normalized = (0 + 1) / 2 = 0.5
        let score = clip_similarity(&a, &b).unwrap();
        assert!((score - 0.5).abs() < 1e-10);
    }

    #[test]
    fn test_clip_similarity_opposite() {
        let a: Vec<f64> = vec![1.0, 0.0];
        let b: Vec<f64> = vec![-1.0, 0.0];
        // cos_sim = -1.0, normalized = (-1 + 1) / 2 = 0.0
        let score = clip_similarity(&a, &b).unwrap();
        assert!(score.abs() < 1e-10);
    }

    #[test]
    fn test_clip_similarity_dimension_mismatch() {
        let a: Vec<f64> = vec![1.0, 2.0];
        let b: Vec<f64> = vec![1.0, 2.0, 3.0];
        let result = clip_similarity(&a, &b);
        assert!(result.is_err());
        assert!(format!("{}", result.unwrap_err()).contains("dimension mismatch"));
    }

    #[test]
    fn test_clip_similarity_empty() {
        let a: Vec<f64> = vec![];
        let b: Vec<f64> = vec![1.0, 2.0];
        let result = clip_similarity(&a, &b);
        assert!(result.is_err());
    }

    // --- tag_similarity tests ---

    #[test]
    fn test_tag_similarity_identical() {
        let q = vec!["warm".to_string(), "industrial".to_string()];
        let c = vec!["warm".to_string(), "industrial".to_string()];
        assert!((tag_similarity(&q, &c) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_tag_similarity_disjoint() {
        let q = vec!["warm".to_string()];
        let c = vec!["cold".to_string()];
        assert!((tag_similarity(&q, &c) - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_tag_similarity_partial_overlap() {
        let q = vec!["warm".to_string(), "industrial".to_string()];
        let c = vec!["warm".to_string(), "cold".to_string()];
        // intersection = {"warm"}, union = {"warm", "industrial", "cold"} = 1/3
        assert!((tag_similarity(&q, &c) - 1.0 / 3.0).abs() < 1e-10);
    }

    #[test]
    fn test_tag_similarity_both_empty() {
        let q: Vec<String> = vec![];
        let c: Vec<String> = vec![];
        assert!((tag_similarity(&q, &c) - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_tag_similarity_query_empty() {
        let q: Vec<String> = vec![];
        let c = vec!["warm".to_string()];
        assert!((tag_similarity(&q, &c) - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_tag_similarity_candidate_empty() {
        let q = vec!["warm".to_string()];
        let c: Vec<String> = vec![];
        assert!((tag_similarity(&q, &c) - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_tag_similarity_one_common() {
        let q = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let c = vec!["c".to_string(), "d".to_string(), "e".to_string()];
        // intersection = {"c"}, union = {"a","b","c","d","e"} = 1/5
        assert!((tag_similarity(&q, &c) - 0.2).abs() < 1e-10);
    }

    // --- compute_hybrid_score tests ---

    #[test]
    fn test_hybrid_score_all_signals() {
        let w = SignalWeights {
            grading: 0.6,
            clip: 0.3,
            tag: 0.1,
        };
        let score = compute_hybrid_score(Some(0.8), Some(0.6), Some(0.9), &w, "video", "video").unwrap();
        // 0.6 * 0.8 + 0.3 * 0.6 + 0.1 * 0.9 = 0.48 + 0.18 + 0.09 = 0.75
        assert!((score.final_score - 0.75).abs() < 1e-10);
        assert!((score.breakdown.grading - 0.8).abs() < 1e-10);
        assert_eq!(score.breakdown.clip, Some(0.6));
        assert_eq!(score.breakdown.tag, Some(0.9));
    }

    #[test]
    fn test_hybrid_score_missing_clip() {
        let w = SignalWeights {
            grading: 0.6,
            clip: 0.3,
            tag: 0.1,
        };
        let score = compute_hybrid_score(Some(0.8), None, Some(0.9), &w, "video", "video").unwrap();
        // resolved: grading = 0.6/0.7, tag = 0.1/0.7
        // 0.857.. * 0.8 + 0.142.. * 0.9 = 0.6857.. + 0.1285.. = 0.8142..
        assert!(score.final_score > 0.0 && score.final_score <= 1.0);
        assert_eq!(score.breakdown.clip, None); // omitted, not Some(0.0)
        assert_eq!(score.breakdown.tag, Some(0.9));
        // Verify sum of resolved weights = 1.0
        let total: f64 = 0.6 / 0.7 + 0.1 / 0.7;
        assert!((total - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_hybrid_score_missing_tag() {
        let w = SignalWeights {
            grading: 0.6,
            clip: 0.3,
            tag: 0.1,
        };
        let score = compute_hybrid_score(Some(0.8), Some(0.6), None, &w, "video", "video").unwrap();
        assert!(score.final_score > 0.0 && score.final_score <= 1.0);
        assert_eq!(score.breakdown.tag, None); // omitted
    }

    #[test]
    fn test_hybrid_score_only_grading() {
        let w = SignalWeights {
            grading: 0.6,
            clip: 0.3,
            tag: 0.1,
        };
        let score = compute_hybrid_score(Some(0.5), None, None, &w, "video", "video").unwrap();
        // Only grading: weight = 1.0, score = 0.5
        assert!((score.final_score - 0.5).abs() < 1e-10);
        assert_eq!(score.breakdown.clip, None);
        assert_eq!(score.breakdown.tag, None);
    }

    #[test]
    fn test_hybrid_score_determinism() {
        let w = SignalWeights {
            grading: 0.6,
            clip: 0.3,
            tag: 0.1,
        };
        let s1 = compute_hybrid_score(Some(0.8), Some(0.6), Some(0.9), &w, "video", "video").unwrap();
        let s2 = compute_hybrid_score(Some(0.8), Some(0.6), Some(0.9), &w, "video", "video").unwrap();
        assert_eq!(s1.final_score, s2.final_score);
        assert_eq!(s1.breakdown, s2.breakdown);
    }

    #[test]
    fn test_hybrid_score_clamp() {
        let w = SignalWeights {
            grading: 0.6,
            clip: 0.3,
            tag: 0.1,
        };
        // All 1.0 should give 1.0 (no drift)
        let score = compute_hybrid_score(Some(1.0), Some(1.0), Some(1.0), &w, "video", "video").unwrap();
        assert!((score.final_score - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_hybrid_score_zero_weight_allowed() {
        // FR15: per-query --weights allows weight=0.0
        let w = SignalWeights {
            grading: 0.5,
            clip: 0.5,
            tag: 0.0,
        };
        let result = compute_hybrid_score(Some(0.8), Some(0.6), Some(0.9), &w, "video", "video");
        assert!(result.is_ok());
    }

    // --- ScoreBreakdown field access ---

    #[test]
    fn test_score_breakdown_fields() {
        let bd = ScoreBreakdown {
            grading: 0.9,
            clip: Some(0.7),
            tag: None,
        };
        assert!((bd.grading - 0.9).abs() < 1e-10);
        assert_eq!(bd.clip, Some(0.7));
        assert!(bd.tag.is_none());
    }

    // --- HybridSearchResult construction ---

    #[test]
    fn test_hybrid_search_result_construction() {
        let result = HybridSearchResult {
            rank: 1,
            project_name: "film_a".to_string(),
            file_path: "scene.dpx".to_string(),
            file_format: "dpx".to_string(),
            score: 0.85,
            score_breakdown: ScoreBreakdown {
                grading: 0.9,
                clip: Some(0.8),
                tag: None,
            },
            match_warnings: vec![],
        };
        assert_eq!(result.rank, 1);
        assert_eq!(result.project_name, "film_a");
        assert_eq!(result.file_path, "scene.dpx");
        assert!((result.score - 0.85).abs() < 1e-10);
    }

    // --- HybridScoringError display tests ---

    #[test]
    fn test_hybrid_scoring_error_display() {
        let err = HybridScoringError::WeightError("test error".to_string());
        assert!(format!("{}", err).contains("HYBRID_SCORING_WEIGHT_ERROR"));

        let err = HybridScoringError::NoSignalsAvailable;
        assert!(format!("{}", err).contains("HYBRID_SCORING_NO_SIGNALS"));
    }

    // --- Score range tests ---

    #[test]
    fn test_score_range_zero() {
        let w = SignalWeights {
            grading: 0.6,
            clip: 0.3,
            tag: 0.1,
        };
        let score = compute_hybrid_score(Some(0.0), Some(0.0), Some(0.0), &w, "video", "video").unwrap();
        assert!((score.final_score - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_score_range_one() {
        let w = SignalWeights {
            grading: 0.6,
            clip: 0.3,
            tag: 0.1,
        };
        let score = compute_hybrid_score(Some(1.0), Some(1.0), Some(1.0), &w, "video", "video").unwrap();
        assert!((score.final_score - 1.0).abs() < 1e-10);
    }

    // --- check_color_space_gap tests ---

    #[test]
    fn test_color_space_gap_same_family_video() {
        assert!(check_color_space_gap("video", "srgb").is_none());
        assert!(check_color_space_gap("srgb", "rec709").is_none());
        assert!(check_color_space_gap("video", "video").is_none());
    }

    #[test]
    fn test_color_space_gap_same_family_log() {
        assert!(check_color_space_gap("acescct", "acescg").is_none());
        assert!(check_color_space_gap("log", "acescct").is_none());
    }

    #[test]
    fn test_color_space_gap_same_family_linear() {
        assert!(check_color_space_gap("linear", "linear").is_none());
    }

    #[test]
    fn test_color_space_gap_different_family_video_log() {
        let w = check_color_space_gap("video", "acescct").unwrap();
        assert!(w.contains("sRGB"));
        assert!(w.contains("ACEScct"));
        assert!(w.contains("large color space gap"));
    }

    #[test]
    fn test_color_space_gap_different_family_video_linear() {
        let w = check_color_space_gap("srgb", "linear").unwrap();
        assert!(w.contains("sRGB"));
        assert!(w.contains("Linear"));
    }

    #[test]
    fn test_color_space_gap_different_family_log_linear() {
        let w = check_color_space_gap("acescct", "linear").unwrap();
        assert!(w.contains("ACEScct"));
        assert!(w.contains("Linear"));
    }

    #[test]
    fn test_color_space_gap_unknown_tag() {
        // Unknown tags treated as unique family — warns against known families
        assert!(check_color_space_gap("video", "custom").is_some());
        // Two unknown tags with same value = same family
        assert!(check_color_space_gap("custom", "custom").is_none());
    }

    // --- display_name tests ---

    #[test]
    fn test_display_name_known_tags() {
        assert_eq!(display_name("video"), std::borrow::Cow::Borrowed("sRGB"));
        assert_eq!(display_name("srgb"), std::borrow::Cow::Borrowed("sRGB"));
        assert_eq!(display_name("rec709"), std::borrow::Cow::Borrowed("sRGB"));
        assert_eq!(display_name("acescct"), std::borrow::Cow::Borrowed("ACEScct"));
        assert_eq!(display_name("acescg"), std::borrow::Cow::Borrowed("ACEScg"));
        assert_eq!(display_name("linear"), std::borrow::Cow::Borrowed("Linear"));
        assert_eq!(display_name("log"), std::borrow::Cow::Borrowed("Log"));
    }

    #[test]
    fn test_display_name_unknown_tag() {
        assert_eq!(display_name("custom"), std::borrow::Cow::Borrowed("custom"));
    }

    // --- compute_hybrid_score warning tests ---

    #[test]
    fn test_hybrid_score_no_warning_same_color_space() {
        let w = SignalWeights::grading_first();
        let score = compute_hybrid_score(Some(0.8), Some(0.6), Some(0.9), &w, "video", "srgb").unwrap();
        assert!(score.warnings.is_empty());
    }

    #[test]
    fn test_hybrid_score_warning_different_color_space() {
        let w = SignalWeights::grading_first();
        let score = compute_hybrid_score(Some(0.8), Some(0.6), Some(0.9), &w, "video", "acescct").unwrap();
        assert_eq!(score.warnings.len(), 1);
        assert!(score.warnings[0].contains("large color space gap"));
    }

    #[test]
    fn test_hybrid_score_warning_does_not_affect_score() {
        let w = SignalWeights::grading_first();
        let s1 = compute_hybrid_score(Some(0.8), Some(0.6), Some(0.9), &w, "video", "video").unwrap();
        let s2 = compute_hybrid_score(Some(0.8), Some(0.6), Some(0.9), &w, "video", "acescct").unwrap();
        // Same signals, different color spaces → same score (FR17)
        assert!((s1.final_score - s2.final_score).abs() < 1e-10);
        assert!(s1.warnings.is_empty());
        assert!(!s2.warnings.is_empty());
    }

    #[test]
    fn test_hybrid_search_result_includes_match_warnings() {
        let result = HybridSearchResult {
            rank: 1,
            project_name: "film_a".to_string(),
            file_path: "scene.dpx".to_string(),
            file_format: "dpx".to_string(),
            score: 0.85,
            score_breakdown: ScoreBreakdown {
                grading: 0.9,
                clip: Some(0.8),
                tag: None,
            },
            match_warnings: vec!["reference (sRGB) ↔ candidate (ACEScct) — large color space gap".to_string()],
        };
        assert_eq!(result.match_warnings.len(), 1);
        assert!(result.match_warnings[0].contains("large color space gap"));
    }
}
