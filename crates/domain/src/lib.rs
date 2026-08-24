//! Shared overlay types. No I/O.

mod greeks;
mod ids;
mod occ;
mod option_contract;
mod order_book;
mod policy;
mod prediction;
mod reject;
mod side;
mod stamps;
mod ticket;

pub use greeks::{require_finite, Greeks};
pub use ids::{PolicyId, SnapshotId};
pub use occ::OccSymbol;
pub use option_contract::{OptionContract, OptionRight};
pub use order_book::{OrderBookSnapshot, PriceLevel};
pub use policy::{Policy, Regime};
pub use prediction::Prediction;
pub use reject::{Reject, RejectCode};
pub use side::Side;
pub use stamps::Stamps;
pub use ticket::{NewOrder, TicketProposal, TicketSide, TimeInForce};

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
