use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ExecutionError {
    #[error("missing broker credentials")]
    MissingCredentials,
    #[error("live trading requires ALPACA_PAPER=false and ALLOW_LIVE=1")]
    NotPaper,
    #[error("broker http {0}")]
    Http(u16),
    #[error("broker http {0}: {1}")]
    HttpMsg(u16, String),
}
