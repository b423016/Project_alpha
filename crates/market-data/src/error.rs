use neural_router_domain::{Reject, RejectCode};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DataError {
    #[error("not implemented: {feature}")]
    NotImplemented { feature: &'static str },
    #[error("{0}")]
    InvalidSnapshot(&'static str),
    #[error("insufficient depth")]
    InsufficientDepth,
    #[error("stale data age_ms={age_ms}")]
    Stale { age_ms: i64 },
    #[error("vendor rate limited")]
    RateLimited,
    #[error("io: {0}")]
    Io(String),
}

impl DataError {
    pub fn reject(&self) -> Reject {
        match self {
            Self::Stale { age_ms } => Reject::new(
                RejectCode::StaleData,
                "data_age_ms",
                age_ms.to_string(),
                "snapshot too old",
            ),
            Self::InsufficientDepth => Reject::new(
                RejectCode::StaleData,
                "chain",
                "shallow",
                "fewer than 20 valid puts",
            ),
            Self::RateLimited => Reject::new(
                RejectCode::StaleData,
                "ingest",
                "429",
                "vendor rate limited",
            ),
            Self::InvalidSnapshot(msg) => {
                Reject::new(RejectCode::Parse, "snapshot", *msg, "invalid snapshot")
            }
            Self::Io(msg) => Reject::new(RejectCode::StaleData, "io", msg.clone(), "ingest io"),
            Self::NotImplemented { feature } => {
                Reject::new(RejectCode::StaleData, "ingest", *feature, "not implemented")
            }
        }
    }
}
