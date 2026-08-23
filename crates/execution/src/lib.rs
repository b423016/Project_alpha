//! Broker adapter, signal routing, and risk controls.

mod alpaca;
mod error;
mod risk;
mod router;

pub use alpaca::{AlpacaClient, Broker};
pub use error::ExecutionError;
pub use risk::{RiskDecision, RiskManager, RiskState};
pub use router::{NewOrder, RouteDecision, decide, route};
