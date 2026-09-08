//! Overlay analytics: IV/Greeks, funnel, Δ band. Not a live GNN.

mod band;
mod error;
mod funnel;
mod iv;
mod names;

pub use band::{BandStatus, band_status, dollar_delta, dollar_delta_stock};
pub use error::MlError;
pub use funnel::{argmax_utility, decide_cpu_ms, funnel};
pub use iv::{greeks_put, implied_vol_put, put_price};
pub use names::{
    FeatureCell, NameBar, NameFeatures, features_from_closes, return_heatmap, synth_bars,
};
