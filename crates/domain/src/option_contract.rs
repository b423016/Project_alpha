use serde::{Deserialize, Serialize};

use crate::occ::OccSymbol;
use crate::reject::{Reject, RejectCode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OptionRight {
    Call,
    Put,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OptionContract {
    pub occ: OccSymbol,
    pub underlying: String,
    pub expiry: String,
    pub right: OptionRight,
    pub strike: f64,
    pub dte: u32,
    pub bid: f64,
    pub ask: f64,
    pub last: Option<f64>,
    pub oi: u64,
    pub volume: u64,
}

impl OptionContract {
    pub fn mid(&self) -> Option<f64> {
        if self.bid > 0.0 && self.ask >= self.bid {
            Some((self.bid + self.ask) / 2.0)
        } else {
            None
        }
    }

    pub fn mid_cents(&self) -> Option<i64> {
        self.mid().map(|m| (m * 100.0).round() as i64)
    }

    pub fn spread_pct(&self) -> Option<f64> {
        let mid = self.mid()?;
        if mid <= 0.0 {
            None
        } else {
            Some((self.ask - self.bid) / mid)
        }
    }

    pub fn check_quotes(&self) -> Result<(), Reject> {
        if self.bid <= 0.0 || self.ask <= 0.0 || self.ask < self.bid {
            return Err(Reject::new(
                RejectCode::Parse,
                "bid_ask",
                format!("{}/{}", self.bid, self.ask),
                "non-positive or crossed quotes",
            ));
        }
        if self.strike <= 0.0 {
            return Err(Reject::new(
                RejectCode::Parse,
                "strike",
                self.strike.to_string(),
                "non-positive strike",
            ));
        }
        Ok(())
    }
}
