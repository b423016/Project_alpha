use serde::{Deserialize, Serialize};

use crate::reject::{Reject, RejectCode};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Greeks {
    pub delta: f64,
    pub gamma: f64,
    pub theta: f64,
    pub vega: f64,
    pub iv: f64,
}

impl Greeks {
    pub fn require_finite(&self) -> Result<(), Reject> {
        for (field, v) in [
            ("delta", self.delta),
            ("gamma", self.gamma),
            ("theta", self.theta),
            ("vega", self.vega),
            ("iv", self.iv),
        ] {
            if !v.is_finite() {
                return Err(Reject::new(
                    RejectCode::Lambda,
                    field,
                    v.to_string(),
                    "non-finite greek",
                ));
            }
        }
        Ok(())
    }
}

pub fn require_finite(field: &'static str, v: f64) -> Result<f64, Reject> {
    if v.is_finite() {
        Ok(v)
    } else {
        Err(Reject::new(
            RejectCode::Lambda,
            field,
            v.to_string(),
            "non-finite",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nan_lambda_fails_closed() {
        let err = require_finite("lambda_svi", f64::NAN).unwrap_err();
        assert_eq!(err.code, RejectCode::Lambda);
        assert_eq!(err.field, "lambda_svi");
    }
}
