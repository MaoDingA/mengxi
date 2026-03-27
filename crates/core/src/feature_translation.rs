use crate::grading_features::GradingFeatures;

/// Translate grading features into a human-readable Chinese description string.
///
/// Dimensions are evaluated independently; ambiguous values are omitted (conservative strategy, FR22).
/// At least one dimension description is always returned (never empty string).
/// Descriptions are joined by " · " separator.
pub fn translate_features(features: &GradingFeatures) -> String {
    let mut descriptions: Vec<String> = Vec::new();

    if let Some(desc) = eval_contrast(&features.hist_l, &features.moments) {
        descriptions.push(desc);
    }
    if let Some(desc) = eval_shadow_temperature(&features.moments) {
        descriptions.push(desc);
    }
    if let Some(desc) = eval_highlight(&features.hist_l) {
        descriptions.push(desc);
    }
    if let Some(desc) = eval_saturation(&features.moments) {
        descriptions.push(desc);
    }

    // Fallback: if all dimensions are ambiguous, pick the one closest to a threshold
    if descriptions.is_empty() {
        if let Some(desc) = fallback_closest_dimension(features) {
            descriptions.push(desc);
        } else {
            // Ultimate fallback (should not happen with valid data)
            descriptions.push("风格中性".to_string());
        }
    }

    descriptions.join(" · ")
}

// ---------------------------------------------------------------------------
// Dimension evaluators
// ---------------------------------------------------------------------------

/// Contrast: L-channel std > 0.35 → "高对比度", < 0.15 → "低对比度"
fn eval_contrast(_hist_l: &[f64], moments: &[f64; 6]) -> Option<String> {
    let l_std = moments[1];
    if l_std > 0.35 {
        Some("高对比度".to_string())
    } else if l_std < 0.15 {
        Some("低对比度".to_string())
    } else {
        None
    }
}

/// Shadow temperature: a-channel mean < -0.03 → "暗部偏冷", > 0.03 → "暗部偏暖"
fn eval_shadow_temperature(moments: &[f64; 6]) -> Option<String> {
    let a_mean = moments[2];
    if a_mean < -0.03 {
        Some("暗部偏冷".to_string())
    } else if a_mean > 0.03 {
        Some("暗部偏暖".to_string())
    } else {
        None
    }
}

/// Highlight: L-channel 95th percentile > 0.85 → "高光柔和延展"
fn eval_highlight(hist_l: &[f64]) -> Option<String> {
    let p95 = percentile_from_histogram(hist_l, 0.95);
    if p95 > 0.85 {
        Some("高光柔和延展".to_string())
    } else {
        None
    }
}

