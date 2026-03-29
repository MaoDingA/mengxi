# Epic v3: Scale & Intelligence

Generated: 2026-03-29
Status: planning
Source: prd-v2-algorithms.md Phase 3 + v2 code review deferred items

## Overview

v3 builds on the completed v2 algorithm enhancement (Oklab + hybrid scoring + feature translation). Focus areas: algorithm parameterization, search at scale, result comparison tooling, and code quality.

## Priority Classification

- **P0 (Must-do)**: Fix technical debt from v2, unlock configurable parameters
- **P1 (High-value)**: Scale to >10K fingerprints, batch operations
- **P2 (Nice-to-have)**: Comparison tooling, advanced color processing

---

## Epic 1: Algorithm Parameterization & Code Quality (P0)

Clean up v2 technical debt and make core algorithm parameters configurable.

### Story 1.1: Configurable Histogram Bins

As a 开发者,
I want histogram bin count to be configurable via project config,
so that I can tune the granularity of color distribution representation.

**Acceptance Criteria:**

1. Given a project config with `histogram_bins = 128`, when features are extracted, then 128-bin histograms are generated (not hardcoded 64)
2. Given a project config with `histogram_bins = 32`, when features are extracted, then 32-bin histograms are generated
3. Given no config, when features are extracted, then default 64-bin histograms are generated (backward compatible)
4. Given `histogram_bins < 8`, when config is loaded, then validation rejects the value with error
5. Changes span MoonBit (GRADING_HIST_BINS parameterized), C FFI (pass bins as argument), Rust (read from config)

**Files:** `moonbit/src/lib/ffi.c`, `crates/core/src/color_science.rs`, `crates/core/src/fingerprint.rs`, `crates/core/src/project.rs`

### Story 1.2: Feature Extraction Pipeline Refactoring

As a 开发者,
I want the feature extraction orchestration to be a single shared function,
so that import and re-extraction use the same code path.

**Acceptance Criteria:**

1. Given a new `extract_features_from_pixels()` function in core, when import or re-extraction runs, then both call the same function
2. Given the refactored pipeline, when tests run, then all 652+ existing tests pass with zero regressions
3. The function handles: color space detection, pixel reading dispatch, downsampling, RGB→Oklab, FFI extraction, BLOB serialization
4. `project.rs` import path and `fingerprint.rs` re-extraction path both delegate to the shared function

**Files:** `crates/core/src/feature_pipeline.rs` (new), `crates/core/src/project.rs`, `crates/core/src/fingerprint.rs`

### Story 1.3: Configurable Grading Style Tag Vocabulary

As a 开发者,
I want the grading style tag vocabulary to be loaded from a config file,
so that teams can extend the vocabulary without code changes.

**Acceptance Criteria:**

1. Given a `vocabulary.toml` in the project directory, when `validate-dataset` runs, then tags are validated against the custom vocabulary
2. Given no vocabulary file, when `validate-dataset` runs, then the built-in default vocabulary is used
3. Given a vocabulary file with duplicate entries, when loaded, then duplicates are silently deduplicated
4. The vocabulary file format is a simple TOML array: `tags = ["high_contrast", "warm", ...]`

**Files:** `crates/cli/src/validate_dataset.rs`, `crates/core/src/config.rs` (or new vocabulary module)

---

## Epic 2: Search Performance at Scale (P1)

Enable mengxi to handle >10K fingerprints with fast vector search.

### Story 2.1: HNSW Vector Index for Embedding Search

As a 调色师,
I want search to remain fast even with 50,000+ fingerprints,
so that I can search across my entire career's worth of projects.

**Acceptance Criteria:**

1. Given 50K fingerprints with embeddings, when a search is performed, then results return in <500ms (vs current O(n) linear scan)
2. Given a new fingerprint imported, when the index is updated, then the index is incrementally updated (no full rebuild)
3. Given <1K fingerprints, when the system starts, then HNSW is not used (fallback to linear scan for small datasets)
4. The HNSW index is persisted alongside the SQLite database
5. No new external C/C++ dependencies — use pure Rust HNSW implementation (e.g., `instant-distance` or `hnsw`)

**Files:** `crates/core/src/vector_index.rs` (new), `crates/core/src/search.rs`, `crates/core/src/project.rs`

### Story 2.2: Batch Embedding Generation

As a 调色师,
I want to regenerate CLIP embeddings for all fingerprints in a project,
so that I can update embeddings after model improvements.

**Acceptance Criteria:**

1. Given a project with 500 fingerprints without embeddings, when `mengxi embed --project film` runs, then all 500 embeddings are generated
2. Given embeddings already exist, when `mengxi embed --project film --force` runs, then all embeddings are regenerated
3. Given the Python AI subprocess is unavailable, when embed runs, then a clear error message is shown
4. Progress is reported to stderr, summary to stdout
5. Supports `--json` output

