use serde::{Deserialize, Serialize};

use crate::ids::{PolicyId, SnapshotId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stamps {
    pub snapshot_id: SnapshotId,
    pub policy_id: PolicyId,
    pub asof_unix_ms: i64,
    pub exchange_ts_ms: Option<i64>,
    pub delayed: bool,
    pub source: String,
}

impl Stamps {
    pub fn data_age_ms(&self, now_ms: i64) -> i64 {
        let origin = self.exchange_ts_ms.unwrap_or(self.asof_unix_ms);
        now_ms.saturating_sub(origin)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn age_prefers_exchange_ts() {
        let stamps = Stamps {
            snapshot_id: SnapshotId::new("snap-0001").unwrap(),
            policy_id: PolicyId::file_default(),
            asof_unix_ms: 1_000,
            exchange_ts_ms: Some(900),
            delayed: true,
            source: "fixture".into(),
        };
        assert_eq!(stamps.data_age_ms(1_100), 200);
    }
}
