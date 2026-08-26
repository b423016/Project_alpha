use serde::{Deserialize, Serialize};

/// One price level on a single side of the book.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PriceLevel {
    pub price: f64,
    pub size: f64,
}

/// L2 snapshot at a single timestamp. Bids and asks are best-first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderBookSnapshot {
    pub symbol: String,
    pub timestamp_us: u64,
    pub bids: Vec<PriceLevel>,
    pub asks: Vec<PriceLevel>,
}

impl OrderBookSnapshot {
    pub fn mid(&self) -> Option<f64> {
        let bid = self.bids.first()?.price;
        let ask = self.asks.first()?.price;
        Some((bid + ask) / 2.0)
    }

    pub fn spread(&self) -> Option<f64> {
        let bid = self.bids.first()?.price;
        let ask = self.asks.first()?.price;
        Some(ask - bid)
    }
}
