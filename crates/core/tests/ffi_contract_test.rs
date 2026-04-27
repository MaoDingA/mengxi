// ffi_contract_test.rs -- Rust <-> MoonBit FFI contract tests.
//
// These tests exercise the public Rust wrappers that cross into the MoonBit
// static library. They intentionally avoid calling raw extern functions so the
// Core layer remains the only place that owns FFI safety checks.

use mengxi_core::color_science::{
    apply_aces_transform, generate_lut, is_aces_ffi_available, ACESColorSpace,
    ColorScienceError,
};
use mengxi_core::fingerprint::{
    extract_fingerprint, is_ffi_available, BINS_PER_CHANNEL,
    FingerprintError,
};

fn sum(values: &[f64]) -> f64 {
    values.iter().sum()
}

#[test]
fn ffi_runtime_contract_is_available() {
    assert!(
        is_ffi_available(),
        "fingerprint FFI should link and answer a trivial call"
    );
    assert!(
        is_aces_ffi_available(),
        "ACES FFI should link and answer a trivial identity transform"
    );
}

#[test]
fn ffi_wrappers_reject_invalid_rgb_buffers_before_crossing_ffi() {
    let fp = extract_fingerprint(&[0.5, 0.5, 0.5, 0.5], "linear");
    assert!(matches!(fp, Err(FingerprintError::InvalidInput(_))));

    let aces = apply_aces_transform(
        &[0.5, 0.5, 0.5, 0.5],
        ACESColorSpace::ACEScg,
        ACESColorSpace::Rec709,
    );
    assert!(matches!(aces, Err(ColorScienceError::FfiError(-1, _))));

    let lut = generate_lut(1, ACESColorSpace::ACEScg, ACESColorSpace::Rec709);
    assert!(matches!(lut, Err(ColorScienceError::FfiError(-1, _))));
}

#[test]
fn fingerprint_ffi_contract_returns_expected_shape_and_normalized_bins() {
    let data = [
        1.0_f64, 0.0, 0.0, // red
        0.0_f64, 1.0, 0.0, // green
    ];

    let fp = extract_fingerprint(&data, "linear").expect("fingerprint FFI should succeed");

    assert_eq!(fp.histogram_r.len(), BINS_PER_CHANNEL);
    assert_eq!(fp.histogram_g.len(), BINS_PER_CHANNEL);
    assert_eq!(fp.histogram_b.len(), BINS_PER_CHANNEL);
    assert!((sum(&fp.histogram_r) - 1.0).abs() < 1e-12);
    assert!((sum(&fp.histogram_g) - 1.0).abs() < 1e-12);
    assert!((sum(&fp.histogram_b) - 1.0).abs() < 1e-12);
    assert_eq!(fp.histogram_r[0], 0.5);
    assert_eq!(fp.histogram_r[63], 0.5);
    assert_eq!(fp.histogram_g[0], 0.5);
    assert_eq!(fp.histogram_g[63], 0.5);
    assert_eq!(fp.histogram_b[0], 1.0);
    assert!(fp.luminance_mean.is_finite());
    assert!(fp.luminance_stddev.is_finite());
}

#[test]
fn aces_ffi_contract_preserves_identity_and_rejects_log_input() {
    let data = [0.25_f64, 0.5, 0.75];

    let identity = apply_aces_transform(
        &data,
        ACESColorSpace::ACEScg,
        ACESColorSpace::ACEScg,
    )
    .expect("ACES identity transform should succeed");

    assert_eq!(identity.len(), data.len());
    for (actual, expected) in identity.iter().zip(data) {
        assert!((actual - expected).abs() < 1e-12);
    }

    let log_input = apply_aces_transform(
        &data,
        ACESColorSpace::ACEScct,
        ACESColorSpace::Rec709,
    );
    assert!(matches!(
        log_input,
        Err(ColorScienceError::LogDataRequiresConversion(_))
    ));
}

#[test]
fn lut_ffi_contract_returns_grid_order_and_finite_values() {
    let lut = generate_lut(2, ACESColorSpace::ACEScg, ACESColorSpace::ACEScg)
        .expect("identity LUT generation should succeed");

    assert_eq!(lut.len(), 2 * 2 * 2 * 3);
    assert!(lut.iter().all(|value| value.is_finite()));

    assert_eq!(&lut[0..3], &[0.0, 0.0, 0.0]);
    assert_eq!(&lut[3..6], &[1.0, 0.0, 0.0]);
    assert_eq!(&lut[21..24], &[1.0, 1.0, 1.0]);
}
