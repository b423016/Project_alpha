//! Model, physics constraints, training, and inference APIs.

mod constraints;
mod error;
mod model;
mod predict;
mod train;

pub use constraints::apply_constraints;
pub use error::MlError;
pub use model::NeuralOrderBookModel;
pub use predict::predict;
pub use train::{TrainConfig, TrainReport, train};
