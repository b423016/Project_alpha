use neural_router_domain::{OrderBookSnapshot, Prediction};

use crate::{MlError, NeuralOrderBookModel};

pub fn predict(
    _model: &NeuralOrderBookModel,
    _book: &OrderBookSnapshot,
) -> Result<Prediction, MlError> {
    Err(MlError::NotImplemented {
        feature: "gnn_transformer_inference",
    })
}
