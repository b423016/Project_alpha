//! Market-data gateway: chain ingest, L0/L1 cache, validation.

mod cache;
mod error;
mod fixture;
mod loader;
mod snapshot;

pub use cache::{IngestCache, NegativeCache};
pub use error::DataError;
pub use fixture::{fixture_path, load_fixture, load_fixture_bytes, load_raw_fixture};
pub use loader::{ChainSource, FixtureChainSource, PlaceholderChainSource, ingest};
pub use snapshot::{ChainSnapshot, RawChain, SnapshotRing, current_or_stale, validate_chain};
