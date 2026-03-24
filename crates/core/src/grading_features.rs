use crate::color_science::ColorScienceError;

/// Grading features extracted from Oklab pixel data: histograms and color moments.
#[derive(Debug, Clone, PartialEq)]
pub struct GradingFeatures {
    /// L-channel histogram (64 bins, range [0.0, 1.0]).
    pub hist_l: Vec<f64>,
    /// a-channel histogram (64 bins, range [-0.5, 0.5]).
    pub hist_a: Vec<f64>,
    /// b-channel histogram (64 bins, range [-0.5, 0.5]).
    pub hist_b: Vec<f64>,
    /// Color moments: [L_mean, L_std, a_mean, a_std, b_mean, b_std].
    pub moments: [f64; 6],
}

impl GradingFeatures {
    /// Number of histogram bins per channel (must match MoonBit constant).
    pub const HIST_BINS: usize = 64;
    /// Number of moments: mean + stddev for each of L, a, b.
    pub const MOMENTS_COUNT: usize = 6;
    /// Total BLOB size: 3 channels x 64 bins x 8 bytes + 6 moments x 8 bytes = 1584.
    pub const TOTAL_BLOB_SIZE: usize =
        3 * Self::HIST_BINS * 8 + Self::MOMENTS_COUNT * 8;

    /// Serialize grading features to a little-endian BLOB.
    ///
    /// Layout: hist_l (512 bytes) + hist_a (512 bytes) + hist_b (512 bytes) + moments (48 bytes).
    /// Total: 1584 bytes. No header, no padding.
    ///
    /// # Panics
    /// Panics if any histogram vector length is not `HIST_BINS` (64).
    pub fn to_blob(&self) -> Vec<u8> {
        assert_eq!(
            self.hist_l.len(),
            Self::HIST_BINS,
            "hist_l must have exactly {} bins, got {}",
            Self::HIST_BINS,
            self.hist_l.len()
        );
        assert_eq!(
            self.hist_a.len(),
            Self::HIST_BINS,
            "hist_a must have exactly {} bins, got {}",
            Self::HIST_BINS,
            self.hist_a.len()
        );
        assert_eq!(
            self.hist_b.len(),
            Self::HIST_BINS,
            "hist_b must have exactly {} bins, got {}",
            Self::HIST_BINS,
            self.hist_b.len()
        );
        let mut blob = Vec::with_capacity(Self::TOTAL_BLOB_SIZE);
        Self::write_channel(&mut blob, &self.hist_l);
        Self::write_channel(&mut blob, &self.hist_a);
        Self::write_channel(&mut blob, &self.hist_b);
        for &m in &self.moments {
            blob.extend_from_slice(&m.to_le_bytes());
        }
        debug_assert_eq!(blob.len(), Self::TOTAL_BLOB_SIZE);
        blob
    }

    /// Deserialize grading features from a little-endian BLOB.
    ///
    /// The blob must be exactly 1584 bytes. Returns `ColorScienceError` on size mismatch.
    pub fn from_blob(blob: &[u8]) -> Result<Self, ColorScienceError> {
        if blob.len() != Self::TOTAL_BLOB_SIZE {
            return Err(ColorScienceError::GradingFeatureDecodeError(format!(
                "expected {} bytes, got {}",
                Self::TOTAL_BLOB_SIZE,
                blob.len()
            )));
        }
        let mut offset = 0;
        let hist_l = Self::read_channel(blob, &mut offset);
        let hist_a = Self::read_channel(blob, &mut offset);
        let hist_b = Self::read_channel(blob, &mut offset);
        let mut moments = [0.0_f64; Self::MOMENTS_COUNT];
        for m in &mut moments {
            *m = f64::from_le_bytes(blob[offset..offset + 8].try_into().unwrap());
            offset += 8;
        }
        debug_assert_eq!(offset, Self::TOTAL_BLOB_SIZE);
        Ok(Self {
            hist_l,
            hist_a,
            hist_b,
            moments,
        })
    }

    fn write_channel(blob: &mut Vec<u8>, channel: &[f64]) {
        for &val in channel {
            blob.extend_from_slice(&val.to_le_bytes());
        }
    }

