use neural_router_domain::{Reject, RejectCode};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PolicyError {
    #[error("brain down: {0}")]
    BrainDown(String),
    #[error(transparent)]
    Reject(#[from] Reject),
}

impl PolicyError {
    pub fn reject(&self) -> Reject {
        match self {
            Self::BrainDown(msg) => Reject::new(
                RejectCode::BrainDown,
                "llm",
                msg.clone(),
                "transport or empty",
            ),
            Self::Reject(r) => r.clone(),
        }
    }
}
