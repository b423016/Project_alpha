//! L2 ingest, feature extraction, and snapshot validation.

mod error;
mod loader;
mod preprocessor;
mod validator;

pub use error::DataError;
pub use loader::{L2Source, collect};
pub use preprocessor::{Preprocessor, order_imbalance};
pub use validator::validate_snapshot;
