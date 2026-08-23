use neural_router_config::Settings;
use neural_router_domain::OrderBookSnapshot;

use crate::DataError;

/// Boundary for an L2 feed. Polygon (or any other venue) implements this.
pub trait L2Source {
    fn next_snapshot(&mut self) -> Result<Option<OrderBookSnapshot>, DataError>;
}

pub fn collect(_settings: &Settings, symbol: &str) -> Result<(), DataError> {
    tracing::info!(%symbol, "collector entrypoint");
    Err(DataError::NotImplemented {
        feature: "polygon_l2_ingest",
    })
}
