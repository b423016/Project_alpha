use neural_router_domain::{Reject, RejectCode};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PolicyError {
    #[error("brain down: {0}")]
    BrainDown(&'static str),
    #[error(transparent)]
    Reject(#[from] Reject),
}

impl PolicyError {
    pub fn reject(&self) -> Reject {
        match self {
            Self::BrainDown(msg) => {
                Reject::new(RejectCode::BrainDown, "llm", *msg, "transport or empty")
            }
            Self::Reject(r) => r.clone(),
        }
    }
}
