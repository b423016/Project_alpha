use neural_router_config::Settings;

use crate::ExecutionError;

#[derive(Debug, Clone)]
pub struct RiskState {
    pub equity: f64,
    pub daily_pnl: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RiskDecision {
    pub size: f64,
}

#[derive(Debug, Clone)]
pub struct RiskManager {
    risk_limit_per_trade: f64,
    max_daily_loss: f64,
}

impl RiskManager {
    pub fn from_settings(settings: &Settings) -> Self {
        Self {
            risk_limit_per_trade: settings.risk_limit_per_trade,
            max_daily_loss: settings.max_daily_loss,
        }
    }

    pub fn size_for(&self, equity: f64) -> f64 {
        (equity * self.risk_limit_per_trade).max(0.0)
    }

    pub fn check(&self, state: &RiskState) -> Result<RiskDecision, ExecutionError> {
        if !state.equity.is_finite() || state.equity <= 0.0 {
            return Err(ExecutionError::Risk("non-positive equity"));
        }
        let loss_limit = state.equity * self.max_daily_loss;
        if state.daily_pnl.is_finite() && state.daily_pnl <= -loss_limit {
            return Err(ExecutionError::Risk("daily loss limit"));
        }
        Ok(RiskDecision {
            size: self.size_for(state.equity),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manager() -> RiskManager {
        RiskManager {
            risk_limit_per_trade: 0.01,
            max_daily_loss: 0.05,
        }
    }

    #[test]
    fn sizes_at_one_percent() {
        assert!((manager().size_for(100_000.0) - 1_000.0).abs() < 1e-9);
    }

    #[test]
    fn trips_daily_loss() {
        let state = RiskState {
            equity: 100_000.0,
            daily_pnl: -5_000.0,
        };
        assert!(matches!(
            manager().check(&state),
            Err(ExecutionError::Risk("daily loss limit"))
        ));
    }

    #[test]
    fn allows_inside_limits() {
        let state = RiskState {
            equity: 100_000.0,
            daily_pnl: -100.0,
        };
        let decision = manager().check(&state).unwrap();
        assert!((decision.size - 1_000.0).abs() < 1e-9);
    }
}
