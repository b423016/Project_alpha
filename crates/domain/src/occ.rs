use serde::{Deserialize, Serialize};

use crate::reject::{Reject, RejectCode};

/// OCC option symbol. v1: `[A-Z0-9]{1,6}\d{6}[CP]\d{8}`
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OccSymbol(pub String);

impl OccSymbol {
    pub fn parse(raw: impl Into<String>) -> Result<Self, Reject> {
        let raw = raw.into();
        if !is_occ(&raw) {
            return Err(Reject::new(
                RejectCode::Parse,
                "occ_symbol",
                raw,
                "not a v1 OCC symbol",
            ));
        }
        Ok(Self(raw))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn is_occ(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() < 15 || b.len() > 21 {
        return false;
    }
    let right_at = b.len() - 9;
    if right_at < 1 {
        return false;
    }
    let root = &b[..right_at - 6];
    let yymmdd = &b[right_at - 6..right_at];
    let right = b[right_at];
    let strike = &b[right_at + 1..];
    if root.is_empty() || root.len() > 6 {
        return false;
    }
    root.iter()
        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        && yymmdd.iter().all(|c| c.is_ascii_digit())
        && (right == b'C' || right == b'P')
        && strike.len() == 8
        && strike.iter().all(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_spy_put() {
        let occ = OccSymbol::parse("SPY260417P00500000").unwrap();
        assert_eq!(occ.as_str(), "SPY260417P00500000");
    }

    #[test]
    fn rejects_garbage() {
        assert!(OccSymbol::parse("SPY PUT").is_err());
        assert!(OccSymbol::parse("spy260417P00500000").is_err());
    }
}