**Files:** `crates/cli/src/main.rs`, `crates/core/src/embedding.rs` (new or extend existing)

### Story 2.3: Batch Re-extraction with Transaction Optimization

As a 调色师,
I want to re-extract features for thousands of fingerprints efficiently,
so that bulk operations don't take hours.

**Acceptance Criteria:**

1. Given 1000 fingerprints in a project, when `mengxi reextract --project film` runs, then all are processed in a single WAL transaction batch
2. Given a batch of 1000, when processing reaches #500 and fails, then #1-#499 are persisted and #500-#1000 are reported as not processed
3. Performance: at least 10x faster than individual auto-commits for large batches

**Files:** `crates/core/src/fingerprint.rs`, `crates/cli/src/main.rs`

---

## Epic 3: Result Comparison & Analysis (P2)

Tools for comparing search results and analyzing color consistency.

### Story 3.1: Feature Comparison View

As a 调色师,
I want to see a side-by-side comparison of two fingerprints' grading features,
so that I can understand exactly how two clips differ in color grading.

**Acceptance Criteria:**

1. Given two fingerprint IDs, when `mengxi compare <id1> <id2>` runs, then a side-by-side feature breakdown is displayed
2. Given comparison output, when in text mode, then histogram deltas, moment deltas, and color space differences are shown
3. Given comparison output, when `--json` is used, then structured feature diff JSON is output to stdout
4. Given fingerprints from different color spaces, when compared, then a color space gap warning is shown

**Files:** `crates/cli/src/main.rs`, `crates/core/src/comparison.rs` (new)

### Story 3.2: Cross-Project Consistency Report

As a 后期总监,
I want to check color consistency across multiple projects,
so that I can identify grading drift between projects.

**Acceptance Criteria:**

1. Given multiple projects, when `mengxi consistency --projects proj1,proj2,proj3` runs, then a consistency report is generated
2. Given the report, when displayed, then it shows average feature distance between projects and outlier fingerprints
3. Given `--json` flag, when run, then structured report JSON is output to stdout

**Files:** `crates/cli/src/main.rs`, `crates/core/src/consistency.rs` (new)

---

## Epic 4: Advanced Color Processing (P2)

Deeper color science improvements from v2 architecture deferred items.

### Story 4.1: Soft Clamp Histogram Edge Handling

As a 开发者,
I want histogram bins to use soft clamping at edges,
so that boundary pixels don't create artificial spikes.

**Acceptance Criteria:**

1. Given pixels at exactly 0.0 or 1.0, when histograms are computed, then edge bins don't receive disproportionate weight
2. Given the change, when tests run, then ΔE round-trip tests still pass

**Files:** `moonbit/src/lib/` (histogram computation), `moonbit/src/lib/ffi.c`

### Story 4.2: Extended Color Moments (Skewness & Kurtosis)

As a 开发者,
I want color moments to include skewness and kurtosis,
so that the feature vector captures distribution shape, not just center and spread.

**Acceptance Criteria:**

1. Given 6 additional moment values (skewness_l/a/b, kurtosis_l/a/b), when features are extracted, then they are stored in the DB
2. Given existing fingerprints, when migration runs, then they are marked stale for re-extraction
3. Given the extended moments, when hybrid scoring runs, then they contribute to the distance metric

**Files:** `moonbit/src/lib/` (extraction), `moonbit/src/lib/ffi.c`, `crates/core/src/grading_features.rs`, `crates/core/src/search.rs`, new migration

---

## Epic-to-FR Mapping

| FR (from PRD v2 Phase 3) | Epic | Notes |
|--------------------------|------|-------|
| Feature comparison view | Epic 3 | Side-by-side diff |
| HNSW vector index | Epic 2 | >10K scalability |
| Batch embedding generation | Epic 2 | Bulk CLIP regeneration |
| Layered feature storage | Epic 4 | Extended moments |
| color_tag-aware processing | Epic 4 | Deferred to later |
| Partial reference search | — | Deferred (too complex) |
| Cross-project consistency | Epic 3 | Consistency report |
| CLIP fine-tuning | — | Deferred (requires ML infra) |

## Out of Scope (Deferred)

- Partial reference image search (requires image segmentation)
- CLIP fine-tuning (requires ML training infrastructure)
- Web UI (separate project)
- Real-time collaborative features

## Implementation Sequence

1. **Epic 1** first — clears tech debt, unlocks configurability
2. **Epic 2** second — scalability for production use
3. **Epic 3 & 4** in parallel — comparison tooling and color science improvements
