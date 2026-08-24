//! Overlay analytics: IV/Greeks, funnel, Δ band. Not a live GNN.

mod band;
mod constraints;
mod error;
mod funnel;
mod iv;
mod model;
mod predict;
mod train;

pub use band::{band_status, dollar_delta_stock, BandStatus};
pub use constraints::apply_constraints;
pub use error::MlError;
pub use funnel::{argmax_utility, decide_cpu_ms, funnel};
pub use iv::{greeks_put, implied_vol_put, put_price};
pub use model::NeuralOrderBookModel;
pub use predict::predict;
pub use train::{train, TrainConfig, TrainReport};
