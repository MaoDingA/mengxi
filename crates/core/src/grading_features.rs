use crate::color_science::ColorScienceError;

/// Grading features extracted from Oklab pixel data: histograms and color moments.
#[derive(Debug, Clone, PartialEq)]
pub struct GradingFeatures {
    /// L-channel histogram (hist_bins bins, range [0.0, 1.0]).
    pub hist_l: Vec<f64>,
    /// a-channel histogram (hist_bins bins, range [-0.5, 0.5]).
    pub hist_a: Vec<f64>,
    /// b-channel histogram (hist_bins bins, range [-0.5, 0.5]).
    pub hist_b: Vec<f64>,
    /// Color moments: [L_mean, L_std, L_skew, L_kurt, a_mean, a_std, a_skew, a_kurt, b_mean, b_std, b_skew, b_kurt].
    pub moments: [f64; 12],
}

impl GradingFeatures {
    /// Default number of histogram bins per channel.
    pub const HIST_BINS: usize = 64;
    /// Number of moments: mean + stddev + skewness + kurtosis for each of L, a, b.
    pub const MOMENTS_COUNT: usize = 12;

    /// Returns the number of bins in each histogram channel.
    pub fn hist_bins(&self) -> usize {
        self.hist_l.len()
    }

    /// BLOB size for a single histogram channel: bins * 8 bytes.
    pub fn channel_blob_size(&self) -> usize {
        self.hist_bins() * 8
    }

    /// BLOB size for moments: 12 x 8 bytes = 96.
    pub const fn moments_blob_size() -> usize {
        Self::MOMENTS_COUNT * 8
    }

    /// Total BLOB size: 3 channels * bins * 8 bytes + 12 moments * 8 bytes.
    pub fn total_blob_size(&self) -> usize {
        3 * self.hist_bins() * 8 + Self::MOMENTS_COUNT * 8
    }

    /// Compute the element-wise average of multiple GradingFeatures.
    ///
    /// Returns `None` if the input is empty or features have different bin counts.
    pub fn average(features: &[&GradingFeatures]) -> Option<GradingFeatures> {
        if features.is_empty() {
            return None;
        }
        let bins = features[0].hist_bins();
        for f in &features[1..] {
            if f.hist_bins() != bins {
                return None;
            }
        }

        let n = features.len() as f64;
        let avg_hist = |channel: fn(&GradingFeatures) -> &Vec<f64>| {
            let mut result = vec![0.0; bins];
            for f in features {
                let h = channel(f);
                for (i, v) in h.iter().enumerate() {
                    result[i] += v / n;
                }
            }
            result
        };

        let mut avg_moments = [0.0; 12];
        for f in features {
            for (i, &v) in f.moments.iter().enumerate() {
                avg_moments[i] += v / n;
            }
        }

        Some(GradingFeatures {
            hist_l: avg_hist(|f| &f.hist_l),
            hist_a: avg_hist(|f| &f.hist_a),
            hist_b: avg_hist(|f| &f.hist_b),
            moments: avg_moments,
        })
    }

    /// Serialize grading features to a little-endian BLOB.
    ///
    /// Layout: hist_l (bins*8 bytes) + hist_a (bins*8 bytes) + hist_b (bins*8 bytes) + moments (96 bytes).
    /// No header, no padding.
    ///
    /// # Panics
    /// Panics if any histogram vector length differs from the others.
    pub fn to_blob(&self) -> Vec<u8> {
        let bins = self.hist_bins();
        assert_eq!(
            self.hist_a.len(),
            bins,
            "hist_a must have {} bins, got {}",
            bins,
            self.hist_a.len()
        );
        assert_eq!(
            self.hist_b.len(),
            bins,
            "hist_b must have {} bins, got {}",
            bins,
            self.hist_b.len()
        );
        let mut blob = Vec::with_capacity(self.total_blob_size());
        Self::write_channel(&mut blob, &self.hist_l);
        Self::write_channel(&mut blob, &self.hist_a);
        Self::write_channel(&mut blob, &self.hist_b);
        for &m in &self.moments {
            blob.extend_from_slice(&m.to_le_bytes());
        }
        debug_assert_eq!(blob.len(), self.total_blob_size());
        blob
    }

