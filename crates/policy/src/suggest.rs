//! Bounded LONG/SHORT/HOLD for the names desk. No qty. No submit.

use std::collections::HashSet;

use neural_router_domain::{Reject, RejectCode};
use serde::{Deserialize, Serialize};

use crate::extract::extract_tool_input;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum SuggestSide {
    Long,
    Short,
    Hold,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NameSuggest {
    pub name: String,
    pub side: SuggestSide,
    pub horizon_bars: u32,
    pub conf: f64,
    pub why: String,
}

const HORIZONS: [u32; 3] = [5, 20, 60];
/// Room for a real one/two-sentence explanation citing a few feature cells
/// (name=value pairs add up fast) without going unbounded.
const WHY_MAX_CHARS: usize = 400;

pub fn validate_suggest(
    raw: &str,
    focused: &str,
    universe: &HashSet<String>,
) -> Result<NameSuggest, Reject> {
    let body = extract_tool_input(raw, "emit_suggest")?;
    let s: NameSuggest = serde_json::from_str(&body).map_err(|e| {
        Reject::new(
            RejectCode::Parse,
            "json",
            e.to_string(),
            "parse/unknown field/type",
        )
    })?;
    if s.name != focused {
        return Err(Reject::new(
            RejectCode::NotInUniverse,
            "name",
            s.name.clone(),
            "must match focused symbol",
        ));
    }
    if !universe.contains(&s.name) {
        return Err(Reject::new(
            RejectCode::NotInUniverse,
            "name",
            s.name.clone(),
            "not in Alpaca tradable universe",
        ));
    }
    if !s.conf.is_finite() || !(0.0..=1.0).contains(&s.conf) {
        return Err(Reject::new(
            RejectCode::Lambda,
            "conf",
            s.conf.to_string(),
            "conf must be finite in [0,1]",
        ));
    }
    if !HORIZONS.contains(&s.horizon_bars) {
        return Err(Reject::new(
            RejectCode::RangeDte,
            "horizon_bars",
            s.horizon_bars.to_string(),
            "horizon must be 5, 20, or 60",
        ));
    }
    if s.why.len() > WHY_MAX_CHARS {
        return Err(Reject::new(
            RejectCode::Parse,
            "why",
            s.why.len().to_string(),
            "why must be a short sentence, at most 400 chars",
        ));
    }
    Ok(s)
}

/// Fail-closed HOLD for genuinely stale/missing features — not the same
/// reason as [`hold_no_signal`]; don't collapse the two into one string.
pub fn hold_stale(focused: &str) -> NameSuggest {
    NameSuggest {
        name: focused.into(),
        side: SuggestSide::Hold,
        horizon_bars: 20,
        conf: 0.0,
        why: "stale or missing features — HOLD".into(),
    }
}

/// Default HOLD when no proposal was made — this desk has no LLM call
/// wired in yet (`llm_names=false`), so an empty POST is the normal
/// no-signal case, not a data problem. Keep the wording honest.
pub fn hold_no_signal(focused: &str) -> NameSuggest {
    NameSuggest {
        name: focused.into(),
        side: SuggestSide::Hold,
        horizon_bars: 20,
        conf: 0.0,
        why: "no AI lean requested (llm_names is off by default) — HOLD".into(),
    }
}

/// Second Claude attempt also failed bounds — HOLD, never a submit, and say
/// which check failed instead of a generic excuse.
pub fn hold_rejected(focused: &str, reject: &Reject) -> NameSuggest {
    NameSuggest {
        name: focused.into(),
        side: SuggestSide::Hold,
        horizon_bars: 20,
        conf: 0.0,
        why: format!(
            "AI proposal rejected twice ({} on {}) — HOLD",
            reject.code.as_str(),
            reject.field
        ),
    }
}

/// V6 shape: one retry with the structured [`Reject`] fed back, same as the
/// overlay's `quant_with_one_retry`. Second failure fails closed — caller
/// turns the `Err` into [`hold_rejected`], never a submit either way.
pub fn suggest_with_one_retry(
    first: &str,
    retry: Option<&str>,
    focused: &str,
    universe: &HashSet<String>,
) -> Result<NameSuggest, Reject> {
    match validate_suggest(first, focused, universe) {
        Ok(s) => Ok(s),
        Err(e) => match retry {
            Some(raw) => validate_suggest(raw, focused, universe),
            None => Err(e),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uni() -> HashSet<String> {
        ["AAPL", "TSLA"].into_iter().map(String::from).collect()
    }

    fn vec_path(name: &str) -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/vectors")
            .join(name)
    }

    fn read(name: &str) -> String {
        std::fs::read_to_string(vec_path(name)).unwrap()
    }

    #[test]
    fn golden_suggest_ok() {
        let s = validate_suggest(&read("suggest_ok_long.json"), "AAPL", &uni()).unwrap();
        assert_eq!(s.side, SuggestSide::Long);
    }

    #[test]
    fn golden_suggest_extra_qty_is_parse() {
        let err = validate_suggest(&read("suggest_extra_qty.json"), "AAPL", &uni()).unwrap_err();
        assert_eq!(err.code, RejectCode::Parse);
    }

    #[test]
    fn golden_suggest_hallucinated_ticker() {
        let err = validate_suggest(&read("suggest_hallucinated_ticker.json"), "FAKEX", &uni())
            .unwrap_err();
        assert_eq!(err.code, RejectCode::NotInUniverse);
    }

    #[test]
    fn golden_suggest_bad_horizon() {
        let err = validate_suggest(&read("suggest_bad_horizon.json"), "AAPL", &uni()).unwrap_err();
        assert_eq!(err.code, RejectCode::RangeDte);
    }

    #[test]
    fn retry_recovers_from_first_reject() {
        let s = suggest_with_one_retry(
            &read("suggest_extra_qty.json"),
            Some(&read("suggest_ok_long.json")),
            "AAPL",
            &uni(),
        )
        .unwrap();
        assert_eq!(s.side, SuggestSide::Long);
    }

    #[test]
    fn second_fail_holds_no_submit() {
        let err = suggest_with_one_retry(
            &read("suggest_extra_qty.json"),
            Some(&read("suggest_bad_horizon.json")),
            "AAPL",
            &uni(),
        )
        .unwrap_err();
        assert!(matches!(err.code, RejectCode::Parse | RejectCode::RangeDte));
        let hold = hold_rejected("AAPL", &err);
        assert_eq!(hold.side, SuggestSide::Hold);
    }

    #[test]
    fn no_retry_offered_fails_on_first() {
        let err = suggest_with_one_retry(&read("suggest_extra_qty.json"), None, "AAPL", &uni())
            .unwrap_err();
        assert_eq!(err.code, RejectCode::Parse);
    }

    #[test]
    fn ok_hold() {
        let raw = r#"{"name":"AAPL","side":"HOLD","horizon_bars":20,"conf":0.4,"why":"mid"}"#;
        let s = validate_suggest(raw, "AAPL", &uni()).unwrap();
        assert_eq!(s.side, SuggestSide::Hold);
    }

    #[test]
    fn extra_qty_is_parse() {
        let raw = r#"{"name":"AAPL","side":"LONG","horizon_bars":20,"conf":0.4,"why":"x","qty":1}"#;
        let err = validate_suggest(raw, "AAPL", &uni()).unwrap_err();
        assert_eq!(err.code, RejectCode::Parse);
    }

    #[test]
    fn hallucinated_ticker_rejected() {
        let raw = r#"{"name":"FAKE","side":"LONG","horizon_bars":20,"conf":0.9,"why":"x"}"#;
        let err = validate_suggest(raw, "AAPL", &uni()).unwrap_err();
        assert_eq!(err.code, RejectCode::NotInUniverse);
    }

    #[test]
    fn focused_mismatch() {
        let raw = r#"{"name":"TSLA","side":"SHORT","horizon_bars":5,"conf":0.2,"why":"x"}"#;
        let err = validate_suggest(raw, "AAPL", &uni()).unwrap_err();
        assert_eq!(err.code, RejectCode::NotInUniverse);
    }
}