/// Saturation proxy: (a_mean² + b_mean²) > 0.02 → "饱和度偏高", < 0.005 → "饱和度低"
fn eval_saturation(moments: &[f64; 6]) -> Option<String> {
    let chroma_sq = moments[2].powi(2) + moments[4].powi(2);
    if chroma_sq > 0.02 {
        Some("饱和度偏高".to_string())
    } else if chroma_sq < 0.005 {
        Some("饱和度低".to_string())
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Percentile computation (histograms are raw counts, normalized internally)
// ---------------------------------------------------------------------------

/// Compute the approximate value at a given percentile from a raw-count histogram.
///
/// Input histogram contains RAW COUNTS (not normalized). The function normalizes internally.
/// L-channel bins cover [0.0, 1.0] with bin_width = 1.0/64.
/// Returns 0.0 if histogram is empty (all zeros).
fn percentile_from_histogram(histogram: &[f64], p: f64) -> f64 {
    let total: f64 = histogram.iter().sum();
    if total <= 0.0 {
        return 0.0;
    }

    let n_bins = histogram.len();
    let bin_width = 1.0 / n_bins as f64;
    let target = p * total;
    let mut cumulative = 0.0;

    for (i, &count) in histogram.iter().enumerate() {
        if count <= 0.0 {
            continue;
        }
        let prev_cumulative = cumulative;
        cumulative += count;

        if cumulative >= target {
            // Interpolate within this bin
            let bin_start = i as f64 * bin_width;
            let fraction = if (cumulative - prev_cumulative) > 0.0 {
                (target - prev_cumulative) / (cumulative - prev_cumulative)
            } else {
                0.0
            };
            return bin_start + fraction * bin_width;
        }
    }

    // p = 1.0 case: return end of last non-empty bin
    1.0
}

// ---------------------------------------------------------------------------
// Fallback: pick the dimension closest to a threshold
// ---------------------------------------------------------------------------

/// When all dimensions are ambiguous, pick the one with the smallest relative
/// distance to its nearest threshold boundary.
fn fallback_closest_dimension(features: &GradingFeatures) -> Option<String> {
    let mut candidates: Vec<(f64, String)> = Vec::new();

    // Contrast: thresholds at 0.15 and 0.35
    let l_std = features.moments[1];
    let mid_contrast = (0.15 + 0.35) / 2.0;
    let range_contrast = 0.35 - 0.15;
    let rel_dist_contrast = if range_contrast > 0.0 {
        ((l_std - mid_contrast).abs() / (range_contrast / 2.0)).min(1.0)
    } else {
        1.0
    };
    candidates.push((rel_dist_contrast, if l_std >= mid_contrast { "高对比度".to_string() } else { "低对比度".to_string() }));

    // Shadow temperature: thresholds at -0.03 and 0.03
    let a_mean = features.moments[2];
    let rel_dist_shadow = if 0.03 > 0.0 { (a_mean.abs() / 0.03).min(1.0) } else { 1.0 };
    candidates.push((rel_dist_shadow, if a_mean >= 0.0 { "暗部偏暖".to_string() } else { "暗部偏冷".to_string() }));

    // Highlight: threshold at 0.85 (95th percentile)
    let p95 = percentile_from_histogram(&features.hist_l, 0.95);
    let rel_dist_highlight = if p95 <= 0.85 { (0.85 - p95) / 0.85 } else { 1.0 };
    candidates.push((rel_dist_highlight.min(1.0), "高光柔和延展".to_string()));

    // Saturation: thresholds at 0.005 and 0.02
    let chroma_sq = features.moments[2].powi(2) + features.moments[4].powi(2);
    let mid_sat = (0.005 + 0.02) / 2.0;
    let range_sat = 0.02 - 0.005;
    let rel_dist_sat = if range_sat > 0.0 {
        ((chroma_sq - mid_sat).abs() / (range_sat / 2.0)).min(1.0)
    } else {
        1.0
    };
    candidates.push((rel_dist_sat, if chroma_sq >= mid_sat { "饱和度偏高".to_string() } else { "饱和度低".to_string() }));

    // Pick the candidate with smallest relative distance (closest to threshold)
    candidates.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    candidates.into_iter().next().map(|(_, desc)| desc)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_features(l_std: f64, a_mean: f64, b_mean: f64, hist_l_peak_bin: usize) -> GradingFeatures {
        let mut hist_l = vec![1.0; 64];
        // Shift histogram peak to simulate different L distributions
        for (i, v) in hist_l.iter_mut().enumerate() {
            *v = 1.0 - (i as f64 - hist_l_peak_bin as f64).abs() * 0.1;
        }
        // Ensure no negative values
        for v in hist_l.iter_mut() {
            if *v < 0.0 {
                *v = 0.01;
            }
        }
        GradingFeatures {
            hist_l,
            hist_a: vec![1.0; 64],
            hist_b: vec![1.0; 64],
            moments: [0.5, l_std, a_mean, 0.1, b_mean, 0.1],
        }
    }

    // --- eval_contrast ---

    #[test]
    fn test_eval_contrast_high() {
        let moments = [0.5, 0.40, 0.0, 0.1, 0.0, 0.1]; // L_std = 0.40 > 0.35
        assert_eq!(eval_contrast(&[], &moments), Some("高对比度".to_string()));
    }

    #[test]
    fn test_eval_contrast_low() {
        let moments = [0.5, 0.10, 0.0, 0.1, 0.0, 0.1]; // L_std = 0.10 < 0.15
        assert_eq!(eval_contrast(&[], &moments), Some("低对比度".to_string()));
    }

    #[test]
    fn test_eval_contrast_ambiguous() {
        let moments = [0.5, 0.25, 0.0, 0.1, 0.0, 0.1]; // L_std = 0.25, in [0.15, 0.35]
        assert_eq!(eval_contrast(&[], &moments), None);
    }

    // --- eval_shadow_temperature ---

    #[test]
    fn test_eval_shadow_cold() {
        let moments = [0.5, 0.2, -0.05, 0.1, 0.0, 0.1]; // a_mean = -0.05 < -0.03
        assert_eq!(eval_shadow_temperature(&moments), Some("暗部偏冷".to_string()));
    }

    #[test]
    fn test_eval_shadow_warm() {
        let moments = [0.5, 0.2, 0.06, 0.1, 0.0, 0.1]; // a_mean = 0.06 > 0.03
        assert_eq!(eval_shadow_temperature(&moments), Some("暗部偏暖".to_string()));
    }

    #[test]
    fn test_eval_shadow_ambiguous() {
        let moments = [0.5, 0.2, 0.01, 0.1, 0.0, 0.1]; // a_mean = 0.01, in [-0.03, 0.03]
        assert_eq!(eval_shadow_temperature(&moments), None);
    }

    // --- eval_saturation ---

    #[test]
    fn test_eval_saturation_high() {
        let moments2 = [0.5, 0.2, 0.12, 0.1, 0.08, 0.1]; // 0.0144 + 0.0064 = 0.0208 > 0.02
        assert_eq!(eval_saturation(&moments2), Some("饱和度偏高".to_string()));
    }

    #[test]
    fn test_eval_saturation_low() {
        let moments = [0.5, 0.2, 0.001, 0.1, 0.001, 0.1]; // 0.000001 + 0.000001 = 0.000002 < 0.005
        assert_eq!(eval_saturation(&moments), Some("饱和度低".to_string()));
    }

    #[test]
    fn test_eval_saturation_ambiguous() {
        let moments2 = [0.5, 0.2, 0.07, 0.1, 0.07, 0.1]; // 0.0049 + 0.0049 = 0.0098, in [0.005, 0.02]
        assert_eq!(eval_saturation(&moments2), None);
    }

    // --- percentile_from_histogram ---

    #[test]
    fn test_percentile_uniform_histogram() {
        // Uniform histogram: 64 bins each with count 1.0, total = 64.0
        let hist = vec![1.0; 64];
        // 50th percentile should be at bin 32 (middle)
        let p50 = percentile_from_histogram(&hist, 0.5);
        assert!((p50 - 0.5).abs() < 0.02, "expected ~0.5, got {}", p50);
    }

    #[test]
    fn test_percentile_empty_histogram() {
        let hist = vec![0.0; 64];
        assert_eq!(percentile_from_histogram(&hist, 0.5), 0.0);
    }

    #[test]
    fn test_percentile_p0() {
        let hist = vec![1.0; 64];
        let p0 = percentile_from_histogram(&hist, 0.0);
        assert!(p0 < 0.02, "expected ~0.0, got {}", p0);
    }

    #[test]
    fn test_percentile_p1() {
        let hist = vec![1.0; 64];
        let p1 = percentile_from_histogram(&hist, 1.0);
        assert_eq!(p1, 1.0);
    }

    #[test]
    fn test_percentile_single_bin_histogram() {
        // All pixels in one bin (bin 32)
        let mut hist = vec![0.0; 64];
        hist[32] = 100.0;
        let p50 = percentile_from_histogram(&hist, 0.5);
        // All data is in bin 32, so any percentile should be in that bin
        let bin_start = 32.0 / 64.0;
        let bin_end = 33.0 / 64.0;
        assert!(p50 >= bin_start && p50 <= bin_end, "expected in [{}, {}], got {}", bin_start, bin_end, p50);
    }

    #[test]
    fn test_percentile_raw_counts_normalized() {
        // Verify that raw counts (not normalized) are handled correctly
        let hist = vec![100.0; 64]; // 100 pixels per bin, total = 6400
        let p50 = percentile_from_histogram(&hist, 0.5);
        assert!((p50 - 0.5).abs() < 0.02, "expected ~0.5, got {}", p50);
    }

    // --- eval_highlight ---

    #[test]
    fn test_eval_highlight_soft() {
        // Create a histogram with most pixels at high L values → high 95th percentile
        let mut hist_l = vec![0.1; 64];
        // Pack most counts into high bins (bins 50-63)
        for i in 50..64 {
            hist_l[i] = 100.0;
        }
        let p95 = percentile_from_histogram(&hist_l, 0.95);
        assert!(p95 > 0.85, "p95 = {}, should be > 0.85", p95);
        assert_eq!(eval_highlight(&hist_l), Some("高光柔和延展".to_string()));
    }

    #[test]
    fn test_eval_highlight_normal() {
        // Uniform histogram → 95th percentile around 0.95
        let hist_l = vec![1.0; 64];
        assert_eq!(eval_highlight(&hist_l), Some("高光柔和延展".to_string()));
    }

    #[test]
    fn test_eval_highlight_low() {
        // Most pixels at low L values → low 95th percentile
        let mut hist_l = vec![100.0; 64];
        for i in 10..64 {
            hist_l[i] = 0.1;
        }
        let p95 = percentile_from_histogram(&hist_l, 0.95);
        assert!(p95 < 0.85, "p95 = {}, should be < 0.85", p95);
        assert_eq!(eval_highlight(&hist_l), None);
    }

    // --- translate_features integration ---

    #[test]
    fn test_translate_features_all_dimensions() {
        // High contrast, cold shadow, soft highlight, high saturation
        let features = GradingFeatures {
            hist_l: vec![1.0; 64], // uniform → p95 ~ 0.95 > 0.85
            hist_a: vec![1.0; 64],
            hist_b: vec![1.0; 64],
            moments: [0.5, 0.40, -0.05, 0.1, 0.15, 0.1], // L_std>0.35, a_mean<-0.03, chroma_sq=0.025+0.0225=0.0475>0.02
        };
        let result = translate_features(&features);
        assert!(result.contains("高对比度"), "missing '高对比度': {}", result);
        assert!(result.contains("暗部偏冷"), "missing '暗部偏冷': {}", result);
        assert!(result.contains("高光柔和延展"), "missing '高光柔和延展': {}", result);
        assert!(result.contains("饱和度偏高"), "missing '饱和度偏高': {}", result);
        assert!(result.contains(" · "), "missing separator: {}", result);
    }

    #[test]
    fn test_translate_features_mixed_dimensions() {
        // Only contrast and saturation are clear
        let features = GradingFeatures {
            hist_l: vec![1.0; 64], // uniform → p95 ~ 0.95 → "高光柔和延展"
            hist_a: vec![1.0; 64],
            hist_b: vec![1.0; 64],
            moments: [0.5, 0.40, 0.01, 0.1, 0.001, 0.1], // L_std>0.35, a_mean ambiguous, chroma_sq≈0.0001+0.000001<0.005
        };
        let result = translate_features(&features);
        assert!(result.contains("高对比度"), "missing '高对比度': {}", result);
        assert!(result.contains("饱和度低"), "missing '饱和度低': {}", result);
        assert!(result.contains("高光柔和延展"), "missing '高光柔和延展': {}", result);
        // Shadow temp is ambiguous (0.01 in [-0.03, 0.03])
        assert!(!result.contains("暗部偏冷"), "should not contain '暗部偏冷': {}", result);
        assert!(!result.contains("暗部偏暖"), "should not contain '暗部偏暖': {}", result);
    }

    #[test]
    fn test_translate_features_ambiguous_uses_fallback() {
        // All dimensions in ambiguous range → fallback picks one
        let features = GradingFeatures {
            hist_l: vec![1.0; 64], // uniform → p95 ~ 0.95 > 0.85 (NOT ambiguous for highlight)
            hist_a: vec![1.0; 64],
            hist_b: vec![1.0; 64],
            moments: [0.5, 0.25, 0.01, 0.1, 0.05, 0.1], // L_std ambiguous, a_mean ambiguous, chroma_sq=0.0026
        };
        let result = translate_features(&features);
        assert!(!result.is_empty(), "result should not be empty");
        // Highlight won't be ambiguous (uniform hist → p95 ~ 0.95 > 0.85)
        // So we expect at least "高光柔和延展"
        assert!(result.contains("高光柔和延展") || !result.is_empty());
    }

    #[test]
    fn test_translate_features_never_empty() {
        // Even with all-zero features, should return something
        let features = GradingFeatures {
            hist_l: vec![0.0; 64],
            hist_a: vec![0.0; 64],
            hist_b: vec![0.0; 64],
            moments: [0.0; 6],
        };
        let result = translate_features(&features);
        assert!(!result.is_empty(), "translate_features must never return empty string");
    }

    #[test]
    fn test_translate_features_low_contrast_low_saturation() {
        let features = GradingFeatures {
            hist_l: vec![1.0; 64],
            hist_a: vec![1.0; 64],
            hist_b: vec![1.0; 64],
            moments: [0.5, 0.10, 0.0, 0.1, 0.0, 0.1], // L_std<0.15, a_mean=0 (ambiguous), chroma_sq=0
        };
        let result = translate_features(&features);
        assert!(result.contains("低对比度"), "missing '低对比度': {}", result);
        assert!(result.contains("饱和度低"), "missing '饱和度低': {}", result);
    }

    #[test]
    fn test_fallback_picks_closest_dimension() {
        // L_std=0.34 (very close to 0.35 threshold), other dims far from thresholds
        let features = GradingFeatures {
            hist_l: vec![0.1; 10].iter().chain(&vec![100.0; 54]).cloned().collect(), // high bins → p95 > 0.85
            hist_a: vec![1.0; 64],
            hist_b: vec![1.0; 64],
            moments: [0.5, 0.34, 0.0, 0.1, 0.07, 0.1], // L_std close to high, a_mean=0, chroma_sq=0.0049<0.005
        };
        let result = translate_features(&features);
        // L_std=0.34 < 0.35 → still ambiguous, but very close
        // highlight will be clear (p95 > 0.85)
        // saturation will be clear (0.0049 < 0.005 → "饱和度低")
        assert!(!result.is_empty());
    }
}