    /// Deserialize grading features from a little-endian BLOB.
    ///
    /// The blob must be exactly `3 * hist_bins * 8 + 96` bytes.
    /// Returns `ColorScienceError` on size mismatch.
    pub fn from_blob(blob: &[u8], hist_bins: usize) -> Result<Self, ColorScienceError> {
        let expected = 3 * hist_bins * 8 + Self::MOMENTS_COUNT * 8;
        if blob.len() != expected {
            return Err(ColorScienceError::GradingFeatureDecodeError(format!(
                "expected {} bytes ({} bins), got {}",
                expected, hist_bins, blob.len()
            )));
        }
        let mut offset = 0;
        let hist_l = Self::read_channel_n(blob, &mut offset, hist_bins);
        let hist_a = Self::read_channel_n(blob, &mut offset, hist_bins);
        let hist_b = Self::read_channel_n(blob, &mut offset, hist_bins);
        let mut moments = [0.0_f64; Self::MOMENTS_COUNT];
        for m in &mut moments {
            *m = f64::from_le_bytes(blob[offset..offset + 8].try_into().unwrap());
            offset += 8;
        }
        debug_assert_eq!(offset, expected);
        Ok(Self {
            hist_l,
            hist_a,
            hist_b,
            moments,
        })
    }

    /// Serialize the L-channel histogram to a little-endian BLOB.
    pub fn hist_l_blob(&self) -> Vec<u8> {
        let mut blob = Vec::with_capacity(self.channel_blob_size());
        Self::write_channel(&mut blob, &self.hist_l);
        blob
    }

    /// Serialize the a-channel histogram to a little-endian BLOB.
    pub fn hist_a_blob(&self) -> Vec<u8> {
        let mut blob = Vec::with_capacity(self.channel_blob_size());
        Self::write_channel(&mut blob, &self.hist_a);
        blob
    }

    /// Serialize the b-channel histogram to a little-endian BLOB.
    pub fn hist_b_blob(&self) -> Vec<u8> {
        let mut blob = Vec::with_capacity(self.channel_blob_size());
        Self::write_channel(&mut blob, &self.hist_b);
        blob
    }

    /// Serialize the color moments to a 96-byte little-endian BLOB.
    pub fn moments_blob(&self) -> Vec<u8> {
        let mut blob = Vec::with_capacity(Self::moments_blob_size());
        for &m in &self.moments {
            blob.extend_from_slice(&m.to_le_bytes());
        }
        debug_assert_eq!(blob.len(), Self::moments_blob_size());
        blob
    }

    /// Deserialize grading features from 4 separate BLOBs.
    ///
    /// Each histogram BLOB must be exactly `hist_bins * 8` bytes; moments BLOB must be exactly 96 bytes.
    pub fn from_separate_blobs(
        hist_l: &[u8],
        hist_a: &[u8],
        hist_b: &[u8],
        moments: &[u8],
        hist_bins: usize,
    ) -> Result<Self, ColorScienceError> {
        let channel_size = hist_bins * 8;
        if hist_l.len() != channel_size {
            return Err(ColorScienceError::GradingFeatureDecodeError(format!(
                "hist_l expected {} bytes, got {}",
                channel_size,
                hist_l.len()
            )));
        }
        if hist_a.len() != channel_size {
            return Err(ColorScienceError::GradingFeatureDecodeError(format!(
                "hist_a expected {} bytes, got {}",
                channel_size,
                hist_a.len()
            )));
        }
        if hist_b.len() != channel_size {
            return Err(ColorScienceError::GradingFeatureDecodeError(format!(
                "hist_b expected {} bytes, got {}",
                channel_size,
                hist_b.len()
            )));
        }
        if moments.len() != Self::moments_blob_size() {
            return Err(ColorScienceError::GradingFeatureDecodeError(format!(
                "moments expected {} bytes, got {}",
                Self::moments_blob_size(),
                moments.len()
            )));
        }

        let mut offset = 0;
        let parsed_hist_l = Self::read_channel_n(hist_l, &mut offset, hist_bins);
        let mut offset = 0;
        let parsed_hist_a = Self::read_channel_n(hist_a, &mut offset, hist_bins);
        let mut offset = 0;
        let parsed_hist_b = Self::read_channel_n(hist_b, &mut offset, hist_bins);

        let mut parsed_moments = [0.0_f64; Self::MOMENTS_COUNT];
        for (i, m) in parsed_moments.iter_mut().enumerate() {
            *m = f64::from_le_bytes(
                moments[i * 8..i * 8 + 8]
                    .try_into()
                    .unwrap(),
            );
        }

        Ok(Self {
            hist_l: parsed_hist_l,
            hist_a: parsed_hist_a,
            hist_b: parsed_hist_b,
            moments: parsed_moments,
        })
    }

