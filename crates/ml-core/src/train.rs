use crate::{MlError, NeuralOrderBookModel};

#[derive(Debug, Clone, Copy)]
pub struct TrainConfig {
    pub epochs: u32,
    pub batch_size: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct TrainReport {
    pub epochs: u32,
}

pub fn train(
    _model: &mut NeuralOrderBookModel,
    config: TrainConfig,
) -> Result<TrainReport, MlError> {
    tracing::info!(
        epochs = config.epochs,
        batch_size = config.batch_size,
        "train"
    );
    Err(MlError::NotImplemented {
        feature: "gnn_transformer_training",
    })
}
