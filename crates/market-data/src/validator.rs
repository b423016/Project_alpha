use neural_router_domain::OrderBookSnapshot;

use crate::DataError;

pub fn validate_snapshot(
    book: &OrderBookSnapshot,
    expected_levels: usize,
) -> Result<(), DataError> {
    if book.symbol.is_empty() {
        return Err(DataError::InvalidSnapshot("missing symbol"));
    }
    if book.bids.is_empty() || book.asks.is_empty() {
        return Err(DataError::InvalidSnapshot("empty side"));
    }
    if book.bids.len() < expected_levels || book.asks.len() < expected_levels {
        return Err(DataError::InvalidSnapshot("insufficient depth"));
    }
    if book.bids.iter().any(|l| l.price <= 0.0 || l.size < 0.0)
        || book.asks.iter().any(|l| l.price <= 0.0 || l.size < 0.0)
    {
        return Err(DataError::InvalidSnapshot("non-positive price or size"));
    }
    if book.bids.windows(2).any(|w| w[0].price < w[1].price) {
        return Err(DataError::InvalidSnapshot("bids not best-first"));
    }
    if book.asks.windows(2).any(|w| w[0].price > w[1].price) {
        return Err(DataError::InvalidSnapshot("asks not best-first"));
    }
    match book.spread() {
        Some(spread) if spread >= 0.0 => Ok(()),
        _ => Err(DataError::InvalidSnapshot("crossed or missing spread")),
    }
}

#[cfg(test)]
mod tests {
    use neural_router_domain::{OrderBookSnapshot, PriceLevel};

    use super::*;

    fn level(price: f64, size: f64) -> PriceLevel {
        PriceLevel { price, size }
    }

    fn valid_book() -> OrderBookSnapshot {
        OrderBookSnapshot {
            symbol: "SPY".into(),
            timestamp_us: 1,
            bids: vec![level(100.0, 1.0), level(99.9, 2.0)],
            asks: vec![level(100.1, 1.0), level(100.2, 2.0)],
        }
    }

    #[test]
    fn accepts_ordered_book() {
        assert!(validate_snapshot(&valid_book(), 2).is_ok());
    }

    #[test]
    fn rejects_crossed_book() {
        let mut book = valid_book();
        book.bids[0].price = 101.0;
        assert!(validate_snapshot(&book, 2).is_err());
    }

    #[test]
    fn rejects_shallow_book() {
        assert!(validate_snapshot(&valid_book(), 10).is_err());
    }
}