    fn read_channel(blob: &[u8], offset: &mut usize) -> Vec<f64> {
        let mut channel = Vec::with_capacity(Self::HIST_BINS);
        for _ in 0..Self::HIST_BINS {
            let val = f64::from_le_bytes(blob[*offset..*offset + 8].try_into().unwrap());
            channel.push(val);
            *offset += 8;
        }
        channel
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_populated_data() {
        let original = GradingFeatures {
            hist_l: (0..64).map(|i| i as f64 * 0.1).collect(),
            hist_a: (0..64).map(|i| (i as f64 - 32.0) * 0.01).collect(),
            hist_b: (0..64).map(|i| (63 - i) as f64 * 0.05).collect(),
            moments: [0.5, 0.2, -0.03, 0.15, 0.01, 0.08],
        };
        let blob = original.to_blob();
        assert_eq!(blob.len(), GradingFeatures::TOTAL_BLOB_SIZE);
        let restored = GradingFeatures::from_blob(&blob).unwrap();
        assert_eq!(restored, original);
    }

    #[test]
    fn round_trip_all_zeros() {
        let original = GradingFeatures {
            hist_l: vec![0.0; 64],
            hist_a: vec![0.0; 64],
            hist_b: vec![0.0; 64],
            moments: [0.0; 6],
        };
        let blob = original.to_blob();
        assert_eq!(blob.len(), GradingFeatures::TOTAL_BLOB_SIZE);
        let restored = GradingFeatures::from_blob(&blob).unwrap();
        assert_eq!(restored, original);
    }

    #[test]
    fn round_trip_special_values() {
        let original = GradingFeatures {
            hist_l: vec![f64::MAX; 64],
            hist_a: vec![f64::MIN; 64],
            hist_b: vec![f64::INFINITY; 64],
            moments: [f64::NEG_INFINITY, 1.0, -1.0, 0.0, 3.14159, 2.71828],
        };
        let blob = original.to_blob();
        let restored = GradingFeatures::from_blob(&blob).unwrap();
        assert_eq!(restored, original);
    }

    #[test]
    fn round_trip_single_pixel_histogram() {
        // Simulate single-pixel extraction: one bin has count 1, rest 0
        let mut hist_l = vec![0.0; 64];
        hist_l[32] = 1.0;
        let original = GradingFeatures {
            hist_l: hist_l.clone(),
            hist_a: {
                let mut h = vec![0.0; 64];
                h[32] = 1.0;
                h
            },
            hist_b: {
                let mut h = vec![0.0; 64];
                h[32] = 1.0;
                h
            },
            moments: [0.5, 0.0, 0.0, 0.0, 0.0, 0.0],
        };
        let blob = original.to_blob();
        let restored = GradingFeatures::from_blob(&blob).unwrap();
        assert_eq!(restored, original);
    }

    #[test]
    fn from_blob_too_short() {
        let short_blob = vec![0u8; 100];
        let result = GradingFeatures::from_blob(&short_blob);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("expected 1584 bytes"), "unexpected error: {}", msg);
        assert!(msg.contains("got 100"), "unexpected error: {}", msg);
    }

