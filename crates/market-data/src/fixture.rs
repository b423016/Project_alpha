use std::fs;
use std::path::PathBuf;

use crate::snapshot::{validate_chain, ChainSnapshot, RawChain};
use crate::DataError;

pub fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/spy_chain.json")
}

pub fn load_fixture() -> Result<ChainSnapshot, DataError> {
    let bytes = fs::read_to_string(fixture_path()).map_err(|e| DataError::Io(e.to_string()))?;
    load_fixture_bytes(bytes.as_bytes())
}

/// Drive validate/ring from fixture bytes (shipped path).
pub fn load_fixture_bytes(bytes: &[u8]) -> Result<ChainSnapshot, DataError> {
    let raw: RawChain =
        serde_json::from_slice(bytes).map_err(|e| DataError::Io(e.to_string()))?;
    validate_chain(raw, 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::{current_or_stale, SnapshotRing};

    #[test]
    fn fixture_loads_delayed_spy_chain() {
        let snap = load_fixture().expect("fixture");
        assert_eq!(snap.underlying, "SPY");
        assert!(snap.stamps.delayed);
        assert!(snap.puts().count() >= 20);
        assert!(snap.contracts.len() >= 200);
        assert_eq!(snap.stamps.source, "fixture");
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

    #[test]
    fn ring_push_current() {
        let snap = load_fixture().unwrap();
        let mut ring = SnapshotRing::new();
        let id = ring.push(snap).unwrap();
        assert_eq!(id.as_str(), "snap-fix01");
        assert_eq!(ring.current().unwrap().stamps.snapshot_id.as_str(), "snap-fix01");
    }
}
