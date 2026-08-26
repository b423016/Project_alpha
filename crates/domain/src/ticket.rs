use serde::{Deserialize, Serialize};

use crate::ids::{PolicyId, SnapshotId};
use crate::occ::OccSymbol;
use crate::reject::{Reject, RejectCode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum TicketSide {
    Buy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum TimeInForce {
    Ioc,
    Fok,
}

/// Quant / argmax proposal. Extra keys fail serde. Qty is integer; `"two"` does not coerce.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TicketProposal {
    pub snapshot_id: SnapshotId,
    pub policy_id: PolicyId,
    pub occ_symbol: OccSymbol,
    pub side: TicketSide,
    pub qty: u32,
    pub limit_cents: i64,
    pub tif: TimeInForce,
    pub why: String,
}

impl TicketProposal {
    pub fn validate_shape(&self) -> Result<(), Reject> {
        if !(1..=1000).contains(&self.qty) {
            return Err(Reject::new(
                RejectCode::QtyRecompute,
                "qty",
                self.qty.to_string(),
                "qty out of 1..=1000",
            ));
        }
        if !(1..=100_000_000).contains(&self.limit_cents) {
            return Err(Reject::new(
                RejectCode::LimitAway,
                "limit_cents",
                self.limit_cents.to_string(),
                "limit out of range",
            ));
        }
        if self.why.len() > 240 {
            return Err(Reject::new(
                RejectCode::Parse,
                "why",
                self.why.len().to_string(),
                "why longer than 240",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewOrder {
    pub client_order_id: String,
    pub occ: OccSymbol,
    pub side: TicketSide,
    pub qty: u32,
    pub limit_cents: i64,
    pub tif: TimeInForce,
    pub snapshot_id: SnapshotId,
    pub policy_id: PolicyId,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_two_is_not_qty() {
        let raw = r#"{
            "snapshot_id": "snap-0001",
            "policy_id": "file-default-policy",
            "occ_symbol": "SPY260417P00500000",
            "side": "BUY",
            "qty": "two",
            "limit_cents": 245,
            "tif": "IOC",
            "why": "x"
        }"#;
        assert!(serde_json::from_str::<TicketProposal>(raw).is_err());
    }

    #[test]
    fn sell_is_not_a_v1_side() {
        let raw = r#"{
            "snapshot_id": "snap-0001",
            "policy_id": "file-default-policy",
            "occ_symbol": "SPY260417P00500000",
            "side": "SELL",
            "qty": 2,
            "limit_cents": 245,
            "tif": "IOC",
            "why": "x"
        }"#;
        assert!(serde_json::from_str::<TicketProposal>(raw).is_err());
    }

    #[test]
    fn extra_field_denied() {
        let raw = r#"{
            "snapshot_id": "snap-0001",
            "policy_id": "file-default-policy",
            "occ_symbol": "SPY260417P00500000",
            "side": "BUY",
            "qty": 2,
            "limit_cents": 245,
            "tif": "IOC",
            "why": "x",
            "greeks": 1
        }"#;
        assert!(serde_json::from_str::<TicketProposal>(raw).is_err());
    }
}
