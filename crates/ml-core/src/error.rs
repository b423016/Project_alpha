use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MlError {
    #[error("not implemented: {feature}")]
    NotImplemented { feature: &'static str },
    #[error("{0}")]
    Constraint(&'static str),
}
