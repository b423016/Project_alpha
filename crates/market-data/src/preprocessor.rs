use neural_router_domain::OrderBookSnapshot;

/// Feature extraction over a validated snapshot.
#[derive(Debug, Default, Clone, Copy)]
pub struct Preprocessor;

impl Preprocessor {
    pub fn imbalance(&self, book: &OrderBookSnapshot) -> Option<f64> {
        order_imbalance(book)
    }
}

/// `(bid_vol - ask_vol) / (bid_vol + ask_vol)` across all provided levels.
pub fn order_imbalance(book: &OrderBookSnapshot) -> Option<f64> {
    let bid_vol: f64 = book.bids.iter().map(|level| level.size).sum();
    let ask_vol: f64 = book.asks.iter().map(|level| level.size).sum();
    let denom = bid_vol + ask_vol;
    if denom == 0.0 {
        None
    } else {
        Some((bid_vol - ask_vol) / denom)
    }
}

#[cfg(test)]
mod tests {
    use neural_router_domain::{OrderBookSnapshot, PriceLevel};

    use super::*;

    fn book(bid_size: f64, ask_size: f64) -> OrderBookSnapshot {
        OrderBookSnapshot {
            symbol: "SPY".into(),
            timestamp_us: 1,
            bids: vec![PriceLevel {
                price: 100.0,
                size: bid_size,
            }],
            asks: vec![PriceLevel {
                price: 100.1,
                size: ask_size,
            }],
        }
    }

    #[test]
    fn imbalance_is_signed_volume_ratio() {
        let value = order_imbalance(&book(70.0, 30.0)).unwrap();
        assert!((value - 0.4).abs() < 1e-12);
    }

    #[test]
    fn imbalance_none_when_empty() {
        assert_eq!(order_imbalance(&book(0.0, 0.0)), None);
    }
}
