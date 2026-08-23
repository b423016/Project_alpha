//! Shared market and trading types. No I/O.

mod order_book;
mod prediction;
mod side;

pub use order_book::{OrderBookSnapshot, PriceLevel};
pub use prediction::Prediction;
pub use side::Side;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mid_and_spread_from_top_of_book() {
        let book = OrderBookSnapshot {
            symbol: "SPY".into(),
            timestamp_us: 1,
            bids: vec![PriceLevel {
                price: 100.0,
                size: 10.0,
            }],
            asks: vec![PriceLevel {
                price: 100.2,
                size: 8.0,
            }],
        };
        assert!((book.mid().unwrap() - 100.1).abs() < 1e-9);
        assert!((book.spread().unwrap() - 0.2).abs() < 1e-9);
    }
}
