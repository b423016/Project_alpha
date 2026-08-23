use serde::{Deserialize, Serialize};

/// Model output for one snapshot and horizon.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Prediction {
    pub spread_widening_prob: f64,
    pub spread_narrowing_prob: f64,
    pub adverse_selection_prob: f64,
    pub confidence: f64,
    pub horizon_us: u64,
}
