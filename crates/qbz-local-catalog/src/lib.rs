//! Frontend-agnostic derived catalog for Local Library.
//!
//! Source databases remain authoritative. This crate owns only a rebuildable
//! read projection and query primitives; it has no dependency on Qt, QML,
//! playback, or any source protocol/API.

mod catalog;
mod model;
mod schema;

pub use catalog::{
    normalize_artist_key, normalize_sort_key, Catalog, CatalogStats, IntegrityReport, QueryMetrics,
};
pub use model::{
    ArtistCredit, CreditRole, ProjectedTrack, QueryDescriptor, QuerySurface, SourceKey, SourceKind,
    TrackCursor, TrackGroup, TrackPage, TrackRecord, TrackRef, TrackSort,
};
pub use schema::SCHEMA_VERSION;

#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    #[error("catalog sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("catalog schema {found} is not supported (expected {expected})")]
    UnsupportedSchema { found: u32, expected: u32 },
    #[error("refusing to open a non-catalog SQLite database")]
    NotCatalog,
    #[error("invalid catalog input: {0}")]
    InvalidInput(String),
    #[error("search terms shorter than three characters are deferred")]
    SearchTooShort,
    #[error("a cursor from a different query descriptor cannot be reused")]
    CursorDescriptorMismatch,
}

pub type Result<T> = std::result::Result<T, CatalogError>;

#[cfg(test)]
mod tests;
