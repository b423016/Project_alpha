//! Market-data gateway: chain ingest, L0/L1 cache, validation.

mod cache;
mod error;
mod fixture;
mod loader;
mod preprocessor;
mod snapshot;
mod validator;

pub use cache::{IngestCache, NegativeCache};
pub use error::DataError;
pub use fixture::{fixture_path, load_fixture, load_fixture_bytes};
pub use loader::{collect, ChainSource, L2Source, PlaceholderChainSource};
pub use preprocessor::{order_imbalance, Preprocessor};
pub use snapshot::{
    current_or_stale, validate_chain, ChainSnapshot, RawChain, SnapshotRing,
};
pub use validator::validate_snapshot;
