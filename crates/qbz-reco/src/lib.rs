//! Frontend-agnostic artist-vector recommendation engine (ADR-006).
//!
//! Cleanroom port of Tauri's `src-tauri/src/artist_vectors/` into a shared
//! crate so the Slint frontend (and any headless caller) can produce a
//! playlist's "Suggested Songs" without a `tauri::State` dependency.
//!
//! Modules are ported 1:1 from the Tauri source. The dead cosine-similarity /
//! `find_nearest` ranking path is dropped (production ranks by summed
//! relationship weight via the vector store) per the epic's decision D3.

mod builder;
mod sparse_vector;
mod store;
mod suggestions;
mod weights;

pub use builder::{ArtistVectorBuilder, BuildResult};
pub use sparse_vector::SparseVector;
pub use store::{ArtistVectorStore, SimilarArtist, VECTOR_TTL_SECS};
pub use suggestions::{
    extract_artist_mbids, SuggestedTrack, SuggestionConfig, SuggestionResult, SuggestionsEngine,
};
pub use weights::RelationshipWeights;
