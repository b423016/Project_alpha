//! Process configuration. Secrets come from the environment only.

use std::env;
use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error("invalid {key}: {value}")]
    Invalid { key: &'static str, value: String },
}

/// Runtime settings. API keys stay optional so tests and local builds
/// do not require credentials; brokers must fail closed if they are missing.
#[derive(Clone)]
pub struct Settings {
    pub polygon_api_key: Option<String>,
    pub alpaca_api_key: Option<String>,
    pub alpaca_secret_key: Option<String>,
    pub alpaca_paper: bool,
    pub mongo_uri: Option<String>,
    pub prediction_horizon_us: u64,
    pub order_book_levels: usize,
    pub risk_limit_per_trade: f64,
    pub max_daily_loss: f64,
    pub symbol: String,
    pub data_dir: PathBuf,
    pub model_dir: PathBuf,
    pub log_dir: PathBuf,
}

impl fmt::Debug for Settings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Settings")
            .field("polygon_api_key", &redact(&self.polygon_api_key))
            .field("alpaca_api_key", &redact(&self.alpaca_api_key))
            .field("alpaca_secret_key", &redact(&self.alpaca_secret_key))
            .field("alpaca_paper", &self.alpaca_paper)
            .field("mongo_uri", &redact(&self.mongo_uri))
            .field("prediction_horizon_us", &self.prediction_horizon_us)
            .field("order_book_levels", &self.order_book_levels)
            .field("risk_limit_per_trade", &self.risk_limit_per_trade)
            .field("max_daily_loss", &self.max_daily_loss)
            .field("symbol", &self.symbol)
            .field("data_dir", &self.data_dir)
            .field("model_dir", &self.model_dir)
            .field("log_dir", &self.log_dir)
            .finish()
    }
}

fn redact(value: &Option<String>) -> Option<&'static str> {
    value.as_ref().map(|_| "[REDACTED]")
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            polygon_api_key: None,
            alpaca_api_key: None,
            alpaca_secret_key: None,
            alpaca_paper: true,
            mongo_uri: None,
            prediction_horizon_us: 500,
            order_book_levels: 10,
            risk_limit_per_trade: 0.01,
            max_daily_loss: 0.05,
            symbol: "SPY".into(),
            data_dir: PathBuf::from("data"),
            model_dir: PathBuf::from("models"),
            log_dir: PathBuf::from("logs"),
        }
    }
}

impl Settings {
    pub fn from_env() -> Result<Self, ConfigError> {
        Ok(Self {
            polygon_api_key: env_optional("POLYGON_API_KEY"),
            alpaca_api_key: env_optional("ALPACA_API_KEY"),
            alpaca_secret_key: env_optional("ALPACA_SECRET_KEY"),
            alpaca_paper: env_bool("ALPACA_PAPER", true)?,
            mongo_uri: env_optional("MONGO_URI"),
            prediction_horizon_us: env_parse("PREDICTION_HORIZON", 500)?,
            order_book_levels: env_parse("ORDER_BOOK_LEVELS", 10)?,
            risk_limit_per_trade: env_parse("RISK_LIMIT_PER_TRADE", 0.01)?,
            max_daily_loss: env_parse("MAX_DAILY_LOSS", 0.05)?,
            symbol: env_optional("SYMBOL").unwrap_or_else(|| "SPY".into()),
            data_dir: PathBuf::from(env_optional("DATA_DIR").unwrap_or_else(|| "data".into())),
            model_dir: PathBuf::from(env_optional("MODEL_DIR").unwrap_or_else(|| "models".into())),
            log_dir: PathBuf::from(env_optional("LOG_DIR").unwrap_or_else(|| "logs".into())),
        })
    }
}

fn env_optional(key: &str) -> Option<String> {
    match env::var(key) {
        Ok(v) if !v.is_empty() => Some(v),
        _ => None,
    }
}

fn env_parse<T: FromStr>(key: &'static str, default: T) -> Result<T, ConfigError>
where
    T::Err: std::fmt::Display,
{
    match env::var(key) {
        Err(_) => Ok(default),
        Ok(v) if v.is_empty() => Ok(default),
        Ok(v) => v
            .parse()
            .map_err(|_| ConfigError::Invalid { key, value: v }),
    }
}

fn env_bool(key: &'static str, default: bool) -> Result<bool, ConfigError> {
    match env::var(key) {
        Err(_) => Ok(default),
        Ok(v) if v.is_empty() => Ok(default),
        Ok(v) => parse_bool(key, &v),
    }
}

fn parse_bool(key: &'static str, value: &str) -> Result<bool, ConfigError> {
    match value.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" => Ok(true),
        "0" | "false" | "no" => Ok(false),
        _ => Err(ConfigError::Invalid {
            key,
            value: value.into(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn research_defaults() {
        let settings = Settings::default();
        assert_eq!(settings.symbol, "SPY");
        assert!(settings.alpaca_paper);
        assert_eq!(settings.order_book_levels, 10);
        assert_eq!(settings.prediction_horizon_us, 500);
        assert!((settings.risk_limit_per_trade - 0.01).abs() < 1e-12);
        assert!((settings.max_daily_loss - 0.05).abs() < 1e-12);
    }

    #[test]
    fn debug_redacts_secrets() {
        let settings = Settings {
            alpaca_api_key: Some("sk-real".into()),
            alpaca_secret_key: Some("secret-real".into()),
            ..Settings::default()
        };
        let rendered = format!("{settings:?}");
        assert!(!rendered.contains("sk-real"));
        assert!(!rendered.contains("secret-real"));
        assert!(rendered.contains("[REDACTED]"));
    }

    #[test]
    fn parse_bool_accepts_common_forms() {
        assert_eq!(parse_bool("ALPACA_PAPER", "true"), Ok(true));
        assert_eq!(parse_bool("ALPACA_PAPER", "0"), Ok(false));
        assert!(matches!(
            parse_bool("ALPACA_PAPER", "maybe"),
            Err(ConfigError::Invalid {
                key: "ALPACA_PAPER",
                ..
            })
        ));
    }
}
