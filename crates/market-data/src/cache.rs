use crate::DataError;

/// L1 ingest TTL cache. Tests drive this without sockets.
#[derive(Debug, Default)]
pub struct IngestCache {
    ttl_ms: i64,
    stored_at_ms: Option<i64>,
}

impl IngestCache {
    pub fn new(ttl_ms: i64) -> Self {
        Self {
            ttl_ms,
            stored_at_ms: None,
        }
    }

    pub fn store(&mut self, now_ms: i64) {
        self.stored_at_ms = Some(now_ms);
    }

    pub fn fresh(&self, now_ms: i64) -> bool {
        match self.stored_at_ms {
            Some(t) => now_ms.saturating_sub(t) <= self.ttl_ms,
            None => false,
        }
    }
}

/// Negative cache for HTTP 429. Exponential 1s → 60s.
#[derive(Debug)]
pub struct NegativeCache {
    until_ms: i64,
    backoff_ms: i64,
}

impl Default for NegativeCache {
    fn default() -> Self {
        Self {
            until_ms: 0,
            backoff_ms: 1_000,
        }
    }
}

impl NegativeCache {
    pub fn blocked(&self, now_ms: i64) -> bool {
        now_ms < self.until_ms
    }

    pub fn on_429(&mut self, now_ms: i64) -> DataError {
        self.until_ms = now_ms.saturating_add(self.backoff_ms);
        self.backoff_ms = (self.backoff_ms.saturating_mul(2)).min(60_000);
        DataError::RateLimited
    }

    pub fn clear(&mut self) {
        self.until_ms = 0;
        self.backoff_ms = 1_000;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_blocks_without_network() {
        let mut n = NegativeCache::default();
        assert!(!n.blocked(10_000));
        let err = n.on_429(10_000);
        assert_eq!(err, DataError::RateLimited);
        assert!(n.blocked(10_500));
        assert!(!n.blocked(11_001));
        n.on_429(11_001);
        assert!(n.blocked(11_001 + 1_999));
    }

    #[test]
    fn ingest_ttl() {
        let mut c = IngestCache::new(900_000);
        assert!(!c.fresh(0));
        c.store(1_000);
        assert!(c.fresh(1_000 + 899_000));
        assert!(!c.fresh(1_000 + 900_001));
    }
}