    fn write_channel(blob: &mut Vec<u8>, channel: &[f64]) {
        for &val in channel {
            blob.extend_from_slice(&val.to_le_bytes());
        }
    }

    fn read_channel_n(blob: &[u8], offset: &mut usize, bins: usize) -> Vec<f64> {
        let mut channel = Vec::with_capacity(bins);
        for _ in 0..bins {
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
            moments: [0.5, 0.2, 0.1, -0.3, -0.03, 0.15, 0.05, 2.1, 0.01, 0.08, -0.02, 0.5],
        };
        let blob = original.to_blob();
        assert_eq!(blob.len(), original.total_blob_size());
        let restored = GradingFeatures::from_blob(&blob, 64).unwrap();
        assert_eq!(restored, original);
    }

    #[test]
    fn round_trip_all_zeros() {
        let original = GradingFeatures {
            hist_l: vec![0.0; 64],
            hist_a: vec![0.0; 64],
            hist_b: vec![0.0; 64],
            moments: [0.0; 12],
        };
        let blob = original.to_blob();
        assert_eq!(blob.len(), original.total_blob_size());
        let restored = GradingFeatures::from_blob(&blob, 64).unwrap();
        assert_eq!(restored, original);
    }

    #[test]
    fn round_trip_special_values() {
        let original = GradingFeatures {
            hist_l: vec![f64::MAX; 64],
            hist_a: vec![f64::MIN; 64],
            hist_b: vec![f64::INFINITY; 64],
            moments: [f64::NEG_INFINITY, 1.0, -1.0, 0.0, 0.5, 3.14159, 2.71828, -0.5, 0.0, 0.0, 0.0, 0.0],
        };
        let blob = original.to_blob();
        let restored = GradingFeatures::from_blob(&blob, 64).unwrap();
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
            moments: [0.5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        };
        let blob = original.to_blob();
        let restored = GradingFeatures::from_blob(&blob, 64).unwrap();
        assert_eq!(restored, original);
    }

