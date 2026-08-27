use std::collections::VecDeque;

use std::hash::{Hash, Hasher};

use neural_router_domain::{OccSymbol, OptionContract, OptionRight, PolicyId, SnapshotId, Stamps};
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

    pub fn expiry_set_hash(&self) -> u64 {
        let mut expiries: Vec<&str> = self.contracts.iter().map(|c| c.expiry.as_str()).collect();
        expiries.sort_unstable();
        expiries.dedup();
        let mut h = std::collections::hash_map::DefaultHasher::new();
        expiries.hash(&mut h);
        h.finish()
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
        // OCC serde is a string newtype; re-parse so garbage never enters the ring.
        if OccSymbol::parse(c.occ.as_str()).is_err() {
            continue;
        }
        if c.underlying != "SPY" || !c.occ.as_str().starts_with("SPY") {
            continue;
        }
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

    pub fn len(&self) -> usize {
        self.slots.len()
    }
}

pub fn current_or_stale(
    ring: &SnapshotRing,
    now_ms: i64,
    max_age_ms: i64,
) -> Result<&ChainSnapshot, DataError> {
    let snap = ring
        .current()
        .ok_or(DataError::Stale { age_ms: i64::MAX })?;
    let age = snap.data_age_ms(now_ms);
    if age > max_age_ms {
        return Err(DataError::Stale { age_ms: age });
    }
    Ok(snap)
}

#[cfg(test)]
mod tests {
    use neural_router_domain::{OccSymbol, OptionContract, OptionRight};

    use super::*;

    fn put(i: u32) -> OptionContract {
        let strike = 400 + i * 5;
        OptionContract {
            occ: OccSymbol::parse(format!("SPY260417P{strike:08}")).unwrap(),
            underlying: "SPY".into(),
            expiry: "2026-04-17".into(),
            right: OptionRight::Put,
            strike: f64::from(strike),
            dte: 30,
            bid: 1.0,
            ask: 1.1,
            last: Some(1.05),
            oi: 100,
            volume: 10,
        }
    }

    fn raw(n_puts: u32) -> RawChain {
        RawChain {
            underlying: "SPY".into(),
            under_price: 500.0,
            asof_unix_ms: 1_000,
            exchange_ts_ms: Some(900),
            delayed: true,
            source: "fixture".into(),
            snapshot_id: "snap-fix01".into(),
            policy_id: "file-default-policy".into(),
            contracts: (0..n_puts).map(put).collect(),
        }
    }

    #[test]
    fn rejects_non_spy_desk() {
        let mut r = raw(20);
        r.underlying = "QQQ".into();
        assert!(matches!(
            validate_chain(r, 0),
            Err(DataError::InvalidSnapshot("v1 desk is SPY only"))
        ));
    }

    #[test]
    fn insufficient_puts_is_depth_error() {
        assert_eq!(
            validate_chain(raw(19), 0).unwrap_err(),
            DataError::InsufficientDepth
        );
    }

    #[test]
    fn drops_crossed_and_garbage_occ() {
        let mut r = raw(22);
        r.contracts[0].bid = 2.0;
        r.contracts[0].ask = 1.0;
        r.contracts[1].occ = OccSymbol("NOT-AN-OCC".into());
        let snap = validate_chain(r, 0).unwrap();
        assert_eq!(snap.puts().count(), 20);
        assert_eq!(snap.data_age_ms(1_100), 200);
    }

    #[test]
    fn ring_refuses_mixed_desks_and_caps_at_four() {
        let snap = validate_chain(raw(20), 0).unwrap();
        let mut ring = SnapshotRing::new();
        for i in 0..5 {
            let mut s = snap.clone();
            s.stamps.snapshot_id = SnapshotId::new(format!("snap-cap{i:02}")).unwrap();
            ring.push(s).unwrap();
        }
        assert_eq!(ring.len(), 4);
        assert_eq!(
            ring.current().unwrap().stamps.snapshot_id.as_str(),
            "snap-cap04"
        );

        let mut other = snap;
        other.underlying = "QQQ".into();
        assert!(matches!(
            ring.push(other),
            Err(DataError::InvalidSnapshot("mixed desks"))
        ));
    }
}
