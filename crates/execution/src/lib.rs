//! OMS gate, blotter, overlay EMS, leftover signal router.

mod alpaca;
mod audit;
mod blotter;
mod error;
mod gate;
mod overlay_broker;
mod risk;
mod router;

pub use alpaca::{AlpacaClient, Broker};
pub use audit::{
    append_file, submit_after_audit, AuditEvent, DecideHist, MemoryAudit,
};
pub use blotter::{Blotter, BlotterRow, OrderState};
pub use error::ExecutionError;
pub use gate::{client_order_id, gate, kernel_qty};
pub use overlay_broker::{
    recon_position, AlpacaOverlay, MockPaperBroker, OverlayBroker, SubmitAck,
};
pub use risk::{RiskDecision, RiskManager};
pub use router::{decide, route, NewOrder, RouteDecision};
