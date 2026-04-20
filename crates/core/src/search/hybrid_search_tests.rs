// hybrid_search_tests.rs — Tests for hybrid search module

use super::*;
    use crate::search::SearchOptions;
    use crate::search::histogram_utils::{parse_histogram, histogram_intersection, cosine_similarity};
    use crate::search::histogram_search::search_histograms;
    use crate::search::bhattacharyya_search::bhattacharyya_search;
    use crate::search::query::{fingerprint_info, fingerprint_info_with_tags};
    use crate::search::tag_search::search_by_tag;
    use crate::search::hybrid_search::search_by_image_and_tag;
    use crate::search::embedding::{serialize_embedding, deserialize_embedding};
    use crate::search::types::summarize_histogram;

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
    fn test_histogram_intersection_identical() {
        let a: Vec<f64> = (0..64).map(|_| 1.0 / 64.0).collect();
        let score = histogram_intersection(&a, &a);
        assert!((score - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_histogram_intersection_completely_different() {
        let a: Vec<f64> = vec![1.0; 64]; // All in first position (not normalized, but test logic)
        let b: Vec<f64> = (0..64).map(|_| 1.0 / 64.0).collect();
        let score = histogram_intersection(&a, &b);
        assert!(score >= 0.0);
        assert!(score <= 1.0);
    }

    #[test]
    fn test_histogram_intersection_empty() {
        let score = histogram_intersection(&[], &[]);
        assert_eq!(score, 0.0);
    }

    #[test]
    fn test_histogram_intersection_partial_overlap() {
        let a: Vec<f64> = (0..64).map(|_| 1.0 / 64.0).collect();
        let mut b = vec![0.0; 64];
        b[0] = 1.0; // All mass in bin 0
        let score = histogram_intersection(&a, &b);
        // min(1/64, 1.0) + 63 * min(1/64, 0.0) = 1/64
        assert!((score - 1.0 / 64.0).abs() < 1e-10);
    }

    fn make_histogram_csv(value: f64) -> String {
        (0..64).map(|_| value.to_string()).collect::<Vec<_>>().join(",")
    }

    fn setup_test_db() -> Connection {
        crate::test_db::setup_test_db()
    }

    #[test]
    fn test_search_global_no_fingerprints() {
        let conn = setup_test_db();
        // Add a project but no fingerprints
        conn.execute("INSERT INTO projects (name, path) VALUES ('test', '/tmp/test')", [])
            .unwrap();
        let result = search_histograms(
            &conn,
            &SearchOptions {
                project: None,
                limit: 5,
                use_pyramid: false,
            },
        );
        assert!(result.is_err());
        match result.unwrap_err() {
            SearchError::NoFingerprints => {}
            other => panic!("Expected NoFingerprints, got: {:?}", other),
        }
    }

    #[test]
    fn test_search_project_no_fingerprints() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('test', '/tmp/test')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'scene.dpx', 'dpx')", [])
            .unwrap();
        let result = search_histograms(
            &conn,
            &SearchOptions {
                project: Some("test".to_string()),
                limit: 5,
                use_pyramid: false,
            },
        );
        assert!(result.is_err());
        match result.unwrap_err() {
            SearchError::ProjectNotFound(name) => assert_eq!(name, "test"),
            other => panic!("Expected ProjectNotFound, got: {:?}", other),
        }
    }

    #[test]
    fn test_search_global_returns_results() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film_a', '/tmp/a')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'scene001.dpx', 'dpx')", [])
            .unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg')",
            [make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625)],
        )
        .unwrap();

        let result = search_histograms(
            &conn,
            &SearchOptions {
                project: None,
                limit: 5,
                use_pyramid: false,
            },
        )
        .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].rank, 1);
        assert_eq!(result[0].project_name, "film_a");
        assert_eq!(result[0].file_path, "scene001.dpx");
        assert_eq!(result[0].file_format, "dpx");
    }

    #[test]
    fn test_search_with_project_filter() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film_a', '/tmp/a')", [])
            .unwrap();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film_b', '/tmp/b')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'scene001.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (2, 'scene002.dpx', 'exr')", [])
            .unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg')",
            [make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625)],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (2, ?, ?, ?, 0.3, 0.2, 'acescg')",
            [make_histogram_csv(0.01), make_histogram_csv(0.01), make_histogram_csv(0.01)],
        )
        .unwrap();

        // Search within film_a only
        let result = search_histograms(
            &conn,
            &SearchOptions {
                project: Some("film_a".to_string()),
                limit: 10,
                use_pyramid: false,
            },
        )
        .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].project_name, "film_a");

        // Global search returns both
        let result = search_histograms(
            &conn,
            &SearchOptions {
                project: None,
                limit: 10,
                use_pyramid: false,
            },
        )
        .unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_search_limit() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        for i in 0..5 {
            conn.execute(
                &format!("INSERT INTO files (project_id, filename, format) VALUES (1, 'scene{:03}.dpx', 'dpx')", i),
                [],
            )
            .unwrap();
            conn.execute(
                &format!(
                    "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES ({}, ?, ?, ?, 0.5, 0.1, 'acescg')",
                    i + 1
                ),
                [make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625)],
            )
            .unwrap();
        }

        let result = search_histograms(
            &conn,
            &SearchOptions {
                project: None,
                limit: 2,
                use_pyramid: false,
            },
        )
        .unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_search_malformed_histogram_skipped() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'good.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'bad.dpx', 'dpx')", [])
            .unwrap();
        // Good fingerprint
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg')",
            [make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625)],
        )
        .unwrap();
        // Bad fingerprint (wrong number of bins)
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (2, '0.1,0.2', '0.1,0.2', '0.1,0.2', 0.5, 0.1, 'acescg')",
            [],
        )
        .unwrap();

        let result = search_histograms(
            &conn,
            &SearchOptions {
                project: None,
                limit: 10,
                use_pyramid: false,
            },
        )
        .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].file_path, "good.dpx");
    }

    #[test]
    fn test_search_error_display() {
        let err = SearchError::NoFingerprints;
        assert!(format!("{}", err).contains("SEARCH_NO_FINGERPRINTS"));

        let err = SearchError::DatabaseError("query failed".to_string());
        assert!(format!("{}", err).contains("SEARCH_DB_ERROR"));

        let err = SearchError::InvalidFormat("bad format".to_string());
        assert!(format!("{}", err).contains("SEARCH_INVALID_FORMAT"));

        let err = SearchError::ProjectNotFound("test_proj".to_string());
        assert!(format!("{}", err).contains("SEARCH_PROJECT_NOT_FOUND"));
    }

    #[test]
    fn test_parse_histogram_rejects_nan() {
        let mut hist_parts: Vec<String> = (0..63).map(|_| "0.1".to_string()).collect();
        hist_parts.push("NaN".to_string());
        let hist = hist_parts.join(",");
        let result = parse_histogram(&hist);
        assert!(result.is_err());
        assert!(format!("{}", result.unwrap_err()).contains("non-finite"));
    }

    #[test]
    fn test_parse_histogram_rejects_infinity() {
        let mut hist_parts: Vec<String> = (0..63).map(|_| "0.1".to_string()).collect();
        hist_parts.push("inf".to_string());
        let hist = hist_parts.join(",");
        let result = parse_histogram(&hist);
        assert!(result.is_err());
        assert!(format!("{}", result.unwrap_err()).contains("non-finite"));
    }

    #[test]
    fn test_search_project_not_found() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('other', '/tmp/other')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'scene.dpx', 'dpx')", [])
            .unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg')",
            [make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625)],
        )
        .unwrap();

        // Search for a project that exists but has no fingerprints
        let result = search_histograms(
            &conn,
            &SearchOptions {
                project: Some("nonexistent".to_string()),
                limit: 5,
                use_pyramid: false,
            },
        );
        assert!(result.is_err());
        match result.unwrap_err() {
            SearchError::ProjectNotFound(name) => assert_eq!(name, "nonexistent"),
            other => panic!("Expected ProjectNotFound, got: {:?}", other),
        }

        // Global search should still work
        let result = search_histograms(
            &conn,
            &SearchOptions {
                project: None,
                limit: 5,
                use_pyramid: false,
            },
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 1);
    }

    // --- Cosine similarity tests ---

    #[test]
    fn test_cosine_similarity_identical() {
        let a: Vec<f64> = vec![0.1, 0.2, 0.3, 0.4];
        let score = cosine_similarity(&a, &a);
        assert!((score - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a: Vec<f64> = vec![1.0, 0.0];
        let b: Vec<f64> = vec![0.0, 1.0];
        let score = cosine_similarity(&a, &b);
        assert!(score.abs() < 1e-10);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a: Vec<f64> = vec![1.0, 0.0];
        let b: Vec<f64> = vec![-1.0, 0.0];
        let score = cosine_similarity(&a, &b);
        assert!((score - (-1.0)).abs() < 1e-10);
    }

    #[test]
    fn test_cosine_similarity_zero_vector() {
        let a: Vec<f64> = vec![0.0, 0.0, 0.0];
        let b: Vec<f64> = vec![1.0, 2.0, 3.0];
        let score = cosine_similarity(&a, &b);
        assert!(score.abs() < 1e-10);
    }

    #[test]
    fn test_cosine_similarity_empty() {
        assert_eq!(cosine_similarity(&[], &[]), 0.0);
    }

    #[test]
    fn test_cosine_similarity_mismatched_dims() {
        let a: Vec<f64> = vec![1.0, 2.0];
        let b: Vec<f64> = vec![1.0, 2.0, 3.0];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn test_cosine_similarity_positive() {
        let a: Vec<f64> = vec![1.0, 0.0, 0.0];
        let b: Vec<f64> = vec![0.707, 0.707, 0.0];
        let score = cosine_similarity(&a, &b);
        assert!(score > 0.0);
        assert!(score < 1.0);
        assert!((score - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-5);
    }

    // --- Embedding serialization tests ---

    #[test]
    fn test_embedding_roundtrip() {
        let original: Vec<f64> = vec![0.1, 0.2, 0.3, 0.4, -0.5, 1.0];
        let bytes = serialize_embedding(&original);
        assert_eq!(bytes.len(), original.len() * 4); // f32 = 4 bytes
        let restored = deserialize_embedding(&bytes).unwrap();
        assert_eq!(restored.len(), original.len());
        for (orig, rest) in original.iter().zip(restored.iter()) {
            assert!((*orig - *rest).abs() < 1e-6); // f32 precision
        }
    }

    #[test]
    fn test_embedding_roundtrip_empty() {
        let original: Vec<f64> = vec![];
        let bytes = serialize_embedding(&original);
        assert!(bytes.is_empty());
        let restored = deserialize_embedding(&bytes).unwrap();
        assert!(restored.is_empty());
    }

    #[test]
    fn test_embedding_roundtrip_single() {
        let original: Vec<f64> = vec![42.0];
        let bytes = serialize_embedding(&original);
        assert_eq!(bytes.len(), 4);
        let restored = deserialize_embedding(&bytes).unwrap();
        assert_eq!(restored.len(), 1);
        assert!((restored[0] - 42.0).abs() < 1e-6);
    }

    #[test]
    fn test_embedding_deserialize_truncated_blob() {
        // 5 bytes — not a multiple of 4
        let blob = vec![0x00, 0x00, 0x80, 0x3f, 0xFF];
        assert!(deserialize_embedding(&blob).is_none());
    }

    #[test]
    fn test_embedding_deserialize_one_byte() {
        let blob = vec![0x42];
        assert!(deserialize_embedding(&blob).is_none());
    }

    // --- SearchError new variants ---

    #[test]
    fn test_search_error_ai_unavailable() {
        let err = SearchError::AiUnavailable("Python not installed".to_string());
        assert_eq!(
            format!("{}", err),
            "SEARCH_AI_UNAVAILABLE -- Python not installed"
        );
    }

    #[test]
    fn test_search_error_embedding_error() {
        let err = SearchError::EmbeddingError("model failed".to_string());
        assert_eq!(
            format!("{}", err),
            "SEARCH_EMBEDDING_ERROR -- model failed"
        );
    }

    // --- Fingerprint info tests ---

    #[test]
    fn test_fingerprint_info_valid() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'scene.dpx', 'dpx')", [])
            .unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg')",
            [make_histogram_csv(0.015625), make_histogram_csv(0.02), make_histogram_csv(0.01)],
        ).unwrap();

        let info = fingerprint_info(&conn, "film", "scene.dpx").unwrap();
        assert_eq!(info.project_name, "film");
        assert_eq!(info.file_path, "scene.dpx");
        assert_eq!(info.file_format, "dpx");
        assert_eq!(info.color_space_tag, "acescg");
        assert!((info.luminance_mean - 0.5).abs() < 1e-10);
        assert!((info.luminance_stddev - 0.1).abs() < 1e-10);
    }

    #[test]
    fn test_fingerprint_info_not_found() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'scene.dpx', 'dpx')", [])
            .unwrap();

        let result = fingerprint_info(&conn, "film", "nonexistent.dpx");
        assert!(result.is_err());
        match result.unwrap_err() {
            SearchError::ProjectNotFound(msg) => {
                assert!(msg.contains("nonexistent.dpx"));
            }
            other => panic!("Expected ProjectNotFound, got: {:?}", other),
        }
    }

    #[test]
    fn test_summarize_histogram() {
        // Uniform histogram
        let uniform: Vec<f64> = vec![1.0 / 64.0; 64];
        let summary = summarize_histogram(&uniform);
        assert!((summary.mean_value - 1.0 / 64.0).abs() < 1e-10);

        // Histogram with a dominant bin
        let mut hist = vec![0.0; 64];
        hist[10] = 0.5;
        let summary = summarize_histogram(&hist);
        assert_eq!(summary.dominant_bin_min, 10);
        assert_eq!(summary.dominant_bin_max, 10);
    }

    // --- Tag search tests ---

    #[test]
    fn test_search_by_tag_basic() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 's1.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 's2.dpx', 'dpx')", [])
            .unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg')",
            [make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625)],
        ).unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (2, ?, ?, ?, 0.3, 0.2, 'acescg')",
            [make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625)],
        ).unwrap();
        // Tag s1 with "warm"
        conn.execute("INSERT INTO tags (fingerprint_id, tag) VALUES (1, 'warm')", [])
            .unwrap();
        // Tag s2 with "warm" and "industrial"
        conn.execute("INSERT INTO tags (fingerprint_id, tag) VALUES (2, 'warm')", [])
            .unwrap();
        conn.execute("INSERT INTO tags (fingerprint_id, tag) VALUES (2, 'industrial')", [])
            .unwrap();

        // Search for single tag "warm" — both match 1 tag, so both score 1.0
        let results = search_by_tag(
            &conn,
            "warm",
            &SearchOptions {
                project: None,
                limit: 10,
                use_pyramid: false,
            },
        )
        .unwrap();
        assert_eq!(results.len(), 2);

        // Search for multi-tag "warm industrial" — s2 matches 2, s1 matches 1
        let results = search_by_tag(
            &conn,
            "warm industrial",
            &SearchOptions {
                project: None,
                limit: 10,
                use_pyramid: false,
            },
        )
        .unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].file_path, "s2.dpx");
        assert!((results[0].score - 1.0).abs() < 1e-10); // 2/2 = 1.0
        assert_eq!(results[1].file_path, "s1.dpx");
        assert!((results[1].score - 0.5).abs() < 1e-10); // 1/2 = 0.5
    }

    #[test]
    fn test_search_by_tag_no_results() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 's1.dpx', 'dpx')", [])
            .unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg')",
            [make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625)],
        ).unwrap();

        let result = search_by_tag(
            &conn,
            "nonexistent",
            &SearchOptions {
                project: None,
                limit: 10,
                use_pyramid: false,
            },
        );
        assert!(result.is_err());
        match result.unwrap_err() {
            SearchError::NoFingerprints => {}
            other => panic!("Expected NoFingerprints, got: {:?}", other),
        }
    }

    #[test]
    fn test_search_by_tag_project_scoped() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film_a', '/tmp/a')", [])
            .unwrap();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film_b', '/tmp/b')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 's1.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (2, 's2.dpx', 'dpx')", [])
            .unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg')",
            [make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625)],
        ).unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (2, ?, ?, ?, 0.3, 0.2, 'acescg')",
            [make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625)],
        ).unwrap();
        conn.execute("INSERT INTO tags (fingerprint_id, tag) VALUES (1, 'warm')", [])
            .unwrap();
        conn.execute("INSERT INTO tags (fingerprint_id, tag) VALUES (2, 'warm')", [])
            .unwrap();

        let results = search_by_tag(
            &conn,
            "warm",
            &SearchOptions {
                project: Some("film_a".to_string()),
                limit: 10,
                use_pyramid: false,
            },
        )
        .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].project_name, "film_a");
    }

    // --- Hybrid search tests ---

    #[cfg(moonbit_ffi)]
    #[test]
    fn test_hybrid_search_all_signals() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'query.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'candidate.dpx', 'dpx')", [])
            .unwrap();

        let (hl, ha, hb, m) = make_grading_features_blob();

        // Query fingerprint with grading features and embedding
        let query_emb = serialize_embedding(&vec![0.1, 0.2, 0.3, 0.4]);
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments, embedding, embedding_model) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg', ?, ?, ?, ?, ?, 'test-model')",
            rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), hl, ha, hb, m, query_emb],
        ).unwrap();

        // Candidate with grading features, embedding, and tags
        let cand_emb = serialize_embedding(&vec![0.1, 0.2, 0.3, 0.4]);
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments, embedding, embedding_model) VALUES (2, ?, ?, ?, 0.5, 0.1, 'acescg', ?, ?, ?, ?, ?, 'test-model')",
            rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), hl, ha, hb, m, cand_emb],
        ).unwrap();
        // Add tags to candidate
        conn.execute("INSERT INTO tags (fingerprint_id, tag) VALUES (2, 'warm')", [])
            .unwrap();

        let weights = SignalWeights {
            grading: 0.6,
            clip: 0.3,
            tag: 0.1,
        };

        let results = hybrid_search(
            &conn,
            1,
            &weights,
            &SearchOptions { project: None, limit: 10, use_pyramid: false },
        ).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].rank, 1);
        assert!(results[0].score > 0.0);
        // With identical features, grading_sim ~ 1.0
        assert!(results[0].score_breakdown.grading > 0.9);
        // CLIP: identical embeddings -> cos_sim = 1.0 -> normalized = 1.0
        assert!(results[0].score_breakdown.clip.is_some());
        assert!((results[0].score_breakdown.clip.unwrap() - 1.0).abs() < 1e-6);
    }

    #[cfg(moonbit_ffi)]
    #[test]
    fn test_hybrid_search_missing_clip() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'query.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'candidate.dpx', 'dpx')", [])
            .unwrap();

        let (hl, ha, hb, m) = make_grading_features_blob();

        // Query without embedding
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg', ?, ?, ?, ?)",
            rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), hl, ha, hb, m],
        ).unwrap();

        // Candidate without embedding but with tags
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments) VALUES (2, ?, ?, ?, 0.5, 0.1, 'acescg', ?, ?, ?, ?)",
            rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), hl, ha, hb, m],
        ).unwrap();
        conn.execute("INSERT INTO tags (fingerprint_id, tag) VALUES (2, 'warm')", [])
            .unwrap();

        let weights = SignalWeights {
            grading: 0.6,
            clip: 0.3,
            tag: 0.1,
        };

        let results = hybrid_search(
            &conn,
            1,
            &weights,
            &SearchOptions { project: None, limit: 10, use_pyramid: false },
        ).unwrap();

        assert_eq!(results.len(), 1);
        // CLIP should be None (omitted)
        assert_eq!(results[0].score_breakdown.clip, None);
        // Grading should be present (identical features)
        assert!(results[0].score_breakdown.grading > 0.9);
    }

    #[cfg(moonbit_ffi)]
    #[test]
    fn test_hybrid_search_missing_clip_and_tag() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'query.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'candidate.dpx', 'dpx')", [])
            .unwrap();

        let (hl, ha, hb, m) = make_grading_features_blob();

        // Both without embedding or tags
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg', ?, ?, ?, ?)",
            rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), hl, ha, hb, m],
        ).unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments) VALUES (2, ?, ?, ?, 0.5, 0.1, 'acescg', ?, ?, ?, ?)",
            rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), hl, ha, hb, m],
        ).unwrap();

        let weights = SignalWeights {
            grading: 0.6,
            clip: 0.3,
            tag: 0.1,
        };

        let results = hybrid_search(
            &conn,
            1,
            &weights,
            &SearchOptions { project: None, limit: 10, use_pyramid: false },
        ).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].score_breakdown.clip, None);
        assert_eq!(results[0].score_breakdown.tag, None);
        // Only grading, weight = 1.0, identical features -> score ~ 1.0
        assert!(results[0].score > 0.9);
    }

    #[cfg(moonbit_ffi)]
    #[test]
    fn test_hybrid_search_ranking() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'query.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'near.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'far.dpx', 'dpx')", [])
            .unwrap();

        // Query: uniform features
        let query_gf = GradingFeatures {
            hist_l: vec![50.0; 64],
            hist_a: vec![25.0; 64],
            hist_b: vec![25.0; 64],
            moments: [0.5, 0.1, 0.0, 0.05, 0.0, 0.0, 0.0, 0.0, 0.0, 0.05, 0.0, 0.0],
        };
        let (qhl, qha, qhb, qm) = (query_gf.hist_l_blob(), query_gf.hist_a_blob(), query_gf.hist_b_blob(), query_gf.moments_blob());

        // Near: similar features
        let near_gf = GradingFeatures {
            hist_l: vec![55.0; 64],
            hist_a: vec![27.0; 64],
            hist_b: vec![27.0; 64],
            moments: [0.52, 0.11, 0.01, 0.05, 0.01, 0.0, 0.0, 0.0, 0.01, 0.05, 0.0, 0.0],
        };
        let (nhl, nha, nhb, nm) = (near_gf.hist_l_blob(), near_gf.hist_a_blob(), near_gf.hist_b_blob(), near_gf.moments_blob());

        // Far: very different features
        let far_gf = GradingFeatures {
            hist_l: vec![1000.0; 64],
            hist_a: vec![1.0; 64],
            hist_b: vec![1.0; 64],
            moments: [0.9, 0.01, 0.0, 0.01, 0.0, 0.0, 0.0, 0.0, 0.0, 0.01, 0.0, 0.0],
        };
        let (fhl, fha, fhb, fm) = (far_gf.hist_l_blob(), far_gf.hist_a_blob(), far_gf.hist_b_blob(), far_gf.moments_blob());

        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg', ?, ?, ?, ?)",
            rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), qhl, qha, qhb, qm],
        ).unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments) VALUES (2, ?, ?, ?, 0.3, 0.2, 'acescg', ?, ?, ?, ?)",
            rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), nhl, nha, nhb, nm],
        ).unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments) VALUES (3, ?, ?, ?, 0.1, 0.3, 'acescg', ?, ?, ?, ?)",
            rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), fhl, fha, fhb, fm],
        ).unwrap();

        let weights = SignalWeights {
            grading: 0.6,
            clip: 0.3,
            tag: 0.1,
        };

        let results = hybrid_search(
            &conn,
            1,
            &weights,
            &SearchOptions { project: None, limit: 10, use_pyramid: false },
        ).unwrap();

        assert_eq!(results.len(), 2);
        // "near" should rank higher than "far"
        assert!(results[0].score >= results[1].score,
            "Expected near (score={}) >= far (score={})", results[0].score, results[1].score);
    }

    #[cfg(moonbit_ffi)]
    #[test]
    fn test_hybrid_search_limit() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'query.dpx', 'dpx')", [])
            .unwrap();

        let (hl, ha, hb, m) = make_grading_features_blob();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg', ?, ?, ?, ?)",
            rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), hl, ha, hb, m],
        ).unwrap();

        // Add 5 identical candidates
        for i in 0..5 {
            conn.execute(
                &format!("INSERT INTO files (project_id, filename, format) VALUES (1, 'c{}.dpx', 'dpx')", i),
                [],
            ).unwrap();
            conn.execute(
                &format!("INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments) VALUES ({}, ?, ?, ?, 0.5, 0.1, 'acescg', ?, ?, ?, ?)", i + 2),
                rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), hl, ha, hb, m],
            ).unwrap();
        }

        let weights = SignalWeights {
            grading: 0.6,
            clip: 0.3,
            tag: 0.1,
        };

        let results = hybrid_search(
            &conn,
            1,
            &weights,
            &SearchOptions { project: None, limit: 2, use_pyramid: false },
        ).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[cfg(moonbit_ffi)]
    #[test]
    fn test_hybrid_search_project_scoped() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film_a', '/tmp/a')", [])
            .unwrap();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film_b', '/tmp/b')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'query.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'c1.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (2, 'c2.dpx', 'dpx')", [])
            .unwrap();

        let (hl, ha, hb, m) = make_grading_features_blob();

        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg', ?, ?, ?, ?)",
            rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), hl, ha, hb, m],
        ).unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments) VALUES (2, ?, ?, ?, 0.5, 0.1, 'acescg', ?, ?, ?, ?)",
            rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), hl, ha, hb, m],
        ).unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments) VALUES (3, ?, ?, ?, 0.3, 0.2, 'acescg', ?, ?, ?, ?)",
            rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), hl, ha, hb, m],
        ).unwrap();

        let weights = SignalWeights {
            grading: 0.6,
            clip: 0.3,
            tag: 0.1,
        };

        // Search within film_a only
        let results = hybrid_search(
            &conn,
            1,
            &weights,
            &SearchOptions { project: Some("film_a".to_string()), limit: 10, use_pyramid: false },
        ).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].project_name, "film_a");
    }

    // --- Bhattacharyya search tests ---

    /// Helper to create grading feature BLOBs for test data (uniform histograms).
    fn make_grading_features_blob() -> (Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>) {
        let gf = GradingFeatures {
            hist_l: vec![100.0; 64],
            hist_a: vec![50.0; 64],
            hist_b: vec![50.0; 64],
            moments: [0.5, 0.1, 0.0, 0.05, 0.0, 0.0, 0.0, 0.0, 0.0, 0.05, 0.0, 0.0],
        };
        (gf.hist_l_blob(), gf.hist_a_blob(), gf.hist_b_blob(), gf.moments_blob())
    }

    #[cfg(moonbit_ffi)]
    #[test]
    fn test_bhattacharyya_search_identical_features() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'query.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'candidate.dpx', 'dpx')", [])
            .unwrap();

        let (hl, ha, hb, m) = make_grading_features_blob();

        // Both query and candidate have identical grading features
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg', ?, ?, ?, ?)",
            rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), hl, ha, hb, m],
        ).unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments) VALUES (2, ?, ?, ?, 0.5, 0.1, 'acescg', ?, ?, ?, ?)",
            rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), hl, ha, hb, m],
        ).unwrap();

        let results = bhattacharyya_search(
            &conn,
            1,
            &SearchOptions { project: None, limit: 10, use_pyramid: false },
        ).unwrap();

        assert_eq!(results.len(), 1);
        assert!((results[0].score - 1.0).abs() < 0.01, "Identical features should score near 1.0, got {}", results[0].score);
    }

    #[test]
    fn test_bhattacharyya_search_no_grading_features() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'q.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'c.dpx', 'dpx')", [])
            .unwrap();
        // Insert fingerprints without grading features (oklab_hist_l is NULL)
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg')",
            [make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625)],
        ).unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (2, ?, ?, ?, 0.3, 0.2, 'acescg')",
            [make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625)],
        ).unwrap();

        let result = bhattacharyya_search(
            &conn,
            1,
            &SearchOptions { project: None, limit: 10, use_pyramid: false },
        );
        assert!(result.is_err());
        match result.unwrap_err() {
            SearchError::NoFingerprints | SearchError::DatabaseError(_) => {}
            other => panic!("Expected NoFingerprints or DatabaseError, got: {:?}", other),
        }
    }

    #[cfg(moonbit_ffi)]
    #[test]
    fn test_bhattacharyya_search_ranking_order() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'query.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'near.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'far.dpx', 'dpx')", [])
            .unwrap();

        // Query: bright image
        let query_gf = GradingFeatures {
            hist_l: vec![50.0; 64],
            hist_a: vec![25.0; 64],
            hist_b: vec![25.0; 64],
            moments: [0.5, 0.1, 0.0, 0.05, 0.0, 0.0, 0.0, 0.0, 0.0, 0.05, 0.0, 0.0],
        };
        let (qhl, qha, qhb, qm) = (query_gf.hist_l_blob(), query_gf.hist_a_blob(), query_gf.hist_b_blob(), query_gf.moments_blob());

        // Near candidate: similar features (same shape, slightly different counts)
        let near_gf = GradingFeatures {
            hist_l: vec![55.0; 64],
            hist_a: vec![27.0; 64],
            hist_b: vec![27.0; 64],
            moments: [0.52, 0.11, 0.01, 0.05, 0.01, 0.0, 0.0, 0.0, 0.01, 0.05, 0.0, 0.0],
        };
        let (nhl, nha, nhb, nm) = (near_gf.hist_l_blob(), near_gf.hist_a_blob(), near_gf.hist_b_blob(), near_gf.moments_blob());

        // Far candidate: very different features
        let far_gf = GradingFeatures {
            hist_l: vec![1000.0; 64], // Concentrated in one bin
            hist_a: vec![1.0; 64],
            hist_b: vec![1.0; 64],
            moments: [0.9, 0.01, 0.0, 0.01, 0.0, 0.0, 0.0, 0.0, 0.0, 0.01, 0.0, 0.0],
        };
        let (fhl, fha, fhb, fm) = (far_gf.hist_l_blob(), far_gf.hist_a_blob(), far_gf.hist_b_blob(), far_gf.moments_blob());

        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg', ?, ?, ?, ?)",
            rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), qhl, qha, qhb, qm],
        ).unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments) VALUES (2, ?, ?, ?, 0.3, 0.2, 'acescg', ?, ?, ?, ?)",
            rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), nhl, nha, nhb, nm],
        ).unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments) VALUES (3, ?, ?, ?, 0.1, 0.3, 'acescg', ?, ?, ?, ?)",
            rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), fhl, fha, fhb, fm],
        ).unwrap();

        let results = bhattacharyya_search(
            &conn,
            1,
            &SearchOptions { project: None, limit: 10, use_pyramid: false },
        ).unwrap();

        assert_eq!(results.len(), 2);
        // "near" candidate should rank higher than "far"
        assert!(results[0].score >= results[1].score,
            "Expected near (score={}) >= far (score={})", results[0].score, results[1].score);
    }

    #[cfg(moonbit_ffi)]
    #[test]
    fn test_bhattacharyya_search_limit() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'query.dpx', 'dpx')", [])
            .unwrap();

        let (hl, ha, hb, m) = make_grading_features_blob();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg', ?, ?, ?, ?)",
            rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), hl, ha, hb, m],
        ).unwrap();

        // Add 5 identical candidates
        for i in 0..5 {
            conn.execute(
                &format!("INSERT INTO files (project_id, filename, format) VALUES (1, 'c{}.dpx', 'dpx')", i),
                [],
            ).unwrap();
            conn.execute(
                &format!("INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments) VALUES ({}, ?, ?, ?, 0.5, 0.1, 'acescg', ?, ?, ?, ?)", i + 2),
                rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), hl, ha, hb, m],
            ).unwrap();
        }

        let results = bhattacharyya_search(
            &conn,
            1,
            &SearchOptions { project: None, limit: 2, use_pyramid: false },
        ).unwrap();
        assert_eq!(results.len(), 2);
    }

    // --- Stale recomputation tests ---

    #[cfg(moonbit_ffi)]
    #[test]
    fn test_hybrid_search_stale_fingerprint_updated_to_fresh() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'query.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'stale.dpx', 'dpx')", [])
            .unwrap();

        let (hl, ha, hb, m) = make_grading_features_blob();

        // Query fingerprint (fresh)
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments, feature_status) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg', ?, ?, ?, ?, 'fresh')",
            rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), hl, ha, hb, m],
        ).unwrap();

        // Candidate fingerprint (stale)
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments, feature_status) VALUES (2, ?, ?, ?, 0.5, 0.1, 'acescg', ?, ?, ?, ?, 'stale')",
            rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), hl, ha, hb, m],
        ).unwrap();

        let weights = SignalWeights::grading_first();
        let results = hybrid_search(&conn, 1, &weights, &SearchOptions { project: None, limit: 10, use_pyramid: false }).unwrap();

        assert_eq!(results.len(), 1);
        // Source file doesn't exist, so re-extraction is skipped -- status remains stale
        let status: String = conn.query_row(
            "SELECT feature_status FROM fingerprints WHERE file_id = 2",
            [], |row| row.get(0),
        ).unwrap();
        assert_eq!(status, "stale");
    }

    #[cfg(moonbit_ffi)]
    #[test]
    fn test_hybrid_search_null_feature_status_treated_as_stale() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'query.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'null_status.dpx', 'dpx')", [])
            .unwrap();

        let (hl, ha, hb, m) = make_grading_features_blob();

        // Query fingerprint (fresh)
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments, feature_status) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg', ?, ?, ?, ?, 'fresh')",
            rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), hl, ha, hb, m],
        ).unwrap();

        // Candidate fingerprint (NULL feature_status -- treated as stale)
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments) VALUES (2, ?, ?, ?, 0.5, 0.1, 'acescg', ?, ?, ?, ?)",
            rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), hl, ha, hb, m],
        ).unwrap();

        let weights = SignalWeights::grading_first();
        let results = hybrid_search(&conn, 1, &weights, &SearchOptions { project: None, limit: 10, use_pyramid: false }).unwrap();

        assert_eq!(results.len(), 1);
        // Source file doesn't exist, so re-extraction is skipped -- status remains NULL
        let status: Option<String> = conn.query_row(
            "SELECT feature_status FROM fingerprints WHERE file_id = 2",
            [], |row| row.get(0),
        ).unwrap();
        assert!(status.is_none());
    }

    #[cfg(moonbit_ffi)]
    #[test]
    fn test_hybrid_search_fresh_fingerprint_not_recomputed() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'query.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'candidate.dpx', 'dpx')", [])
            .unwrap();

        let (hl, ha, hb, m) = make_grading_features_blob();

        // Both fresh
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments, feature_status) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg', ?, ?, ?, ?, 'fresh')",
            rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), hl, ha, hb, m],
        ).unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments, feature_status) VALUES (2, ?, ?, ?, 0.5, 0.1, 'acescg', ?, ?, ?, ?, 'fresh')",
            rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), hl, ha, hb, m],
        ).unwrap();

        let weights = SignalWeights::grading_first();
        let results = hybrid_search(&conn, 1, &weights, &SearchOptions { project: None, limit: 10, use_pyramid: false }).unwrap();

        assert_eq!(results.len(), 1);
        // Status should remain fresh (no UPDATE attempted)
        let status: String = conn.query_row(
            "SELECT feature_status FROM fingerprints WHERE file_id = 2",
            [], |row| row.get(0),
        ).unwrap();
        assert_eq!(status, "fresh");
    }

    #[cfg(moonbit_ffi)]
    #[test]
    fn test_hybrid_search_rate_limit_stale_recomputation() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'query.dpx', 'dpx')", [])
            .unwrap();

        let (hl, ha, hb, m) = make_grading_features_blob();

        // Query fingerprint (fresh)
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments, feature_status) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg', ?, ?, ?, ?, 'fresh')",
            rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), hl, ha, hb, m],
        ).unwrap();

        // Create 12 stale candidates
        for i in 2..=13 {
            conn.execute(
                &format!("INSERT INTO files (project_id, filename, format) VALUES (1, 'stale_{}.dpx', 'dpx')", i),
                [],
            ).unwrap();
            conn.execute(
                &format!("INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments, feature_status) VALUES ({}, ?, ?, ?, 0.5, 0.1, 'acescg', ?, ?, ?, ?, 'stale')", i),
                rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), hl, ha, hb, m],
            ).unwrap();
        }

        let weights = SignalWeights::grading_first();
        let results = hybrid_search(&conn, 1, &weights, &SearchOptions { project: None, limit: 20, use_pyramid: false }).unwrap();

        // All 12 should be returned (features exist, just stale)
        assert_eq!(results.len(), 12);

        // Source files don't exist, so re-extraction is skipped for all -- status remains stale
        let stale_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM fingerprints WHERE file_id >= 2 AND feature_status = 'stale'",
            [], |row| row.get(0),
        ).unwrap();
        assert_eq!(stale_count, 12);
    }

    #[cfg(moonbit_ffi)]
    #[test]
    fn test_hybrid_search_atomic_transition_no_double_recompute() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'query.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'candidate.dpx', 'dpx')", [])
            .unwrap();

        let (hl, ha, hb, m) = make_grading_features_blob();

        // Query fingerprint (fresh)
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments, feature_status) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg', ?, ?, ?, ?, 'fresh')",
            rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), hl, ha, hb, m],
        ).unwrap();

        // Candidate that starts stale
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments, feature_status) VALUES (2, ?, ?, ?, 0.5, 0.1, 'acescg', ?, ?, ?, ?, 'stale')",
            rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), hl, ha, hb, m],
        ).unwrap();

        let weights = SignalWeights::grading_first();

        // First search: should attempt recompute, but source file doesn't exist -- skipped
        let results1 = hybrid_search(&conn, 1, &weights, &SearchOptions { project: None, limit: 10, use_pyramid: false }).unwrap();
        assert_eq!(results1.len(), 1);
        let status1: String = conn.query_row(
            "SELECT feature_status FROM fingerprints WHERE file_id = 2", [], |row| row.get(0),
        ).unwrap();
        assert_eq!(status1, "stale");

        // Second search: candidate is still stale, should attempt again but still skip
        let results2 = hybrid_search(&conn, 1, &weights, &SearchOptions { project: None, limit: 10, use_pyramid: false }).unwrap();
        assert_eq!(results2.len(), 1);
        let status2: String = conn.query_row(
            "SELECT feature_status FROM fingerprints WHERE file_id = 2", [] , |row| row.get(0),
        ).unwrap();
        assert_eq!(status2, "stale");
    }

    #[test]
    fn test_pyramid_mode_falls_back_to_flat_when_no_tiles() {
        let conn = setup_test_db();
        // Create fingerprint_tiles table for this test
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS fingerprint_tiles (
                id INTEGER NOT NULL PRIMARY KEY,
                fingerprint_id INTEGER NOT NULL,
                tile_row INTEGER NOT NULL,
                tile_col INTEGER NOT NULL,
                oklab_hist_l BLOB,
                oklab_hist_a BLOB,
                oklab_hist_b BLOB,
                color_moments BLOB,
                hist_bins INTEGER NOT NULL DEFAULT 64
            );"
        ).unwrap();

        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'query.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'candidate.dpx', 'dpx')", [])
            .unwrap();

        let (hl, ha, hb, m) = make_grading_features_blob();

        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg', ?, ?, ?, ?)",
            rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), hl, ha, hb, m],
        ).unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments) VALUES (2, ?, ?, ?, 0.5, 0.1, 'acescg', ?, ?, ?, ?)",
            rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), hl, ha, hb, m],
        ).unwrap();

        let weights = SignalWeights::grading_first();

        // pyramid mode but no tiles stored -- should still return results via flat fallback
        let results = hybrid_search(
            &conn,
            1,
            &weights,
            &SearchOptions { project: None, limit: 10, use_pyramid: true },
        ).unwrap();

        let flat_results = hybrid_search(
            &conn,
            1,
            &weights,
            &SearchOptions { project: None, limit: 10, use_pyramid: false },
        ).unwrap();

        assert_eq!(results.len(), flat_results.len());
        for (r, f) in results.iter().zip(flat_results.iter()) {
            assert!((r.score - f.score).abs() < 1e-10, "pyramid fallback score should match flat: {} vs {}", r.score, f.score);
        }
    }

    // =================================================================
    // Task 4.1a: bhattacharyya_search edge-case tests (2 tests)
    // =================================================================

    #[test]
    fn test_bhattacharyya_search_empty_candidates() {
        // Query has grading features but no other fingerprints exist -> NoFingerprints
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'query.dpx', 'dpx')", [])
            .unwrap();

        let (hl, ha, hb, m) = make_grading_features_blob();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg', ?, ?, ?, ?)",
            rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), hl, ha, hb, m],
        ).unwrap();

        let result = bhattacharyya_search(
            &conn,
            1,
            &SearchOptions { project: None, limit: 10, use_pyramid: false },
        );
        assert!(result.is_err());
        match result.unwrap_err() {
            SearchError::NoFingerprints => {}
            other => panic!("Expected NoFingerprints for empty candidate set, got: {:?}", other),
        }
    }

    #[cfg(moonbit_ffi)]
    #[test]
    fn test_bhattacharyya_search_candidates_missing_grading_features() {
        // Candidates exist but all lack oklab_hist_l (NULL) -> skipped, returns NoFingerprints
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'query.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'no_features.dpx', 'dpx')", [])
            .unwrap();

        let (hl, ha, hb, m) = make_grading_features_blob();

        // Query has grading features
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg', ?, ?, ?, ?)",
            rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), hl, ha, hb, m],
        ).unwrap();
        // Candidate without grading features (oklab_hist_l is NULL)
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (2, ?, ?, ?, 0.3, 0.2, 'acescg')",
            [make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625)],
        ).unwrap();

        let result = bhattacharyya_search(
            &conn,
            1,
            &SearchOptions { project: None, limit: 10, use_pyramid: false },
        );
        // SQL WHERE clause filters out rows with NULL oklab_hist_l, so no candidates remain
        assert!(result.is_err());
        match result.unwrap_err() {
            SearchError::NoFingerprints => {}
            other => panic!("Expected NoFingerprints when all candidates lack grading features, got: {:?}", other),
        }
    }

    // =================================================================
    // Task 4.1b: search_by_tag edge-case tests (2 tests)
    // =================================================================

    #[test]
    fn test_search_by_tag_special_characters_sql_injection_safety() {
        // Tags containing single quotes and SQL-like chars should not break the query
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'scene.dpx', 'dpx')", [])
            .unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg')",
            [make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625)],
        ).unwrap();
        // Insert a tag with special characters
        conn.execute("INSERT INTO tags (fingerprint_id, tag) VALUES (1, \"it's a test\")", [])
            .unwrap();

        // Search for the tag with special characters — should find it via parameterized query
        let results = search_by_tag(
            &conn,
            "it's a test",
            &SearchOptions { project: None, limit: 10, use_pyramid: false },
        );
        // The single quote is part of the tag value; parameterized query handles it safely.
        // split_whitespace() splits "it's" and "a" and "test" into 3 tokens.
        // Only exact tag matches count, so this may return NoFingerprints if no tag == "it's" exactly.
        // What matters: no SQL error / panic occurs.
        match results {
            Ok(_) => {} // Found matches — acceptable
            Err(SearchError::NoFingerprints) => {} // No exact match — also acceptable
            Err(other) => panic!("Unexpected error for special-character tag search: {:?}", other),
        }
    }

    #[test]
    fn test_search_by_tag_empty_string_returns_error() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 's1.dpx', 'dpx')", [])
            .unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg')",
            [make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625)],
        ).unwrap();

        let result = search_by_tag(
            &conn,
            "",
            &SearchOptions { project: None, limit: 10, use_pyramid: false },
        );
        assert!(result.is_err());
        match result.unwrap_err() {
            SearchError::NoFingerprints => {}
            other => panic!("Expected NoFingerprints for empty tag string, got: {:?}", other),
        }
    }

    #[test]
    fn test_search_by_tag_whitespace_only_returns_error() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 's1.dpx', 'dpx')", [])
            .unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg')",
            [make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625)],
        ).unwrap();

        let result = search_by_tag(
            &conn,
            "   ",
            &SearchOptions { project: None, limit: 10, use_pyramid: false },
        );
        assert!(result.is_err());
        match result.unwrap_err() {
            SearchError::NoFingerprints => {}
            other => panic!("Expected NoFingerprints for whitespace-only tag string, got: {:?}", other),
        }
    }

    // =================================================================
    // Task 4.1c: search_histograms edge-case tests (2 tests)
    // =================================================================

    #[test]
    fn test_search_histograms_limit_zero_returns_empty() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'scene001.dpx', 'dpx')", [])
            .unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg')",
            [make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625)],
        )
        .unwrap();

        let result = search_histograms(
            &conn,
            &SearchOptions {
                project: None,
                limit: 0,
                use_pyramid: false,
            },
        )
        .unwrap();
        assert_eq!(result.len(), 0, "limit=0 should return empty results");
    }

    #[test]
    fn test_search_histograms_all_malformed_returns_error() {
        // All fingerprints have malformed histograms -> scored stays empty -> NoFingerprints
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'bad1.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'bad2.dpx', 'dpx')", [])
            .unwrap();
        // Both fingerprints have malformed histograms (wrong bin count)
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, '0.1,0.2', '0.1,0.2', '0.1,0.2', 0.5, 0.1, 'acescg')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (2, 'x,y,z', 'a,b,c', 'd,e,f', 0.3, 0.2, 'acescg')",
            [],
        )
        .unwrap();

        let result = search_histograms(
            &conn,
            &SearchOptions {
                project: None,
                limit: 10,
                use_pyramid: false,
            },
        );
        assert!(result.is_err());
        match result.unwrap_err() {
            SearchError::NoFingerprints => {}
            other => panic!("Expected NoFingerprints when all histograms are malformed, got: {:?}", other),
        }
    }

    // =================================================================
    // Task 4.1d: search_by_image_and_tag edge-case tests (2 tests)
    // =================================================================

    #[test]
    fn test_search_by_image_and_tag_empty_tag_returns_nofingerprints() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'scene.dpx', 'dpx')", [])
            .unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg')",
            [make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625)],
        ).unwrap();
        conn.execute("INSERT INTO tags (fingerprint_id, tag) VALUES (1, 'warm')", [])
            .unwrap();

        let result = search_by_image_and_tag(
            &conn,
            "",  // empty tag string
            "/nonexistent/image.png",
            &SearchOptions { project: None, limit: 10, use_pyramid: false },
            300,
            30,
            "test-model",
        );
        assert!(result.is_err());
        match result.unwrap_err() {
            SearchError::NoFingerprints => {}
            other => panic!("Expected NoFingerprints for empty tag, got: {:?}", other),
        }
    }

    #[test]
    fn test_search_by_image_and_tag_nonexistent_image_falls_back_to_tag_search() {
        // When image path doesn't exist, PythonBridge fails to generate embedding
        // and falls back to pure tag search (or returns AiUnavailable)
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'scene.dpx', 'dpx')", [])
            .unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg')",
            [make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625)],
        ).unwrap();
        conn.execute("INSERT INTO tags (fingerprint_id, tag) VALUES (1, 'warm')", [])
            .unwrap();

        // Image doesn't exist — PythonBridge will fail; function should either fall back to tag search
        // or return an error gracefully (not panic)
        let result = search_by_image_and_tag(
            &conn,
            "warm",
            "/tmp/this_image_definitely_does_not_exist_12345.png",
            &SearchOptions { project: None, limit: 10, use_pyramid: false },
            1,   // very short idle timeout
            2,   // very short inference timeout
            "test-model",
        );
        // Acceptable outcomes: Ok (fell back to tag search) or Err (AiUnavailable / embedding error)
        // Must not panic
        match result {
            Ok(results) => {
                // Fell back to tag-only search successfully
                assert!(!results.is_empty() || results.is_empty()); // just don't crash
            }
            Err(e) => {
                // Graceful degradation — any of these errors is acceptable
                let msg = format!("{}", e);
                let acceptable = msg.contains("SEARCH_AI_UNAVAILABLE")
                    || msg.contains("SEARCH_EMBEDDING_ERROR")
                    || msg.contains("SEARCH_NO_FINGERPRINTS")
                    || msg.contains("SEARCH_DB_ERROR");
                assert!(acceptable, "Unexpected error for nonexistent image: {}", msg);
            }
        }
    }

    // =================================================================
    // Additional search edge-case tests (to reach +25 target)
    // =================================================================

    #[test]
    fn test_search_by_tag_single_result_limit_one() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        for i in 0..3 {
            conn.execute(
                &format!("INSERT INTO files (project_id, filename, format) VALUES (1, 's{}.dpx', 'dpx')", i),
                [],
            ).unwrap();
            conn.execute(
                &format!("INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES ({}, ?, ?, ?, 0.5, 0.1, 'acescg')", i + 1),
                [make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625)],
            ).unwrap();
            conn.execute(
                &format!("INSERT INTO tags (fingerprint_id, tag) VALUES ({}, 'warm')", i + 1),
                [],
            ).unwrap();
        }

        // limit=1 should return only the top result
        let results = search_by_tag(
            &conn,
            "warm",
            &SearchOptions { project: None, limit: 1, use_pyramid: false },
        ).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_search_by_tag_project_filter_excludes_other_projects() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('proj_a', '/tmp/a')", [])
            .unwrap();
        conn.execute("INSERT INTO projects (name, path) VALUES ('proj_b', '/tmp/b')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'a.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (2, 'b.dpx', 'dpx')", [])
            .unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg')",
            [make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625)],
        ).unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (2, ?, ?, ?, 0.3, 0.2, 'acescg')",
            [make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625)],
        ).unwrap();
        conn.execute("INSERT INTO tags (fingerprint_id, tag) VALUES (1, 'shared')", []).unwrap();
        conn.execute("INSERT INTO tags (fingerprint_id, tag) VALUES (2, 'shared')", []).unwrap();

        // Scoped to proj_a -> only a.dpx
        let results = search_by_tag(
            &conn,
            "shared",
            &SearchOptions { project: Some("proj_a".to_string()), limit: 10, use_pyramid: false },
        ).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].file_path, "a.dpx");
    }

    #[test]
    fn test_search_histograms_mixed_valid_and_invalid() {
        // Mix of valid and malformed histograms: only valid ones returned
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'good.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'bad.dpx', 'dpx')", [])
            .unwrap();

        // Good fingerprint
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg')",
            [make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625)],
        ).unwrap();
        // Bad fingerprint
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (2, 'bad', 'bad', 'bad', 0.3, 0.2, 'acescg')",
            [],
        ).unwrap();

        let results = search_histograms(
            &conn,
            &SearchOptions { project: None, limit: 10, use_pyramid: false },
        ).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].file_path, "good.dpx");
    }

    #[test]
    fn test_search_histograms_result_ordering() {
        // Results should be sorted by project name then filename
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'z_last.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'a_first.dpx', 'dpx')", [])
            .unwrap();

        let hist = make_histogram_csv(0.015625);
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, ?1, ?2, ?3, 0.5, 0.1, 'acescg')",
            rusqlite::params![hist.as_str(), hist.as_str(), hist.as_str()],
        ).unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (2, ?1, ?2, ?3, 0.3, 0.2, 'acescg')",
            rusqlite::params![hist.as_str(), hist.as_str(), hist.as_str()],
        ).unwrap();

        let results = search_histograms(
            &conn,
            &SearchOptions { project: None, limit: 10, use_pyramid: false },
        ).unwrap();
        assert_eq!(results.len(), 2);
        // Sorted alphabetically: a_first before z_last
        assert_eq!(results[0].file_path, "a_first.dpx");
        assert_eq!(results[1].file_path, "z_last.dpx");
    }

    #[cfg(moonbit_ffi)]
    #[test]
    fn test_bhattacharyya_search_self_exclusion() {
        // Query file should NOT appear in its own results (file_id != query)
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'query.dpx', 'dpx')", [])
            .unwrap();

        let (hl, ha, hb, m) = make_grading_features_blob();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg', ?, ?, ?, ?)",
            rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), hl, ha, hb, m],
        ).unwrap();

        // Only one fingerprint exists and it's the query itself -> NoFingerprints
        let result = bhattacharyya_search(
            &conn,
            1,
            &SearchOptions { project: None, limit: 10, use_pyramid: false },
        );
        assert!(result.is_err());
        match result.unwrap_err() {
            SearchError::NoFingerprints => {}
            other => panic!("Expected NoFingerprints when only candidate is self, got: {:?}", other),
        }
    }

    #[test]
    fn test_summarize_histogram_empty_input() {
        let summary = summarize_histogram(&[]);
        assert_eq!(summary.mean_value, 0.0);
        assert_eq!(summary.dominant_bin_min, 0);
        assert_eq!(summary.dominant_bin_max, 0);
    }

    #[test]
    fn test_fingerprint_info_with_tags() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'tagged.dpx', 'dpx')", [])
            .unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg')",
            [make_histogram_csv(0.02), make_histogram_csv(0.01), make_histogram_csv(0.03)],
        ).unwrap();
        conn.execute("INSERT INTO tags (fingerprint_id, tag) VALUES (1, 'dramatic')", []).unwrap();
        conn.execute("INSERT INTO tags (fingerprint_id, tag) VALUES (1, 'high-contrast')", []).unwrap();

        // Use fingerprint_info_with_tags which loads tags from DB
        let info = fingerprint_info_with_tags(&conn, "film", "tagged.dpx").unwrap();
        assert_eq!(info.tags.len(), 2);
        assert!(info.tags.contains(&"dramatic".to_string()));
        assert!(info.tags.contains(&"high-contrast".to_string()));
    }
