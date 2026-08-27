use std::fs;
use std::path::PathBuf;

use crate::DataError;
use crate::snapshot::{ChainSnapshot, RawChain, validate_chain};

pub fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/spy_chain.json")
}

pub fn load_raw_fixture() -> Result<RawChain, DataError> {
    let bytes = fs::read(fixture_path()).map_err(|e| DataError::Io(e.to_string()))?;
    serde_json::from_slice(&bytes).map_err(|e| DataError::Io(e.to_string()))
}

pub fn load_fixture() -> Result<ChainSnapshot, DataError> {
    load_fixture_bytes(&fs::read(fixture_path()).map_err(|e| DataError::Io(e.to_string()))?)
}

/// Drive validate/ring from fixture bytes (shipped path).
pub fn load_fixture_bytes(bytes: &[u8]) -> Result<ChainSnapshot, DataError> {
    let raw: RawChain = serde_json::from_slice(bytes).map_err(|e| DataError::Io(e.to_string()))?;
    validate_chain(raw, 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::{ChainSource, FixtureChainSource};
    use crate::snapshot::{SnapshotRing, current_or_stale, validate_chain};

    #[test]
    fn fixture_source_loads_delayed_spy_into_ring() {
        let raw = FixtureChainSource.fetch().expect("fixture fetch");
        assert!(raw.delayed);
        assert_eq!(raw.source, "fixture");
        let snap = validate_chain(raw, 0).expect("validate");
        assert_eq!(snap.underlying, "SPY");
        assert!(snap.stamps.delayed);
        assert!(snap.contracts.len() >= 200);
        assert!(snap.puts().count() >= 20);
        assert!(
            snap.contracts
                .iter()
                .all(|c| c.bid > 0.0 && c.ask >= c.bid && c.oi > 0)
        );
        let mut ring = SnapshotRing::new();
        let id = ring.push(snap).unwrap();
        assert_eq!(id.as_str(), "snap-fix01");
        let current = ring.current().unwrap();
        assert_eq!(current.stamps.snapshot_id.as_str(), "snap-fix01");
        assert!(current.data_age_ms(current.stamps.asof_unix_ms) == 0);
    }

    #[test]
    fn empty_ring_is_stale_data() {
        let ring = SnapshotRing::new();
        let err = current_or_stale(&ring, 1, 900_000).unwrap_err();
        let reject = err.reject();
        assert_eq!(reject.code, neural_router_domain::RejectCode::StaleData);
    }

    #[test]
    fn stale_age_yields_no_ticket_signal() {
        let snap = load_fixture().unwrap();
        let mut ring = SnapshotRing::new();
        ring.push(snap).unwrap();
        let now = 1_773_792_000_000 + 1_000_000;
        let err = current_or_stale(&ring, now, 900_000).unwrap_err();
        match err {
            DataError::Stale { age_ms } => assert!(age_ms > 900_000),
            other => panic!("{other:?}"),
        }
    }
}