    #[test]
    fn from_blob_too_long() {
        let long_blob = vec![0u8; 2000];
        let result = GradingFeatures::from_blob(&long_blob);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("expected 1584 bytes"), "unexpected error: {}", msg);
        assert!(msg.contains("got 2000"), "unexpected error: {}", msg);
    }

    #[test]
    fn from_blob_empty() {
        let result = GradingFeatures::from_blob(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn blob_size_constant() {
        assert_eq!(GradingFeatures::TOTAL_BLOB_SIZE, 1584);
        assert_eq!(GradingFeatures::TOTAL_BLOB_SIZE, 3 * 64 * 8 + 6 * 8);
    }

    #[test]
    fn to_blob_exact_size() {
        let features = GradingFeatures {
            hist_l: vec![1.0; 64],
            hist_a: vec![2.0; 64],
            hist_b: vec![3.0; 64],
            moments: [0.1, 0.2, 0.3, 0.4, 0.5, 0.6],
        };
        let blob = features.to_blob();
        assert_eq!(blob.len(), GradingFeatures::TOTAL_BLOB_SIZE);
    }

    #[test]
    fn blob_preserves_byte_order() {
        // Verify little-endian encoding by checking specific bytes
        let features = GradingFeatures {
            hist_l: {
                let mut h = vec![0.0; 64];
                h[0] = 1.0; // 0x3FF0000000000000 in LE: [0,0,0,0,0,0,0xF0,0x3F]
                h
            },
            hist_a: vec![0.0; 64],
            hist_b: vec![0.0; 64],
            moments: [0.0; 6],
        };
        let blob = features.to_blob();
        // First 8 bytes should be 1.0 in little-endian
        assert_eq!(blob[0..8], [0, 0, 0, 0, 0, 0, 0xF0, 0x3F]);
    }

    #[test]
    fn round_trip_nan_values() {
        // NaN round-trip: NaN != NaN via PartialEq, so verify byte-exact BLOB
        let original = GradingFeatures {
            hist_l: {
                let mut h = vec![0.0; 64];
                h[0] = f64::NAN;
                h[1] = f64::NAN;
                h
            },
            hist_a: vec![0.0; 64],
            hist_b: vec![0.0; 64],
            moments: [f64::NAN, 0.0, 0.0, 0.0, 0.0, 0.0],
        };
        let blob = original.to_blob();
        assert_eq!(blob.len(), GradingFeatures::TOTAL_BLOB_SIZE);
        let restored = GradingFeatures::from_blob(&blob).unwrap();
        // NaN != NaN, so compare BLOB bytes instead
        assert_eq!(original.to_blob(), restored.to_blob());
        // Verify the NaN bits are preserved exactly
        assert!(restored.hist_l[0].is_nan());
        assert!(restored.moments[0].is_nan());
    }

    #[test]
    fn from_blob_off_by_one_short() {
        let blob = vec![0u8; GradingFeatures::TOTAL_BLOB_SIZE - 1];
        let result = GradingFeatures::from_blob(&blob);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains(&format!("expected {} bytes", GradingFeatures::TOTAL_BLOB_SIZE)),
            "unexpected error: {}",
            msg
        );
    }

    #[test]
    fn from_blob_off_by_one_long() {
        let blob = vec![0u8; GradingFeatures::TOTAL_BLOB_SIZE + 1];
        let result = GradingFeatures::from_blob(&blob);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains(&format!("expected {} bytes", GradingFeatures::TOTAL_BLOB_SIZE)),
            "unexpected error: {}",
            msg
        );
    }

    #[test]
    fn deserialized_field_values_independent() {
        let original = GradingFeatures {
            hist_l: (0..64).map(|i| (i + 1) as f64 * 0.01).collect(),
            hist_a: (0..64).map(|i| (i as f64 - 32.0) * 0.005).collect(),
            hist_b: (0..64).map(|i| (63 - i) as f64 * 0.02).collect(),
            moments: [0.75, 0.18, -0.042, 0.123, 0.007, 0.091],
        };
        let blob = original.to_blob();
        let restored = GradingFeatures::from_blob(&blob).unwrap();

        // Verify each field independently, not just struct equality
        assert_eq!(restored.hist_l.len(), GradingFeatures::HIST_BINS);
        assert_eq!(restored.hist_a.len(), GradingFeatures::HIST_BINS);
        assert_eq!(restored.hist_b.len(), GradingFeatures::HIST_BINS);
        assert_eq!(restored.moments.len(), GradingFeatures::MOMENTS_COUNT);

        // Spot-check specific bin values
        assert_eq!(restored.hist_l[0], 0.01);
        assert_eq!(restored.hist_l[63], 0.64);
        assert_eq!(restored.hist_a[0], -0.16);
        assert_eq!(restored.hist_a[32], 0.0);
        assert_eq!(restored.hist_b[0], 1.26);
        assert_eq!(restored.hist_b[63], 0.0);

        // Spot-check moments
        assert_eq!(restored.moments[0], 0.75);
        assert_eq!(restored.moments[3], 0.123);
        assert_eq!(restored.moments[5], 0.091);
    }

    #[test]
    fn re_export_path_works() {
        use crate::color_science::GradingFeatures as ReExported;
        let gf = ReExported {
            hist_l: vec![1.0; 64],
            hist_a: vec![2.0; 64],
            hist_b: vec![3.0; 64],
            moments: [0.1, 0.2, 0.3, 0.4, 0.5, 0.6],
        };
        let blob = gf.to_blob();
        let restored = ReExported::from_blob(&blob).unwrap();
        assert_eq!(restored.hist_l.len(), 64);
    }

    #[test]
    fn error_display_format_prefix() {
        let result = GradingFeatures::from_blob(&[]);
        let err = result.unwrap_err();
        let msg = format!("{}", err);
        assert!(
            msg.starts_with("GRADING_FEATURE_DECODE_ERROR -- "),
            "expected error prefix 'GRADING_FEATURE_DECODE_ERROR -- ', got: {}",
            msg
        );
    }

    #[test]
    #[should_panic(expected = "hist_l must have exactly 64 bins")]
    fn to_blob_panics_on_wrong_histogram_length() {
        let bad_features = GradingFeatures {
            hist_l: vec![0.0; 32], // wrong length
            hist_a: vec![0.0; 64],
            hist_b: vec![0.0; 64],
            moments: [0.0; 6],
        };
        let _ = bad_features.to_blob();
    }
}
