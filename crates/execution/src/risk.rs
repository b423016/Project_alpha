use neural_router_config::Settings;
use neural_router_domain::{Reject, RejectCode, RiskState};

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

    pub fn overlay(risk_frac: f64, max_daily_loss: f64) -> Self {
        Self {
            risk_limit_per_trade: risk_frac,
            max_daily_loss,
        }
    }

    pub fn risk_frac(&self) -> f64 {
        self.risk_limit_per_trade
    }

    /// Overlay book in cents. Fail closed: breaker, equity, 5% daily loss.
    pub fn check_overlay(&self, risk: &RiskState) -> Result<(), Reject> {
        if risk.breaker {
            return Err(Reject::new(
                RejectCode::Breaker,
                "breaker",
                risk.rejection_count.to_string(),
                "circuit breaker",
            ));
        }
        if risk.equity_cents <= 0 {
            return Err(Reject::new(
                RejectCode::PremiumCap,
                "equity_cents",
                risk.equity_cents.to_string(),
                "non-positive equity",
            ));
        }
        let loss_lim = (risk.equity_cents as f64 * self.max_daily_loss) as i64;
        if risk.daily_pnl_cents <= -loss_lim {
            return Err(Reject::new(
                RejectCode::DailyLoss,
                "daily_pnl_cents",
                risk.daily_pnl_cents.to_string(),
                "daily loss limit",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manager() -> RiskManager {
        RiskManager::overlay(0.01, 0.05)
    }

    #[test]
    fn one_percent_from_settings() {
        assert!((manager().risk_frac() - 0.01).abs() < 1e-12);
        let m = RiskManager::from_settings(&neural_router_config::Settings::default());
        assert!((m.risk_frac() - 0.01).abs() < 1e-12);
    }

    #[test]
    fn trips_daily_loss() {
        let mut state = RiskState::paper_book(10_000_000);
        state.daily_pnl_cents = -500_000;
        assert_eq!(
            manager().check_overlay(&state).unwrap_err().code,
            RejectCode::DailyLoss
        );
    }

    #[test]
    fn allows_inside_limits() {
        let state = RiskState::paper_book(10_000_000);
        manager().check_overlay(&state).unwrap();
    }
}
