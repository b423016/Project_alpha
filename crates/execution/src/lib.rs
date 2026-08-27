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
pub use audit::{AuditEvent, DecideHist, MemoryAudit, append_file, submit_after_audit};
pub use blotter::{Blotter, BlotterRow, OrderState};
pub use error::ExecutionError;
pub use gate::{GateLimits, client_order_id, gate, kernel_qty};
pub use overlay_broker::{
    AlpacaOverlay, LIVE_BASE, MockPaperBroker, OverlayBroker, PAPER_BASE, ScriptedHttpBroker,
    SubmitAck, recon_position,
};
pub use risk::{RiskDecision, RiskManager};
pub use router::{NewOrder, RouteDecision, decide, route};
