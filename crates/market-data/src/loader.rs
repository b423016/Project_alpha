use crate::DataError;
use crate::snapshot::{ChainSnapshot, RawChain, validate_chain};

/// Options chain source. Vendor HTTP stays behind this trait.
pub trait ChainSource {
    fn fetch(&mut self) -> Result<RawChain, DataError>;
}

/// Placeholder vendor. Fails closed until a real adapter is wired.
pub struct PlaceholderChainSource;

impl ChainSource for PlaceholderChainSource {
    fn fetch(&mut self) -> Result<RawChain, DataError> {
        Err(DataError::NotImplemented {
            feature: "polygon_or_yfinance_chain",
        })
    }
}

/// Hermetic source: testdata only, no sockets.
pub struct FixtureChainSource;

impl ChainSource for FixtureChainSource {
    fn fetch(&mut self) -> Result<RawChain, DataError> {
        crate::fixture::load_raw_fixture()
    }
}

pub fn ingest(src: &mut impl ChainSource, now_ms: i64) -> Result<ChainSnapshot, DataError> {
    validate_chain(src.fetch()?, now_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_fails_closed_without_network() {
        let err = PlaceholderChainSource.fetch().unwrap_err();
        assert!(matches!(err, DataError::NotImplemented { .. }));
    }

    #[test]
    fn ingest_fixture_is_spy_delayed() {
        let snap = ingest(&mut FixtureChainSource, 0).unwrap();
        assert_eq!(snap.underlying, "SPY");
        assert!(snap.stamps.delayed);
    }
}
