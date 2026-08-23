use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DataError {
    #[error("not implemented: {feature}")]
    NotImplemented { feature: &'static str },
    #[error("{0}")]
    InvalidSnapshot(&'static str),
}
