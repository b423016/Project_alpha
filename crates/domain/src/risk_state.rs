use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskState {
    pub equity_cents: i64,
    pub daily_pnl_cents: i64,
    pub rejection_count: u32,
    pub breaker: bool,
}

impl RiskState {
    pub fn paper_book(equity_cents: i64) -> Self {
        Self {
            equity_cents,
            daily_pnl_cents: 0,
            rejection_count: 0,
            breaker: false,
        }
    }

    pub fn bump_reject(&mut self, trip_at: u32) {
        self.rejection_count = self.rejection_count.saturating_add(1);
        if self.rejection_count >= trip_at {
            self.breaker = true;
        }
    }
}
