use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ExecutionError {
    #[error("not implemented: {feature}")]
    NotImplemented { feature: &'static str },
    #[error("missing broker credentials")]
    MissingCredentials,
    #[error("risk rejected: {0}")]
    Risk(&'static str),
    #[error("live trading requires ALPACA_PAPER=false and ALLOW_LIVE=1")]
    NotPaper,
}
