use serde::{Deserialize, Serialize};

use crate::reject::{Reject, RejectCode};

const ID_MIN: usize = 8;
const ID_MAX: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SnapshotId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PolicyId(pub String);

impl SnapshotId {
    pub fn new(raw: impl Into<String>) -> Result<Self, Reject> {
        parse_id("snapshot_id", raw.into()).map(Self)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl PolicyId {
    pub fn new(raw: impl Into<String>) -> Result<Self, Reject> {
        parse_id("policy_id", raw.into()).map(Self)
    }

    /// File policy when LLM is off (LLD: default file policy id).
    pub fn file_default() -> Self {
        Self("file-default-policy".into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn parse_id(field: &'static str, raw: String) -> Result<String, Reject> {
    let n = raw.len();
    if n < ID_MIN || n > ID_MAX {
        return Err(Reject::new(
            RejectCode::Parse,
            field,
            raw,
            "id length must be 8..=64",
        ));
    }
    Ok(raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_short_id() {
        let err = SnapshotId::new("short").unwrap_err();
        assert_eq!(err.code, RejectCode::Parse);
        assert_eq!(err.field, "snapshot_id");
    }

    #[test]
    fn accepts_legal_id() {
        assert!(SnapshotId::new("snap-0001").is_ok());
        assert_eq!(PolicyId::file_default().as_str(), "file-default-policy");
    }
}
