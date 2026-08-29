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
    pub risk_limit_per_trade: f64,
    pub max_daily_loss: f64,
    pub symbol: String,
    pub data_dir: PathBuf,
    pub model_dir: PathBuf,
    pub log_dir: PathBuf,
    pub max_data_age_ms: u64,
    pub ingest_ttl_ms: u64,
    pub max_slippage: f64,
    pub rth_only: bool,
    pub llm_strategist: bool,
    pub llm_quant: bool,
    pub panic_hedge: bool,
    pub allow_live: bool,
    pub anthropic_api_key: Option<String>,
    pub anthropic_model: String,
    pub ui_token: Option<String>,
    pub audit_path: PathBuf,
    pub last_good_policy_path: PathBuf,
    pub bind: String,
}

impl fmt::Debug for Settings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Settings")
            .field("polygon_api_key", &redact(&self.polygon_api_key))
            .field("alpaca_api_key", &redact(&self.alpaca_api_key))
            .field("alpaca_secret_key", &redact(&self.alpaca_secret_key))
            .field("alpaca_paper", &self.alpaca_paper)
            .field("risk_limit_per_trade", &self.risk_limit_per_trade)
            .field("max_daily_loss", &self.max_daily_loss)
            .field("symbol", &self.symbol)
            .field("data_dir", &self.data_dir)
            .field("model_dir", &self.model_dir)
            .field("log_dir", &self.log_dir)
            .field("max_data_age_ms", &self.max_data_age_ms)
            .field("ingest_ttl_ms", &self.ingest_ttl_ms)
            .field("max_slippage", &self.max_slippage)
            .field("rth_only", &self.rth_only)
            .field("llm_strategist", &self.llm_strategist)
            .field("llm_quant", &self.llm_quant)
            .field("panic_hedge", &self.panic_hedge)
            .field("allow_live", &self.allow_live)
            .field("anthropic_api_key", &redact(&self.anthropic_api_key))
            .field("anthropic_model", &self.anthropic_model)
            .field("ui_token", &redact(&self.ui_token))
            .field("audit_path", &self.audit_path)
            .field("last_good_policy_path", &self.last_good_policy_path)
            .field("bind", &self.bind)
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
            risk_limit_per_trade: 0.01,
            max_daily_loss: 0.05,
            symbol: "SPY".into(),
            data_dir: PathBuf::from("data"),
            model_dir: PathBuf::from("models"),
            log_dir: PathBuf::from("logs"),
            max_data_age_ms: 900_000,
            ingest_ttl_ms: 900_000,
            max_slippage: 0.03,
            rth_only: true,
            llm_strategist: false,
            llm_quant: false,
            panic_hedge: false,
            allow_live: false,
            anthropic_api_key: None,
            anthropic_model: "claude-sonnet-4-5".into(),
            ui_token: None,
            audit_path: PathBuf::from("logs/audit.jsonl"),
            last_good_policy_path: PathBuf::from("logs/last_good_policy.json"),
            bind: "127.0.0.1:8080".into(),
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
            risk_limit_per_trade: env_parse("RISK_LIMIT_PER_TRADE", 0.01)?,
            max_daily_loss: env_parse("MAX_DAILY_LOSS", 0.05)?,
            symbol: env_optional("SYMBOL").unwrap_or_else(|| "SPY".into()),
            data_dir: PathBuf::from(env_optional("DATA_DIR").unwrap_or_else(|| "data".into())),
            model_dir: PathBuf::from(env_optional("MODEL_DIR").unwrap_or_else(|| "models".into())),
            log_dir: PathBuf::from(env_optional("LOG_DIR").unwrap_or_else(|| "logs".into())),
            max_data_age_ms: env_parse("MAX_DATA_AGE_MS", 900_000)?,
            ingest_ttl_ms: env_parse("INGEST_TTL_MS", 900_000)?,
            max_slippage: env_parse("MAX_SLIPPAGE", 0.03)?,
            rth_only: env_bool("RTH_ONLY", true)?,
            llm_strategist: env_bool("LLM_STRATEGIST", false)?,
            llm_quant: env_bool("LLM_QUANT", false)?,
            panic_hedge: env_bool("PANIC_HEDGE", false)?,
            allow_live: env::var("ALLOW_LIVE").ok().as_deref() == Some("1"),
            anthropic_api_key: env_optional("ANTHROPIC_API_KEY"),
            anthropic_model: env_optional("ANTHROPIC_MODEL")
                .unwrap_or_else(|| "claude-sonnet-4-5".into()),
            ui_token: env_optional("UI_TOKEN"),
            audit_path: PathBuf::from(
                env_optional("AUDIT_PATH").unwrap_or_else(|| "logs/audit.jsonl".into()),
            ),
            last_good_policy_path: PathBuf::from(
                env_optional("LAST_GOOD_POLICY_PATH")
                    .unwrap_or_else(|| "logs/last_good_policy.json".into()),
            ),
            bind: env_optional("BIND").unwrap_or_else(|| "127.0.0.1:8080".into()),
        })
    }

    /// Live orders require both paper=false and ALLOW_LIVE=1.
    pub fn live_trading_allowed(&self) -> bool {
        !self.alpaca_paper && self.allow_live
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
        assert!((settings.risk_limit_per_trade - 0.01).abs() < 1e-12);
        assert!((settings.max_daily_loss - 0.05).abs() < 1e-12);
        assert!(!settings.llm_strategist);
        assert!(!settings.llm_quant);
        assert!(!settings.panic_hedge);
        assert!(!settings.allow_live);
        assert!(settings.alpaca_paper);
        assert!(!settings.live_trading_allowed());
        assert!(settings.rth_only);
    }

    #[test]
    fn debug_redacts_secrets() {
        let settings = Settings {
            alpaca_api_key: Some("sk-real".into()),
            alpaca_secret_key: Some("secret-real".into()),
            anthropic_api_key: Some("sk-ant-real".into()),
            ..Settings::default()
        };
        let rendered = format!("{settings:?}");
        assert!(!rendered.contains("sk-real"));
        assert!(!rendered.contains("secret-real"));
        assert!(!rendered.contains("sk-ant-real"));
        assert!(rendered.contains("[REDACTED]"));
    }

    #[test]
    fn live_requires_double_flag() {
        let paper_off = Settings {
            alpaca_paper: false,
            ..Settings::default()
        };
        assert!(!paper_off.live_trading_allowed());
        let live = Settings {
            alpaca_paper: false,
            allow_live: true,
            ..Settings::default()
        };
        assert!(live.live_trading_allowed());
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
