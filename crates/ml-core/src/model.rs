use neural_router_config::Settings;

/// GNN + temporal transformer handle. Weights are not loaded yet.
#[derive(Debug, Clone)]
pub struct NeuralOrderBookModel {
    pub levels: usize,
    pub horizon_us: u64,
}

impl NeuralOrderBookModel {
    pub fn from_settings(settings: &Settings) -> Self {
        Self {
            levels: settings.order_book_levels,
            horizon_us: settings.prediction_horizon_us,
        }
    }
}
