use serde::{Deserialize, Serialize};

use crate::greeks::require_finite;
use crate::ids::PolicyId;
use crate::reject::{Reject, RejectCode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Regime {
    Calm,
    VolExpanding,
    Stress,
    Unknown,
}

/// Strategist output / file policy. Extra JSON keys are a PARSE reject via serde.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub policy_id: PolicyId,
    pub regime: Regime,
    pub dte_min: u32,
    pub dte_max: u32,
    pub delta_min: f64,
    pub delta_max: f64,
    pub max_premium_cents: i64,
    pub lambda_svi: f64,
    pub lambda_pca: f64,
    pub lambda_eff: f64,
    pub reason: String,
}

impl Policy {
    /// Default file policy when LLM is off (DTE 30–60, put Δ [−0.50, −0.20]).
    pub fn file_default() -> Self {
        Self {
            policy_id: PolicyId::file_default(),
            regime: Regime::Unknown,
            dte_min: 30,
            dte_max: 60,
            delta_min: -0.50,
            delta_max: -0.20,
            max_premium_cents: 100_000,
            lambda_svi: 1.0,
            lambda_pca: 1.0,
            lambda_eff: 1.0,
            reason: "file-default".into(),
        }
    }

    pub fn validate_physics(&self) -> Result<(), Reject> {
        if !(1..=365).contains(&self.dte_min) || !(1..=365).contains(&self.dte_max) {
            return Err(Reject::new(
                RejectCode::RangeDte,
                "dte",
                format!("{}-{}", self.dte_min, self.dte_max),
                "dte out of 1..=365",
            ));
        }
        if self.dte_min > self.dte_max {
            return Err(Reject::new(
                RejectCode::RangeDte,
                "dte",
                format!("{}-{}", self.dte_min, self.dte_max),
                "dte_min > dte_max",
            ));
        }
        let lo = require_finite("delta_min", self.delta_min)?;
        let hi = require_finite("delta_max", self.delta_max)?;
        if !(-1.0..=0.0).contains(&lo) || !(-1.0..=0.0).contains(&hi) || lo > hi {
            return Err(Reject::new(
                RejectCode::RangeDelta,
                "delta",
                format!("{lo}/{hi}"),
                "put delta band invalid",
            ));
        }
        if self.max_premium_cents < 0 {
            return Err(Reject::new(
                RejectCode::PremiumCap,
                "max_premium_cents",
                self.max_premium_cents.to_string(),
                "negative premium cap",
            ));
        }
        require_finite("lambda_svi", self.lambda_svi)?;
        require_finite("lambda_pca", self.lambda_pca)?;
        require_finite("lambda_eff", self.lambda_eff)?;
        if self.lambda_svi < 0.0 || self.lambda_pca < 0.0 || self.lambda_eff < 0.0 {
            return Err(Reject::new(
                RejectCode::Lambda,
                "lambda",
                format!(
                    "{}/{}/{}",
                    self.lambda_svi, self.lambda_pca, self.lambda_eff
                ),
                "lambda < 0",
            ));
        }
        if self.reason.len() > 240 {
            return Err(Reject::new(
                RejectCode::Parse,
                "reason",
                self.reason.len().to_string(),
                "reason longer than 240",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok_json() -> &'static str {
        r#"{
            "policy_id": "file-default-policy",
            "regime": "unknown",
            "dte_min": 30,
            "dte_max": 60,
            "delta_min": -0.5,
            "delta_max": -0.2,
            "max_premium_cents": 100000,
            "lambda_svi": 1.0,
            "lambda_pca": 1.0,
            "lambda_eff": 1.0,
            "reason": "file-default"
        }"#
    }

    #[test]
    fn extra_json_field_is_parse_fail() {
        let raw = r#"{
            "policy_id": "file-default-policy",
            "regime": "unknown",
            "dte_min": 30,
            "dte_max": 60,
            "delta_min": -0.5,
            "delta_max": -0.2,
            "max_premium_cents": 100000,
            "lambda_svi": 1.0,
            "lambda_pca": 1.0,
            "lambda_eff": 1.0,
            "reason": "x",
            "extra": true
        }"#;
        let err = serde_json::from_str::<Policy>(raw).unwrap_err();
        assert!(err.to_string().contains("unknown field"));
    }

    #[test]
    fn file_default_validates() {
        let p = Policy::file_default();
        p.validate_physics().unwrap();
        let round = serde_json::from_str::<Policy>(ok_json()).unwrap();
        assert_eq!(round.dte_min, 30);
    }

    #[test]
    fn nan_lambda_fails_closed() {
        let mut p = Policy::file_default();
        p.lambda_svi = f64::NAN;
        let err = p.validate_physics().unwrap_err();
        assert_eq!(err.code, RejectCode::Lambda);
    }
}
