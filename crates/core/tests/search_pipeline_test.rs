// search_pipeline_test.rs — End-to-end search pipeline integration test.
//
// Creates an isolated DB via open_db_at_path, inserts test data (fingerprint + tags)
// using direct SQL, then exercises the mengxi_core search API and verifies results.

use tempfile::TempDir;

fn make_histogram_csv(value: f64) -> String {
    (0..64)
        .map(|_| value.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

/// Create an isolated file-backed DB with all migrations applied.
fn setup_isolated_db() -> (TempDir, rusqlite::Connection) {
    let dir = tempfile::tempdir().unwrap();
    let db_file = dir.path().join("pipeline_test.db");
    let conn = mengxi_core::db::open_db_at_path(&db_file).unwrap();
    (dir, conn)
}

#[test]
fn test_search_pipeline_full_roundtrip() {
    let (_dir, conn) = setup_isolated_db();

    // --- Insert test data ---

    // Project
    conn.execute(
        "INSERT INTO projects (name, path) VALUES ('film_noir', '/tmp/film_noir')",
        [],
    )
    .unwrap();

    // Files
    conn.execute(
        "INSERT INTO files (project_id, filename, format) VALUES (1, 'scene001.dpx', 'dpx')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO files (project_id, filename, format) VALUES (1, 'scene002.dpx', 'dpx')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO files (project_id, filename, format) VALUES (1, 'scene003.exr', 'exr')",
        [],
    )
    .unwrap();

    // Fingerprints with valid histograms
    let hist = make_histogram_csv(0.015625);
    for file_id in 1..=3 {
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (?1, ?2, ?2, ?2, 0.5, 0.1, 'acescg')",
            rusqlite::params![file_id, hist],
        )
        .unwrap();
    }

    // Tags
    conn.execute("INSERT INTO tags (fingerprint_id, tag) VALUES (1, 'dark')", [])
        .unwrap();
    conn.execute("INSERT INTO tags (fingerprint_id, tag) VALUES (1, 'moody')", [])
        .unwrap();
    conn.execute("INSERT INTO tags (fingerprint_id, tag) VALUES (2, 'dark')", [])
        .unwrap();
    conn.execute("INSERT INTO tags (fingerprint_id, tag) VALUES (3, 'bright')", [])
        .unwrap();

    // --- Exercise search APIs ---

    use mengxi_core::search::{SearchOptions, SearchError};

    // 1. Histogram search: global scope should return all 3 fingerprints
    let hist_results = mengxi_core::search::search_histograms(
        &conn,
        &SearchOptions {
            project: None,
            limit: 10,
            use_pyramid: false,
        },
    )
    .expect("histogram search should succeed with valid data");
    assert_eq!(
        hist_results.len(),
        3,
        "Expected 3 histogram results, got {}",
        hist_results.len()
    );
    // Verify result structure
    for r in &hist_results {
        assert!(r.rank >= 1);
        assert_eq!(r.project_name, "film_noir");
        assert!(!r.file_path.is_empty());
        assert!(r.score > 0.0);
    }

    // 2. Histogram search: scoped to project should return same 3
    let scoped_results = mengxi_core::search::search_histograms(
        &conn,
        &SearchOptions {
            project: Some("film_noir".to_string()),
            limit: 10,
            use_pyramid: false,
        },
    )
    .expect("scoped histogram search should succeed");
    assert_eq!(
        scoped_results.len(),
        3,
        "Expected 3 scoped histogram results, got {}",
        scoped_results.len()
    );

    // 3. Tag search: search for "dark" should match scene001 and scene002
    let tag_results = mengxi_core::search::search_by_tag(
        &conn,
        "dark",
        &SearchOptions {
            project: None,
            limit: 10,
            use_pyramid: false,
        },
    )
    .expect("tag search for 'dark' should succeed");
    assert_eq!(
        tag_results.len(),
        2,
        "Expected 2 results tagged 'dark', got {}",
        tag_results.len()
    );
    let filenames: Vec<&str> = tag_results.iter().map(|r| r.file_path.as_str()).collect();
    assert!(
        filenames.contains(&"scene001.dpx"),
        "Results should include scene001.dpx"
    );
    assert!(
        filenames.contains(&"scene002.dpx"),
        "Results should include scene002.dpx"
    );

    // 4. Tag search: multi-tag "dark moody" should rank scene001 highest (matches both)
    let multi_tag_results = mengxi_core::search::search_by_tag(
        &conn,
        "dark moody",
        &SearchOptions {
            project: None,
            limit: 10,
            use_pyramid: false,
        },
    )
    .expect("multi-tag search should succeed");
    assert_eq!(
        multi_tag_results.len(),
        2,
        "Expected 2 multi-tag results, got {}",
        multi_tag_results.len()
    );
    // scene001 matches both tags -> score should be 1.0 (max count / max count)
    assert_eq!(
        multi_tag_results[0].file_path, "scene001.dpx",
        "scene001 (matches both tags) should rank first"
    );
    assert!(
        (multi_tag_results[0].score - 1.0).abs() < 1e-10,
        "Top result score should be 1.0, got {}",
        multi_tag_results[0].score
    );

    // 5. Tag search: non-existent tag returns error
    let no_tag_result = mengxi_core::search::search_by_tag(
        &conn,
        "nonexistent_tag_xyz",
        &SearchOptions {
            project: None,
            limit: 10,
            use_pyramid: false,
        },
    );
    assert!(
        no_tag_result.is_err(),
        "Searching for non-existent tag should return an error"
    );

    // 6. Histogram search: limit=1 returns at most 1 result
    let limited_results = mengxi_core::search::search_histograms(
        &conn,
        &SearchOptions {
            project: None,
            limit: 1,
            use_pyramid: false,
        },
    )
    .unwrap();
    assert_eq!(limited_results.len(), 1);

    // 7. Fingerprint info query: look up a specific fingerprint
    let info =
        mengxi_core::search::fingerprint_info(&conn, "film_noir", "scene001.dpx")
            .expect("fingerprint_info should find the inserted record");
    assert_eq!(info.project_name, "film_noir");
    assert_eq!(info.file_path, "scene001.dpx");
    assert_eq!(info.color_space_tag, "acescg");

    // 8. Fingerprint info: non-existent file returns error
    let missing_info =
        mengxi_core::search::fingerprint_info(&conn, "film_noir", "nonexistent.dpx");
    assert!(missing_info.is_err(), "Non-existent fingerprint should return error");
}

#[test]
fn test_search_pipeline_empty_database() {
    // Verify graceful degradation when DB has schema but no data
    let (_dir, conn) = setup_isolated_db();

    use mengxi_core::search::{SearchOptions, SearchError};

    // Histogram search on empty DB -> NoFingerprints
    let result = mengxi_core::search::search_histograms(
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
        other => panic!("Expected NoFingerprints on empty DB, got {:?}", other),
    }

    // Tag search on empty DB -> NoFingerprints
    let result = mengxi_core::search::search_by_tag(
        &conn,
        "any_tag",
        &SearchOptions {
            project: None,
            limit: 10,
            use_pyramid: false,
        },
    );
    assert!(result.is_err());
    match result.unwrap_err() {
        SearchError::NoFingerprints => {}
        other => panic!("Expected NoFingerprints on empty DB, got {:?}", other),
    }
}

#[test]
fn test_search_pipeline_project_isolation() {
    // Verify that project scoping correctly isolates results
    let (_dir, conn) = setup_isolated_db();

    use mengxi_core::search::SearchOptions;

    // Two projects
    conn.execute(
        "INSERT INTO projects (name, path) VALUES ('project_a', '/tmp/a')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO projects (name, path) VALUES ('project_b', '/tmp/b')",
        [],
    )
    .unwrap();

    // Files in each project
    conn.execute(
        "INSERT INTO files (project_id, filename, format) VALUES (1, 'a_scene.dpx', 'dpx')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO files (project_id, filename, format) VALUES (2, 'b_scene.dpx', 'dpx')",
        [],
    )
    .unwrap();

    let hist = make_histogram_csv(0.015625);
    conn.execute(
        "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, ?1, ?1, ?1, 0.5, 0.1, 'acescg')",
        rusqlite::params![hist],
    ).unwrap();
    conn.execute(
        "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (2, ?1, ?1, ?1, 0.5, 0.1, 'acescg')",
        rusqlite::params![hist],
    ).unwrap();

    // Scoped to project_a -> only 1 result
    let results_a = mengxi_core::search::search_histograms(
        &conn,
        &SearchOptions {
            project: Some("project_a".to_string()),
            limit: 10,
            use_pyramid: false,
        },
    ).unwrap();
    assert_eq!(results_a.len(), 1);
    assert_eq!(results_a[0].project_name, "project_a");

    // Scoped to project_b -> only 1 result
    let results_b = mengxi_core::search::search_histograms(
        &conn,
        &SearchOptions {
            project: Some("project_b".to_string()),
            limit: 10,
            use_pyramid: false,
        },
    ).unwrap();
    assert_eq!(results_b.len(), 1);
    assert_eq!(results_b[0].project_name, "project_b");

    // Global -> 2 results
    let results_global = mengxi_core::search::search_histograms(
        &conn,
        &SearchOptions {
            project: None,
            limit: 10,
            use_pyramid: false,
        },
    ).unwrap();
    assert_eq!(results_global.len(), 2);
}

#[test]
fn test_search_pipeline_tag_search_ranking_by_match_count() {
    // Verify that multi-tag results are ranked by match count (descending)
    let (_dir, conn) = setup_isolated_db();

    use mengxi_core::search::SearchOptions;

    conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", []).unwrap();

    // 3 files, each with different tag counts
    for i in 1..=3 {
        conn.execute(
            &format!("INSERT INTO files (project_id, filename, format) VALUES (1, 's{}.dpx', 'dpx')", i),
            [],
        ).unwrap();
        let hist = make_histogram_csv(0.015625);
        conn.execute(
            &format!("INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES ({}, ?1, ?1, ?1, 0.5, 0.1, 'acescg')", i),
            rusqlite::params![hist],
        ).unwrap();
    }

    // s1: 3 tags, s2: 2 tags, s3: 1 tag
    for &tag in &["a", "b", "c"] {
        conn.execute("INSERT INTO tags (fingerprint_id, tag) VALUES (1, ?1)", rusqlite::params![tag]).unwrap();
    }
    for &tag in &["a", "b"] {
        conn.execute("INSERT INTO tags (fingerprint_id, tag) VALUES (2, ?1)", rusqlite::params![tag]).unwrap();
    }
    conn.execute("INSERT INTO tags (fingerprint_id, tag) VALUES (3, 'a')", []).unwrap();

    let results = mengxi_core::search::search_by_tag(
        &conn,
        "a b c",
        &SearchOptions { project: None, limit: 10, use_pyramid: false },
    ).unwrap();

    assert_eq!(results.len(), 3);
    // s1 matches all 3 tags -> score 1.0
    assert_eq!(results[0].file_path, "s1.dpx");
    assert!((results[0].score - 1.0).abs() < 1e-10);
    // s2 matches 2 tags -> score 2/3
    assert_eq!(results[1].file_path, "s2.dpx");
    assert!((results[1].score - 2.0 / 3.0).abs() < 1e-10);
    // s3 matches 1 tag -> score 1/3
    assert_eq!(results[2].file_path, "s3.dpx");
    assert!((results[2].score - 1.0 / 3.0).abs() < 1e-10);
}

#[test]
fn test_search_pipeline_histogram_search_nonexistent_project() {
    let (_dir, conn) = setup_isolated_db();

    use mengxi_core::search::{SearchOptions, SearchError};

    // Insert data into project_a only
    conn.execute("INSERT INTO projects (name, path) VALUES ('project_a', '/tmp/a')", []).unwrap();
    conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 's.dpx', 'dpx')", []).unwrap();
    let hist = make_histogram_csv(0.015625);
    conn.execute(
        "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, ?1, ?1, ?1, 0.5, 0.1, 'acescg')",
        rusqlite::params![hist],
    ).unwrap();

    // Search for nonexistent project -> ProjectNotFound
    let result = mengxi_core::search::search_histograms(
        &conn,
        &SearchOptions {
            project: Some("nonexistent_project".to_string()),
            limit: 10,
            use_pyramid: false,
        },
    );
    assert!(result.is_err());
    match result.unwrap_err() {
        SearchError::ProjectNotFound(name) => {
            assert_eq!(name, "nonexistent_project");
        }
        other => panic!("Expected ProjectNotFound, got {:?}", other),
    }
}

#[test]
fn test_search_pipeline_fingerprint_info_tags_loaded() {
    let (_dir, conn) = setup_isolated_db();

    conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", []).unwrap();
    conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'scene.dpx', 'dpx')", []).unwrap();
    let hist = make_histogram_csv(0.015625);
    conn.execute(
        "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, ?1, ?1, ?1, 0.5, 0.1, 'rec709')",
        rusqlite::params![hist],
    ).unwrap();
    conn.execute("INSERT INTO tags (fingerprint_id, tag) VALUES (1, 'cinematic')", []).unwrap();
    conn.execute("INSERT INTO tags (fingerprint_id, tag) VALUES (1, 'wide-gamut')", []).unwrap();

    let info = mengxi_core::search::fingerprint_info_with_tags(&conn, "film", "scene.dpx").unwrap();
    assert_eq!(info.tags.len(), 2);
    assert!(info.tags.contains(&"cinematic".to_string()));
    assert!(info.tags.contains(&"wide-gamut".to_string()));
    assert_eq!(info.color_space_tag, "rec709");
}
