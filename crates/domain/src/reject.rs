use serde::{Deserialize, Serialize};

/// Stable catalog from `docs/lld.md` §6. New codes require an LLD edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RejectCode {
    Parse,
    RangeDte,
    RangeDelta,
    Lambda,
    PremiumCap,
    StaleSnap,
    StalePolicy,
    NotInTop20,
    QtyRecompute,
    LimitAway,
    Rth,
    DailyLoss,
    OverHedge,
    Breaker,
    StaleData,
    StalePos,
    BrainDown,
    MissingCreds,
    NotPaper,
}

impl RejectCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Parse => "PARSE",
            Self::RangeDte => "RANGE_DTE",
            Self::RangeDelta => "RANGE_DELTA",
            Self::Lambda => "LAMBDA",
            Self::PremiumCap => "PREMIUM_CAP",
            Self::StaleSnap => "STALE_SNAP",
            Self::StalePolicy => "STALE_POLICY",
            Self::NotInTop20 => "NOT_IN_TOP20",
            Self::QtyRecompute => "QTY_RECOMPUTE",
            Self::LimitAway => "LIMIT_AWAY",
            Self::Rth => "RTH",
            Self::DailyLoss => "DAILY_LOSS",
            Self::OverHedge => "OVER_HEDGE",
            Self::Breaker => "BREAKER",
            Self::StaleData => "STALE_DATA",
            Self::StalePos => "STALE_POS",
            Self::BrainDown => "BRAIN_DOWN",
            Self::MissingCreds => "MISSING_CREDS",
            Self::NotPaper => "NOT_PAPER",
        }
    }
}

impl std::fmt::Display for RejectCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reject {
    pub code: RejectCode,
    pub field: &'static str,
    pub got: String,
    pub message: String,
}

impl Reject {
    pub fn new(
        code: RejectCode,
        field: &'static str,
        got: impl Into<String>,
        message: &'static str,
    ) -> Self {
        Self {
            code,
            field,
            got: got.into(),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for Reject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} field={} got={} {}",
            self.code, self.field, self.got, self.message
        )
    }
}

impl std::error::Error for Reject {}
