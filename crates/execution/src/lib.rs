//! OMS gate, blotter, paper EMS. No L2/GNN router.

mod audit;
mod blotter;
mod error;
mod gate;
mod overlay_broker;
mod risk;

pub use audit::{AuditEvent, DecideHist, MemoryAudit, append_file, now_ms, submit_after_audit};
pub use blotter::{Blotter, BlotterRow, OrderState};
pub use error::ExecutionError;
pub use gate::{GateLimits, client_order_id, gate, kernel_qty};
pub use overlay_broker::{
    AlpacaOverlay, Broker, LIVE_BASE, MockPaperBroker, PAPER_BASE, PaperAccount, SubmitAck,
    recon_position,
};
pub use risk::RiskManager;