    #[test]
    fn from_blob_too_short() {
        let short_blob = vec![0u8; 100];
        let result = GradingFeatures::from_blob(&short_blob, 64);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("expected 1632 bytes"), "unexpected error: {}", msg);
        assert!(msg.contains("got 100"), "unexpected error: {}", msg);
    }

    #[test]
    fn from_blob_too_long() {
        let long_blob = vec![0u8; 2000];
        let result = GradingFeatures::from_blob(&long_blob, 64);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("expected 1632 bytes"), "unexpected error: {}", msg);
        assert!(msg.contains("got 2000"), "unexpected error: {}", msg);
    }

    #[test]
    fn from_blob_empty() {
        let result = GradingFeatures::from_blob(&[], 64);
        assert!(result.is_err());
    }

    #[test]
    fn blob_size_64_bins() {
        let features = GradingFeatures {
            hist_l: vec![0.0; 64],
            hist_a: vec![0.0; 64],
            hist_b: vec![0.0; 64],
            moments: [0.0; 12],
        };
        assert_eq!(features.total_blob_size(), 1632);
        assert_eq!(features.total_blob_size(), 3 * 64 * 8 + 12 * 8);
    }

    #[test]
    fn to_blob_exact_size() {
        let features = GradingFeatures {
            hist_l: vec![1.0; 64],
            hist_a: vec![2.0; 64],
            hist_b: vec![3.0; 64],
            moments: [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0, 1.1, 1.2],
        };
        let blob = features.to_blob();
        assert_eq!(blob.len(), features.total_blob_size());
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
            moments: [0.0; 12],
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
            moments: [f64::NAN, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        };
        let blob = original.to_blob();
        assert_eq!(blob.len(), original.total_blob_size());
        let restored = GradingFeatures::from_blob(&blob, 64).unwrap();
        // NaN != NaN, so compare BLOB bytes instead
        assert_eq!(original.to_blob(), restored.to_blob());
        // Verify the NaN bits are preserved exactly
        assert!(restored.hist_l[0].is_nan());
        assert!(restored.moments[0].is_nan());
    }

    #[test]
    fn from_blob_off_by_one_short() {
        let expected = 3 * 64 * 8 + 12 * 8;
        let blob = vec![0u8; expected - 1];
        let result = GradingFeatures::from_blob(&blob, 64);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains(&format!("expected {} bytes", expected)),
            "unexpected error: {}",
            msg
        );
    }

    #[test]
    fn from_blob_off_by_one_long() {
        let expected = 3 * 64 * 8 + 12 * 8;
        let blob = vec![0u8; expected + 1];
        let result = GradingFeatures::from_blob(&blob, 64);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains(&format!("expected {} bytes", expected)),
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
            moments: [0.75, 0.18, -0.042, 0.123, 0.007, 0.091, 0.01, -0.5, 0.03, 0.02, -0.01, 0.4],
        };
        let blob = original.to_blob();
        let restored = GradingFeatures::from_blob(&blob, 64).unwrap();

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
        assert_eq!(restored.moments[9], 0.02);
    }

    #[test]
    fn re_export_path_works() {
        use crate::color_science::GradingFeatures as ReExported;
        let gf = ReExported {
            hist_l: vec![1.0; 64],
            hist_a: vec![2.0; 64],
            hist_b: vec![3.0; 64],
            moments: [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0, 1.1, 1.2],
        };
        let blob = gf.to_blob();
        let restored = ReExported::from_blob(&blob, 64).unwrap();
        assert_eq!(restored.hist_l.len(), 64);
    }

    #[test]
    fn error_display_format_prefix() {
        let result = GradingFeatures::from_blob(&[], 64);
        let err = result.unwrap_err();
        let msg = format!("{}", err);
        assert!(
            msg.starts_with("GRADING_FEATURE_DECODE_ERROR -- "),
            "expected error prefix 'GRADING_FEATURE_DECODE_ERROR -- ', got: {}",
            msg
        );
    }

    #[test]
    #[should_panic(expected = "hist_a must have 32 bins")]
    fn to_blob_panics_on_wrong_histogram_length() {
        let bad_features = GradingFeatures {
            hist_l: vec![0.0; 32], // hist_a has 64, hist_l has 32 — mismatch
            hist_a: vec![0.0; 64],
            hist_b: vec![0.0; 64],
            moments: [0.0; 12],
        };
        let _ = bad_features.to_blob();
    }

    // --- Separate BLOB serialization tests ---

    #[test]
    fn separate_blobs_round_trip_populated() {
        let original = GradingFeatures {
            hist_l: (0..64).map(|i| i as f64 * 0.1).collect(),
            hist_a: (0..64).map(|i| (i as f64 - 32.0) * 0.01).collect(),
            hist_b: (0..64).map(|i| (63 - i) as f64 * 0.05).collect(),
            moments: [0.5, 0.2, 0.1, -0.3, -0.03, 0.15, 0.05, 2.1, 0.01, 0.08, -0.02, 0.5],
        };
        let restored = GradingFeatures::from_separate_blobs(
            &original.hist_l_blob(),
            &original.hist_a_blob(),
            &original.hist_b_blob(),
            &original.moments_blob(),
            64,
        )
        .unwrap();
        assert_eq!(restored, original);
    }

    #[test]
    fn separate_blobs_round_trip_all_zeros() {
        let original = GradingFeatures {
            hist_l: vec![0.0; 64],
            hist_a: vec![0.0; 64],
            hist_b: vec![0.0; 64],
            moments: [0.0; 12],
        };
        let restored = GradingFeatures::from_separate_blobs(
            &original.hist_l_blob(),
            &original.hist_a_blob(),
            &original.hist_b_blob(),
            &original.moments_blob(),
            64,
        )
        .unwrap();
        assert_eq!(restored, original);
    }

    #[test]
    fn separate_blobs_round_trip_special_values() {
        let original = GradingFeatures {
            hist_l: vec![f64::MAX; 64],
            hist_a: vec![f64::MIN; 64],
            hist_b: vec![f64::INFINITY; 64],
            moments: [f64::NEG_INFINITY, 1.0, -1.0, 0.0, 0.5, 3.14159, 2.71828, -0.5, 0.0, 0.0, 0.0, 0.0],
        };
        let restored = GradingFeatures::from_separate_blobs(
            &original.hist_l_blob(),
            &original.hist_a_blob(),
            &original.hist_b_blob(),
            &original.moments_blob(),
            64,
        )
        .unwrap();
        assert_eq!(restored, original);
    }

    #[test]
    fn separate_blobs_round_trip_nan() {
        let original = GradingFeatures {
            hist_l: {
                let mut h = vec![0.0; 64];
                h[0] = f64::NAN;
                h
            },
            hist_a: vec![0.0; 64],
            hist_b: vec![0.0; 64],
            moments: [f64::NAN, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        };
        let restored = GradingFeatures::from_separate_blobs(
            &original.hist_l_blob(),
            &original.hist_a_blob(),
            &original.hist_b_blob(),
            &original.moments_blob(),
            64,
        )
        .unwrap();
        // NaN != NaN, compare bytes
        assert_eq!(original.hist_l_blob(), restored.hist_l_blob());
        assert!(restored.hist_l[0].is_nan());
        assert!(restored.moments[0].is_nan());
    }

    #[test]
    fn separate_blobs_match_combined_blob() {
        let features = GradingFeatures {
            hist_l: (0..64).map(|i| i as f64 * 0.015).collect(),
            hist_a: (0..64).map(|i| (i as f64 - 30.0) * 0.008).collect(),
            hist_b: (0..64).map(|i| (50 - i) as f64 * 0.003).collect(),
            moments: [0.62, 0.21, -0.035, 0.14, 0.008, 0.077, 0.01, -0.2, 0.003, 0.015, -0.005, 0.3],
        };
        let combined_blob = features.to_blob();
        let separate_restored = GradingFeatures::from_separate_blobs(
            &features.hist_l_blob(),
            &features.hist_a_blob(),
            &features.hist_b_blob(),
            &features.moments_blob(),
            64,
        )
        .unwrap();
        let combined_restored = GradingFeatures::from_blob(&combined_blob, 64).unwrap();
        assert_eq!(separate_restored, combined_restored);
        // Verify byte-level equivalence
        let channel_size = 64 * 8;
        assert_eq!(features.hist_l_blob(), combined_blob[0..channel_size]);
        assert_eq!(
            features.hist_a_blob(),
            combined_blob[channel_size..2 * channel_size]
        );
        assert_eq!(
            features.hist_b_blob(),
            combined_blob[2 * channel_size..3 * channel_size]
        );
        assert_eq!(
            features.moments_blob(),
            combined_blob[3 * channel_size..3 * channel_size + 96]
        );
    }

    #[test]
    fn separate_blobs_wrong_size_hist_l() {
        let bad = vec![0u8; 100];
        let result = GradingFeatures::from_separate_blobs(
            &bad,
            &vec![0u8; 512],
            &vec![0u8; 512],
            &vec![0u8; 48],
            64,
        );
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("hist_l expected 512 bytes"));
    }

    #[test]
    fn separate_blobs_wrong_size_moments() {
        let result = GradingFeatures::from_separate_blobs(
            &vec![0u8; 512],
            &vec![0u8; 512],
            &vec![0u8; 512],
            &vec![0u8; 40],
            64,
        );
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("moments expected 96 bytes"));
    }

    #[test]
    fn channel_blob_exact_sizes() {
        let features = GradingFeatures {
            hist_l: vec![1.0; 64],
            hist_a: vec![2.0; 64],
            hist_b: vec![3.0; 64],
            moments: [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0, 1.1, 1.2],
        };
        assert_eq!(features.hist_l_blob().len(), features.channel_blob_size());
        assert_eq!(features.hist_a_blob().len(), features.channel_blob_size());
        assert_eq!(features.hist_b_blob().len(), features.channel_blob_size());
        assert_eq!(features.moments_blob().len(), GradingFeatures::moments_blob_size());
    }

    #[test]
    fn channel_blob_preserves_byte_order() {
        let features = GradingFeatures {
            hist_l: {
                let mut h = vec![0.0; 64];
                h[0] = 1.0; // 0x3FF0000000000000 in LE: [0,0,0,0,0,0,0xF0,0x3F]
                h
            },
            hist_a: {
                let mut h = vec![0.0; 64];
                h[1] = 2.0; // 0x4000000000000000 in LE: [0,0,0,0,0,0,0,0x40]
                h
            },
            hist_b: vec![0.0; 64],
            moments: [0.0; 12],
        };
        assert_eq!(features.hist_l_blob()[0..8], [0, 0, 0, 0, 0, 0, 0xF0, 0x3F]);
        assert_eq!(features.hist_a_blob()[8..16], [0, 0, 0, 0, 0, 0, 0, 0x40]);
    }
}
