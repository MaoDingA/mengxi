use std::io::Write;

use serde::Serialize;

use mengxi_core::color_science;

const DELTA_E_THRESHOLD: f64 = 0.1;

/// Oklab Euclidean distance: sqrt((L2-L1)^2 + (a2-a1)^2 + (b2-b1)^2)
fn oklab_delta_e(l1: f64, a1: f64, b1: f64, l2: f64, a2: f64, b2: f64) -> f64 {
    let dl = l2 - l1;
    let da = a2 - a1;
    let db = b2 - b1;
    (dl * dl + da * da + db * db).sqrt()
}

#[derive(Debug, Clone, Serialize)]
pub struct ValidationResult {
    pub path: String,
    pub delta_e_max: f64,
    pub delta_e_mean: f64,
    pub passed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ValidationSummary {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ValidationOutput {
    pub results: Vec<ValidationResult>,
    pub summary: ValidationSummary,
}

/// Test vectors for round-trip precision validation.
/// Each entry: (name, input_values).
fn srgb_test_vectors() -> Vec<(&'static str, Vec<f64>)> {
    vec![
        ("pure_black", vec![0.0, 0.0, 0.0]),
        ("pure_white", vec![1.0, 1.0, 1.0]),
        ("mid_gray", vec![0.5, 0.5, 0.5]),
        ("saturated_red", vec![1.0, 0.0, 0.0]),
        ("saturated_green", vec![0.0, 1.0, 0.0]),
        ("saturated_blue", vec![0.0, 0.0, 1.0]),
    ]
}

fn acescct_test_vectors() -> Vec<(&'static str, Vec<f64>)> {
    vec![
        ("pure_black", vec![0.0, 0.0, 0.0]),
        ("mid_gray", vec![0.413, 0.413, 0.413]),
        ("saturated_red", vec![1.0, 0.0, 0.0]),
        ("saturated_green", vec![0.0, 1.0, 0.0]),
        ("saturated_blue", vec![0.0, 0.0, 1.0]),
    ]
}

/// Run round-trip test for a single color space path.
/// Returns (max_delta_e, mean_delta_e) across all test vectors.
fn run_roundtrip_test(
    vectors: &[(&str, Vec<f64>)],
    to_oklab: fn(&[f64]) -> Result<Vec<f64>, color_science::ColorScienceError>,
    from_oklab: fn(&[f64]) -> Result<Vec<f64>, color_science::ColorScienceError>,
) -> Result<(f64, f64), String> {
    let mut max_de = 0.0_f64;
    let mut total_de = 0.0_f64;
    let mut pixel_count = 0_usize;

    for (_name, input) in vectors {
        let oklab = to_oklab(input).map_err(|e| e.to_string())?;
        let back = from_oklab(&oklab).map_err(|e| e.to_string())?;

        let num_pixels = input.len() / 3;
        for i in 0..num_pixels {
            let (l1, a1, b1) = (oklab[i * 3], oklab[i * 3 + 1], oklab[i * 3 + 2]);
            let (r2, g2, b2) = (back[i * 3], back[i * 3 + 1], back[i * 3 + 2]);

            // Convert back to Oklab to measure ΔE in perceptual space
            let oklab2 = to_oklab(&[r2, g2, b2]).map_err(|e| e.to_string())?;
            let de = oklab_delta_e(l1, a1, b1, oklab2[0], oklab2[1], oklab2[2]);

            if de > max_de {
                max_de = de;
            }
            total_de += de;
            pixel_count += 1;
        }
    }

    let mean_de = if pixel_count > 0 { total_de / pixel_count as f64 } else { 0.0 };
    Ok((max_de, mean_de))
}

pub fn run_validate(is_json: bool) -> i32 {
    // Check FFI availability
    if !color_science::is_aces_ffi_available() {
        if is_json {
            let output = serde_json::json!({
                "error": { "code": "VALIDATION_FFI_UNAVAILABLE", "message": "MoonBit FFI library not linked" }
            });
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
        } else {
            eprintln!("Error: VALIDATION_FFI_UNAVAILABLE — MoonBit FFI library not linked");
        }
        return 1;
    }

    let mut results = Vec::new();

    // sRGB ↔ Oklab round-trip
    let vectors = srgb_test_vectors();
    match run_roundtrip_test(&vectors, color_science::srgb_to_oklab, color_science::oklab_to_srgb) {
        Ok((max_de, mean_de)) => {
            let passed = max_de < DELTA_E_THRESHOLD;
            results.push(ValidationResult {
                path: "srgb_to_oklab_roundtrip".to_string(),
                delta_e_max: max_de,
                delta_e_mean: mean_de,
                passed,
            });
        }
        Err(e) => {
            eprintln!("Warning: sRGB round-trip test failed: {}", e);
            results.push(ValidationResult {
                path: "srgb_to_oklab_roundtrip".to_string(),
                delta_e_max: -1.0,
                delta_e_mean: -1.0,
                passed: false,
            });
        }
    }

    // ACEScct ↔ Oklab round-trip
    let vectors = acescct_test_vectors();
    match run_roundtrip_test(&vectors, color_science::acescct_to_oklab, color_science::oklab_to_acescct) {
        Ok((max_de, mean_de)) => {
            let passed = max_de < DELTA_E_THRESHOLD;
            results.push(ValidationResult {
                path: "acescct_to_oklab_roundtrip".to_string(),
                delta_e_max: max_de,
                delta_e_mean: mean_de,
                passed,
            });
        }
        Err(e) => {
            eprintln!("Warning: ACEScct round-trip test failed: {}", e);
            results.push(ValidationResult {
                path: "acescct_to_oklab_roundtrip".to_string(),
                delta_e_max: -1.0,
                delta_e_mean: -1.0,
                passed: false,
            });
        }
    }

    let passed_count = results.iter().filter(|r| r.passed).count();
    let failed_count = results.len() - passed_count;
    let all_passed = failed_count == 0;
    let total = results.len();

    let output = ValidationOutput {
        results,
        summary: ValidationSummary {
            total,
            passed: passed_count,
            failed: failed_count,
        },
    };

    if is_json {
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
    } else {
        let _ = format_text(&mut std::io::stdout(), &output);
    }

    if all_passed { 0 } else { 1 }
}

fn format_text(w: &mut impl Write, output: &ValidationOutput) -> std::io::Result<()> {
    writeln!(w, "Color Space Validation Results")?;
    writeln!(w, "==============================")?;
    writeln!(w)?;

    for result in &output.results {
        let status = if result.passed { "✓ PASS" } else { "✗ FAIL (threshold: 0.1)" };
        writeln!(w, "{} ↔ Oklab round-trip", human_path(&result.path))?;
        writeln!(w, "  Max ΔE: {:.6}  Mean ΔE: {:.6}  {}", result.delta_e_max, result.delta_e_mean, status)?;
        writeln!(w)?;
    }

    if output.summary.failed == 0 {
        writeln!(w, "Summary: {}/{} tests passed", output.summary.passed, output.summary.total)?;
    } else {
        writeln!(w, "Summary: {}/{} tests passed, {} failed", output.summary.passed, output.summary.total, output.summary.failed)?;
    }
    Ok(())
}

fn human_path(path: &str) -> &str {
    match path {
        "srgb_to_oklab_roundtrip" => "sRGB",
        "acescct_to_oklab_roundtrip" => "ACEScct",
        _ => path,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_oklab_delta_e_zero() {
        let de = oklab_delta_e(0.5, 0.1, -0.2, 0.5, 0.1, -0.2);
        assert!((de - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_oklab_delta_e_known_distance() {
        // (0,0,0) to (1,0,0) should be exactly 1.0
        let de = oklab_delta_e(0.0, 0.0, 0.0, 1.0, 0.0, 0.0);
        assert!((de - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_oklab_delta_e_3d() {
        // (0,0,0) to (1,1,1) should be sqrt(3)
        let de = oklab_delta_e(0.0, 0.0, 0.0, 1.0, 1.0, 1.0);
        let expected = 3.0_f64.sqrt();
        assert!((de - expected).abs() < 1e-10);
    }

    #[test]
    fn test_srgb_roundtrip_all_pass() {
        if !color_science::is_aces_ffi_available() {
            eprintln!("note: skipping test_srgb_roundtrip_all_pass — FFI not available");
            return;
        }

        let vectors = srgb_test_vectors();
        let (max_de, mean_de) = run_roundtrip_test(
            &vectors,
            color_science::srgb_to_oklab,
            color_science::oklab_to_srgb,
        ).unwrap();

        assert!(max_de < DELTA_E_THRESHOLD, "sRGB max ΔE {} exceeds threshold {}", max_de, DELTA_E_THRESHOLD);
        assert!(mean_de < DELTA_E_THRESHOLD, "sRGB mean ΔE {} exceeds threshold {}", mean_de, DELTA_E_THRESHOLD);
    }

    #[test]
    fn test_acescct_roundtrip_all_pass() {
        if !color_science::is_aces_ffi_available() {
            eprintln!("note: skipping test_acescct_roundtrip_all_pass — FFI not available");
            return;
        }

        let vectors = acescct_test_vectors();
        let (max_de, mean_de) = run_roundtrip_test(
            &vectors,
            color_science::acescct_to_oklab,
            color_science::oklab_to_acescct,
        ).unwrap();

        assert!(max_de < DELTA_E_THRESHOLD, "ACEScct max ΔE {} exceeds threshold {}", max_de, DELTA_E_THRESHOLD);
        assert!(mean_de < DELTA_E_THRESHOLD, "ACEScct mean ΔE {} exceeds threshold {}", mean_de, DELTA_E_THRESHOLD);
    }

    #[test]
    fn test_json_output_valid() {
        if !color_science::is_aces_ffi_available() {
            eprintln!("note: skipping test_json_output_valid — FFI not available");
            return;
        }

        let vectors = srgb_test_vectors();
        let (max_de, mean_de) = run_roundtrip_test(
            &vectors,
            color_science::srgb_to_oklab,
            color_science::oklab_to_srgb,
        ).unwrap();

        let result = ValidationResult {
            path: "srgb_to_oklab_roundtrip".to_string(),
            delta_e_max: max_de,
            delta_e_mean: mean_de,
            passed: max_de < DELTA_E_THRESHOLD,
        };
        let output = ValidationOutput {
            results: vec![result],
            summary: ValidationSummary {
                total: 1,
                passed: 1,
                failed: 0,
            },
        };

        let json_str = serde_json::to_string_pretty(&output).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        assert_eq!(parsed["results"].as_array().unwrap().len(), 1);
        assert_eq!(parsed["summary"]["total"], 1);
        assert_eq!(parsed["summary"]["passed"], 1);
        assert_eq!(parsed["summary"]["failed"], 0);
        assert!(parsed["results"][0]["delta_e_max"].is_number());
    }

    #[test]
    fn test_text_output_contains_expected_strings() {
        if !color_science::is_aces_ffi_available() {
            eprintln!("note: skipping test_text_output_contains_expected_strings — FFI not available");
            return;
        }

        let vectors = srgb_test_vectors();
        let (max_de, mean_de) = run_roundtrip_test(
            &vectors,
            color_science::srgb_to_oklab,
            color_science::oklab_to_srgb,
        ).unwrap();

        let result = ValidationResult {
            path: "srgb_to_oklab_roundtrip".to_string(),
            delta_e_max: max_de,
            delta_e_mean: mean_de,
            passed: true,
        };
        let output = ValidationOutput {
            results: vec![result],
            summary: ValidationSummary {
                total: 1,
                passed: 1,
                failed: 0,
            },
        };

        let mut buf = Vec::new();
        format_text(&mut buf, &output).unwrap();
        let text = String::from_utf8(buf).unwrap();
        assert!(text.contains("Color Space Validation Results"));
        assert!(text.contains("sRGB"));
        assert!(text.contains("PASS"));
        assert!(text.contains("Max ΔE:"));
        assert!(text.contains("Mean ΔE:"));
    }

    #[test]
    fn test_validation_result_failed_state() {
        let result = ValidationResult {
            path: "test_path".to_string(),
            delta_e_max: 0.5,
            delta_e_mean: 0.3,
            passed: false,
        };
        assert!(!result.passed);
        assert!(result.delta_e_max > DELTA_E_THRESHOLD);
    }

    #[test]
    fn test_human_path_mapping() {
        assert_eq!(human_path("srgb_to_oklab_roundtrip"), "sRGB");
        assert_eq!(human_path("acescct_to_oklab_roundtrip"), "ACEScct");
        assert_eq!(human_path("unknown"), "unknown");
    }
}
