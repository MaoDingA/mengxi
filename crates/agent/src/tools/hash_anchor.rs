// tools/hash_anchor.rs — Hash-anchored LUT verification for safe editing
//
// Divides LUT entries into luminance regions and computes content hashes
// per region, enabling optimistic concurrency control for agent-driven edits.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use mengxi_format::lut::LutData;

// ---------------------------------------------------------------------------
// Luminance regions
// ---------------------------------------------------------------------------

/// Luminance-based tonal region for selective LUT editing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LuminanceRegion {
    Shadows,
    Midtones,
    Highlights,
}

impl LuminanceRegion {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Shadows => "shadows",
            Self::Midtones => "midtones",
            Self::Highlights => "highlights",
        }
    }

    /// All regions in order.
    pub fn all() -> &'static [LuminanceRegion] {
        &[Self::Shadows, Self::Midtones, Self::Highlights]
    }
}

/// Classify an RGB triplet into a luminance region using BT.709 coefficients.
pub fn classify_entry(r: f64, g: f64, b: f64) -> LuminanceRegion {
    let lum = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    if lum <= 0.25 {
        LuminanceRegion::Shadows
    } else if lum <= 0.75 {
        LuminanceRegion::Midtones
    } else {
        LuminanceRegion::Highlights
    }
}

// ---------------------------------------------------------------------------
// Hash utilities
// ---------------------------------------------------------------------------

/// Compute a stable hash over a slice of f64 values.
///
/// Uses bit-level hashing of the raw f64 representation.
pub fn hash_values(values: &[f64]) -> u64 {
    let mut hasher = DefaultHasher::new();
    for v in values {
        v.to_bits().hash(&mut hasher);
    }
    hasher.finish()
}

// ---------------------------------------------------------------------------
// Region anchors
// ---------------------------------------------------------------------------

/// Content hash for a single luminance region of a LUT.
#[derive(Debug, Clone)]
pub struct RegionAnchor {
    pub region: LuminanceRegion,
    pub hash: u64,
    pub entry_count: usize,
}

/// Full anchored snapshot of a LUT for verification.
#[derive(Debug, Clone)]
pub struct AnchoredLut {
    pub full_hash: u64,
    pub region_anchors: Vec<RegionAnchor>,
}

/// Compute hash anchors for each luminance region in the LUT.
///
/// Groups entries by their **output** luminance (the RGB values stored in the LUT),
/// then hashes each group independently.
pub fn compute_anchors(lut: &LutData) -> AnchoredLut {
    let total = lut.grid_size as usize;
    let total_entries = total * total * total;

    // Collect values per region
    let mut region_values: [Vec<f64>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    let mut all_values = Vec::with_capacity(lut.values.len());

    for i in 0..total_entries {
        let r = lut.values[i * 3];
        let g = lut.values[i * 3 + 1];
        let b = lut.values[i * 3 + 2];

        all_values.push(r);
        all_values.push(g);
        all_values.push(b);

        let region = classify_entry(r, g, b);
        let idx = match region {
            LuminanceRegion::Shadows => 0,
            LuminanceRegion::Midtones => 1,
            LuminanceRegion::Highlights => 2,
        };
        region_values[idx].push(r);
        region_values[idx].push(g);
        region_values[idx].push(b);
    }

    let full_hash = hash_values(&all_values);
    let region_anchors = LuminanceRegion::all()
        .iter()
        .enumerate()
        .map(|(i, &region)| {
            let vals = &region_values[i];
            RegionAnchor {
                region,
                hash: hash_values(vals),
                entry_count: vals.len() / 3,
            }
        })
        .collect();

    AnchoredLut {
        full_hash,
        region_anchors,
    }
}

/// Verify that a specific region's hash still matches the current LUT state.
pub fn verify_region(anchored: &AnchoredLut, lut: &LutData, region: LuminanceRegion) -> bool {
    let current = compute_anchors(lut);
    anchored
        .region_anchors
        .iter()
        .zip(current.region_anchors.iter())
        .any(|(expected, actual)| expected.region == region && expected.hash == actual.hash)
}

/// Verify the full LUT hash matches.
pub fn verify_full(anchored: &AnchoredLut, lut: &LutData) -> bool {
    let current = compute_anchors(lut);
    anchored.full_hash == current.full_hash
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_shadows() {
        assert_eq!(classify_entry(0.0, 0.0, 0.0), LuminanceRegion::Shadows);
        assert_eq!(classify_entry(0.1, 0.1, 0.1), LuminanceRegion::Shadows);
    }

    #[test]
    fn test_classify_midtones() {
        assert_eq!(classify_entry(0.5, 0.5, 0.5), LuminanceRegion::Midtones);
    }

    #[test]
    fn test_classify_highlights() {
        assert_eq!(classify_entry(1.0, 1.0, 1.0), LuminanceRegion::Highlights);
        assert_eq!(classify_entry(0.9, 0.9, 0.9), LuminanceRegion::Highlights);
    }

    #[test]
    fn test_compute_anchors_identity() {
        let lut = LutData::identity(9);
        let anchored = compute_anchors(&lut);
        assert_ne!(anchored.full_hash, 0);
        assert_eq!(anchored.region_anchors.len(), 3);
        // Identity LUT: all three regions should have entries
        for anchor in &anchored.region_anchors {
            assert!(anchor.entry_count > 0, "{} has no entries", anchor.region.label());
        }
    }

    #[test]
    fn test_verify_region_unchanged() {
        let lut = LutData::identity(9);
        let anchored = compute_anchors(&lut);
        assert!(verify_region(&anchored, &lut, LuminanceRegion::Shadows));
        assert!(verify_region(&anchored, &lut, LuminanceRegion::Midtones));
        assert!(verify_region(&anchored, &lut, LuminanceRegion::Highlights));
        assert!(verify_full(&anchored, &lut));
    }

    #[test]
    fn test_verify_region_modified() {
        let lut = LutData::identity(9);
        let anchored = compute_anchors(&lut);

        // Modify all values (push shadows brighter)
        let mut modified = lut.clone();
        for v in modified.values.iter_mut() {
            *v = (*v + 0.3).min(1.0);
        }
        modified.validate().unwrap();

        // Full hash should differ
        assert!(!verify_full(&anchored, &modified));
    }

    #[test]
    fn test_hash_deterministic() {
        let lut = LutData::identity(9);
        let a = compute_anchors(&lut);
        let b = compute_anchors(&lut);
        assert_eq!(a.full_hash, b.full_hash);
        for (ra, rb) in a.region_anchors.iter().zip(b.region_anchors.iter()) {
            assert_eq!(ra.hash, rb.hash);
        }
    }
}
