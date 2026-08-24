use neural_router_config::Settings;
use neural_router_domain::OrderBookSnapshot;

use crate::snapshot::RawChain;
use crate::DataError;

/// Boundary for an L2 feed. Polygon (or any other venue) implements this.
pub trait L2Source {
    fn next_snapshot(&mut self) -> Result<Option<OrderBookSnapshot>, DataError>;
}

/// Options chain source. Vendor HTTP stays behind this trait (placeholder).
pub trait ChainSource {
    fn fetch(&mut self) -> Result<RawChain, DataError>;
}

/// Placeholder vendor. Fails closed until a real adapter is wired.
pub struct PlaceholderChainSource;

impl ChainSource for PlaceholderChainSource {
    fn fetch(&mut self) -> Result<RawChain, DataError> {
        Err(DataError::NotImplemented {
            feature: "polygon_or_yfinance_chain",
        })
    }
}

pub fn collect(_settings: &Settings, symbol: &str) -> Result<(), DataError> {
    tracing::info!(%symbol, "collector entrypoint");
    Err(DataError::NotImplemented {
        feature: "polygon_l2_ingest",
    })
}
