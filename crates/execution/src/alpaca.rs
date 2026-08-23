use neural_router_config::Settings;

use crate::{ExecutionError, NewOrder};

pub trait Broker {
    fn submit(&self, order: &NewOrder) -> Result<(), ExecutionError>;
    fn position(&self, symbol: &str) -> Result<f64, ExecutionError>;
}

/// Alpaca paper/live adapter. Transport is not wired yet.
/// Credentials are checked at construction and are not stored on the client.
#[derive(Debug, Clone)]
pub struct AlpacaClient {
    paper: bool,
}

impl AlpacaClient {
    pub fn from_settings(settings: &Settings) -> Result<Self, ExecutionError> {
        if settings.alpaca_api_key.is_none() || settings.alpaca_secret_key.is_none() {
            return Err(ExecutionError::MissingCredentials);
        }
        Ok(Self {
            paper: settings.alpaca_paper,
        })
    }
}

impl Broker for AlpacaClient {
    fn submit(&self, order: &NewOrder) -> Result<(), ExecutionError> {
        tracing::info!(
            paper = self.paper,
            symbol = %order.symbol,
            side = ?order.side,
            notional = order.notional,
            "alpaca submit"
        );
        Err(ExecutionError::NotImplemented {
            feature: "alpaca_submit_order",
        })
    }

    fn position(&self, symbol: &str) -> Result<f64, ExecutionError> {
        tracing::info!(%symbol, "alpaca position");
        Err(ExecutionError::NotImplemented {
            feature: "alpaca_get_position",
        })
    }
}

#[cfg(test)]
mod tests {
    use neural_router_config::Settings;

    use super::*;

    #[test]
    fn fails_closed_without_keys() {
        assert!(matches!(
            AlpacaClient::from_settings(&Settings::default()),
            Err(ExecutionError::MissingCredentials)
        ));
    }
}
