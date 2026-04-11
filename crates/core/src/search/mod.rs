// mod.rs — Search module re-exports for backward compatibility
//
// This module was split from a single 2734-line search.rs file into
// focused submodules. All public APIs are re-exported here so that
// external code using `crate::search::Xxx` continues to work unchanged.

mod types;
mod histogram_utils;
mod embedding;
mod query;
mod histogram_search;
mod tag_search;
mod bhattacharyya_search;
mod image_search;
mod hybrid_search;

// Re-export all public types
pub use types::{SearchError, SearchResult, SearchOptions, FingerprintInfo, HistogramSummary};

// Re-export all public functions
pub use query::{fingerprint_info, fingerprint_info_with_tags};
pub use histogram_utils::{parse_histogram, histogram_intersection, cosine_similarity};
pub use embedding::{serialize_embedding, deserialize_embedding};
pub use histogram_search::search_histograms;
pub use tag_search::search_by_tag;
pub use bhattacharyya_search::bhattacharyya_search;
pub use image_search::search_by_image;
pub use hybrid_search::{
    search_by_image_and_tag, hybrid_search, hybrid_search_with_index,
};

// Re-export hybrid scoring types used in hybrid search results
pub use crate::hybrid_scoring::HybridSearchResult;
