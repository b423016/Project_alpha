use serde::{Deserialize, Serialize};

use crate::greeks::Greeks;
use crate::ids::{PolicyId, SnapshotId};
use crate::option_contract::OptionContract;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Enriched {
    pub contract: OptionContract,
    pub greeks: Greeks,
    pub utility: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Top20 {
    pub snapshot_id: SnapshotId,
    pub policy_id: PolicyId,
    pub rows: Vec<Enriched>,
}

impl Top20 {
    pub fn contains_occ(&self, occ: &str) -> bool {
        self.rows.iter().any(|r| r.contract.occ.as_str() == occ)
    }

    pub fn get(&self, occ: &str) -> Option<&Enriched> {
        self.rows.iter().find(|r| r.contract.occ.as_str() == occ)
    }
}
