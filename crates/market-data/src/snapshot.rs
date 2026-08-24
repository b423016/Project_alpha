use std::collections::VecDeque;

use neural_router_domain::{
    OptionContract, OptionRight, PolicyId, SnapshotId, Stamps,
};
use serde::{Deserialize, Serialize};

use crate::DataError;

const MIN_PUTS: usize = 20;
const RING_CAP: usize = 4;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawChain {
    pub underlying: String,
    pub under_price: f64,
    pub asof_unix_ms: i64,
    pub exchange_ts_ms: Option<i64>,
    pub delayed: bool,
    pub source: String,
    pub snapshot_id: String,
    pub policy_id: String,
    pub contracts: Vec<OptionContract>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainSnapshot {
    pub stamps: Stamps,
    pub underlying: String,
    pub under_price: f64,
    pub contracts: Vec<OptionContract>,
}

impl ChainSnapshot {
    pub fn puts(&self) -> impl Iterator<Item = &OptionContract> {
        self.contracts
            .iter()
            .filter(|c| c.right == OptionRight::Put)
    }

    pub fn data_age_ms(&self, now_ms: i64) -> i64 {
        self.stamps.data_age_ms(now_ms)
    }
}

pub fn validate_chain(raw: RawChain, _now_ms: i64) -> Result<ChainSnapshot, DataError> {
    if raw.underlying != "SPY" {
        return Err(DataError::InvalidSnapshot("v1 desk is SPY only"));
    }
    if raw.under_price <= 0.0 {
        return Err(DataError::InvalidSnapshot("non-positive under_price"));
    }
    let snapshot_id =
        SnapshotId::new(raw.snapshot_id).map_err(|_| DataError::InvalidSnapshot("snapshot_id"))?;
    let policy_id =
        PolicyId::new(raw.policy_id).map_err(|_| DataError::InvalidSnapshot("policy_id"))?;
    let mut contracts = Vec::new();
    for c in raw.contracts {
        if c.check_quotes().is_err() {
            continue;
        }
        contracts.push(c);
    }
    let puts = contracts
        .iter()
        .filter(|c| c.right == OptionRight::Put)
        .count();
    if puts < MIN_PUTS {
        return Err(DataError::InsufficientDepth);
    }
    Ok(ChainSnapshot {
        stamps: Stamps {
            snapshot_id,
            policy_id,
            asof_unix_ms: raw.asof_unix_ms,
            exchange_ts_ms: raw.exchange_ts_ms,
            delayed: raw.delayed,
            source: raw.source,
        },
        underlying: raw.underlying,
        under_price: raw.under_price,
        contracts,
    })
}

#[derive(Debug, Default)]
pub struct SnapshotRing {
    desk: Option<String>,
    slots: VecDeque<ChainSnapshot>,
}

impl SnapshotRing {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, s: ChainSnapshot) -> Result<SnapshotId, DataError> {
        if let Some(desk) = &self.desk {
            if desk != &s.underlying {
                return Err(DataError::InvalidSnapshot("mixed desks"));
            }
        } else {
            self.desk = Some(s.underlying.clone());
        }
        let id = s.stamps.snapshot_id.clone();
        if self.slots.len() == RING_CAP {
            self.slots.pop_front();
        }
        self.slots.push_back(s);
        Ok(id)
    }

    pub fn current(&self) -> Option<&ChainSnapshot> {
        self.slots.back()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }
}

pub fn current_or_stale<'a>(
    ring: &'a SnapshotRing,
    now_ms: i64,
    max_age_ms: i64,
) -> Result<&'a ChainSnapshot, DataError> {
    let snap = ring.current().ok_or(DataError::Stale { age_ms: i64::MAX })?;
    let age = snap.data_age_ms(now_ms);
    if age > max_age_ms {
        return Err(DataError::Stale { age_ms: age });
    }
    Ok(snap)
}
