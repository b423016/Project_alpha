//! Names desk HTTP. Suggest path never calls Broker::submit.

use std::collections::HashSet;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::Response;
use neural_router_ml::{NameFeatures, features_from_closes, synth_bars};
use neural_router_policy::{
    Llm, LlmReq, hold_no_signal, hold_rejected, hold_stale, validate_suggest,
};
use serde_json::json;

use crate::http::AppState;

fn json_ok(v: serde_json::Value) -> Result<Response, StatusCode> {
    let bytes = serde_json::to_vec(&v).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CACHE_CONTROL, "no-store")
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(bytes))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

pub async fn universe(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    crate::http::auth(&state, &headers)?;
    let rows = state
        .universe
        .lock()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    json_ok(json!({ "rows": &*rows, "n": rows.len() }))
}

fn in_universe(state: &AppState, sym: &str) -> Result<bool, StatusCode> {
    let u = state
        .universe
        .lock()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(u.iter().any(|r| r.symbol.eq_ignore_ascii_case(sym)))
}

fn bars_for(sym: &str) -> Vec<neural_router_ml::NameBar> {
    let mut b = synth_bars(sym, 60);
    if let Some(last) = b.last().map(|x| x.t) {
        let d = neural_router_execution::now_ms() - last;
        for x in &mut b {
            x.t += d;
        }
    }
    b
}

pub async fn name_bars(
    State(state): State<AppState>,
    Path(sym): Path<String>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    crate::http::auth(&state, &headers)?;
    let sym = sym.to_uppercase();
    if !in_universe(&state, &sym)? {
        return Ok((StatusCode::NOT_FOUND, "NOT_IN_UNIVERSE").into_response());
    }
    json_ok(json!({ "symbol": sym, "bars": bars_for(&sym) }))
}

pub async fn name_features(
    State(state): State<AppState>,
    Path(sym): Path<String>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    crate::http::auth(&state, &headers)?;
    let sym = sym.to_uppercase();
    if !in_universe(&state, &sym)? {
        return Ok((StatusCode::NOT_FOUND, "NOT_IN_UNIVERSE").into_response());
    }
    let bars = bars_for(&sym);
    let closes: Vec<f64> = bars.iter().map(|b| b.c).collect();
    let asof = bars.last().map(|b| b.t).unwrap_or(0);
    let now = neural_router_execution::now_ms();
    match features_from_closes(&sym, &closes, asof, now) {
        Ok(f) => json_ok(serde_json::to_value(f).unwrap_or(json!({}))),
        Err(_) => Ok((StatusCode::UNPROCESSABLE_ENTITY, "STALE_DATA").into_response()),
    }
}

pub async fn name_surface(
    State(state): State<AppState>,
    Path(sym): Path<String>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    crate::http::auth(&state, &headers)?;
    let sym = sym.to_uppercase();
    if !in_universe(&state, &sym)? {
        return Ok((StatusCode::NOT_FOUND, "NOT_IN_UNIVERSE").into_response());
    }
    let bars = bars_for(&sym);
    let closes: Vec<f64> = bars.iter().map(|b| b.c).collect();
    let grid = neural_router_ml::return_heatmap(&closes, 20);
    let feat = features_from_closes(
        &sym,
        &closes,
        bars.last().map(|b| b.t).unwrap_or(0),
        neural_router_execution::now_ms(),
    )
    .ok();
    json_ok(json!({
        "symbol": sym,
        "kind": "return_abs_heatmap",
        "grid": grid,
        "features": feat,
        "note": "quoted IV grid when has_options chain is loaded; this heatmap is from bars",
    }))
}

/// Rust hands over the already-computed feature cells only — Claude never
/// sees bars/quotes/broker state and never recomputes them (AGENTS.md: LLM
/// never on the tick path, never computes size).
fn build_suggest_prompt(sym: &str, feat: &NameFeatures) -> String {
    let mut user = format!(
        "Equity desk lean for {sym}. Call emit_suggest only. name must equal \"{sym}\" exactly. \
         side must be LONG, SHORT, or HOLD. horizon_bars must be 5, 20, or 60. conf finite in [0,1]. \
         why must be one short sentence, at most 400 characters, in plain English for a trader who \
         has not seen these numbers — describe the setup in words (e.g. \"pushed up hard over the \
         last 20 bars and stretched near its recent high\"), never cite the raw variable names or \
         bare values below (ret_20, range_pos=0.93, etc. mean nothing to the reader). \
         Never include qty or limit — this is a display-only lean, Rust ignores/rejects any sizing \
         field and never sends an order from it. Features already computed by Rust, do not recompute:\n"
    );
    for c in &feat.cells {
        user.push_str(&format!(
            "- {} = {:.4} {} — {}\n",
            c.name, c.value, c.units, c.meaning
        ));
    }
    user
}

fn retry_prompt(base: &str, reject: &neural_router_domain::Reject) -> String {
    format!(
        "{base}\nYour last emit_suggest call was rejected: code={} field={} got={} — {}. \
         Fix that specific field and call emit_suggest again with valid fields only.",
        reject.code.as_str(),
        reject.field,
        reject.got,
        reject.message
    )
}

/// V0–V6 for the names desk: one Claude call, one retry with the structured
/// [`Reject`] fed back, then fail closed to HOLD — same shape as the
/// overlay's policy/ticket path, never a `Broker::submit` either way.
fn suggest_via_claude(
    claude: &dyn Llm,
    sym: &str,
    feat: &NameFeatures,
    universe: &HashSet<String>,
) -> neural_router_policy::NameSuggest {
    let prompt = build_suggest_prompt(sym, feat);
    let req = LlmReq {
        prompt_version: "v1",
        user: prompt.clone(),
        cache_control: None,
        tool: "emit_suggest",
    };
    let first = match claude.complete(&req) {
        Ok(raw) => raw,
        Err(e) => {
            tracing::warn!(sym, error = %e, "names desk: claude transport — HOLD");
            return hold_rejected(sym, &e.reject());
        }
    };
    let reject = match validate_suggest(&first, sym, universe) {
        Ok(s) => return s,
        Err(e) => e,
    };
    tracing::info!(
        sym,
        code = reject.code.as_str(),
        field = reject.field,
        "names desk: retry"
    );
    let retry = claude.complete(&LlmReq {
        prompt_version: "v1",
        user: retry_prompt(&prompt, &reject),
        cache_control: None,
        tool: "emit_suggest",
    });
    match retry {
        Ok(raw) => match validate_suggest(&raw, sym, universe) {
            Ok(s) => s,
            Err(e2) => hold_rejected(sym, &e2),
        },
        Err(_) => hold_rejected(sym, &reject),
    }
}

pub async fn name_suggest(
    State(state): State<AppState>,
    Path(sym): Path<String>,
    headers: HeaderMap,
    body: String,
) -> Result<Response, StatusCode> {
    crate::http::auth(&state, &headers)?;
    let sym = sym.to_uppercase();
    if !in_universe(&state, &sym)? {
        return Ok((StatusCode::NOT_FOUND, "NOT_IN_UNIVERSE").into_response());
    }
    let set: HashSet<String> = state
        .universe
        .lock()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .iter()
        .map(|r| r.symbol.clone())
        .collect();
    let bars = bars_for(&sym);
    let closes: Vec<f64> = bars.iter().map(|b| b.c).collect();
    let feat = features_from_closes(
        &sym,
        &closes,
        bars.last().map(|b| b.t).unwrap_or(0),
        neural_router_execution::now_ms(),
    );
    if body.trim().is_empty() {
        // Fresh (non-stale) features + llm_names on + a configured client is
        // the only path that spends a real Claude call. Everything else is
        // the deterministic default — no network, same as before.
        let fresh = match &feat {
            Ok(f) if !f.stale => Some(f.clone()),
            _ => None,
        };
        if let (Some(f), true, Some(claude)) = (fresh, state.llm_names, state.claude.clone()) {
            let sym2 = sym.clone();
            let hold = tokio::task::spawn_blocking(move || {
                suggest_via_claude(claude.as_ref(), &sym2, &f, &set)
            })
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            return json_ok(serde_json::to_value(hold).unwrap_or(json!({})));
        }
        // Distinguish the two HOLD reasons — a stale/missing feed is a
        // fail-closed data problem; an empty POST with fresh features is
        // just "no LLM wired in yet" and should say so, not lie about staleness.
        let stale = feat.as_ref().map(|f| f.stale).unwrap_or(true);
        let hold = if stale {
            hold_stale(&sym)
        } else {
            hold_no_signal(&sym)
        };
        return json_ok(serde_json::to_value(hold).unwrap_or(json!({})));
    }
    match validate_suggest(&body, &sym, &set) {
        Ok(s) => json_ok(serde_json::to_value(s).unwrap_or(json!({}))),
        Err(r) => {
            let v = json!({
                "ok": false,
                "code": r.code.as_str(),
                "field": r.field,
                "got": r.got,
            });
            let bytes = serde_json::to_vec(&v).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            Response::builder()
                .status(StatusCode::UNPROCESSABLE_ENTITY)
                .header(header::CACHE_CONTROL, "no-store")
                .header(header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from(bytes))
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

use axum::response::IntoResponse;

#[cfg(test)]
mod claude_tests {
    use std::cell::RefCell;
    use std::collections::HashSet;

    use neural_router_policy::{MockLlm, PolicyError, SuggestSide};

    use super::*;

    struct SeqLlm(RefCell<Vec<Result<String, PolicyError>>>);

    impl Llm for SeqLlm {
        fn complete(&self, _req: &LlmReq) -> Result<String, PolicyError> {
            self.0.borrow_mut().remove(0)
        }
    }

    fn feat() -> NameFeatures {
        let bars = synth_bars("AAPL", 30);
        let closes: Vec<f64> = bars.iter().map(|b| b.c).collect();
        features_from_closes("AAPL", &closes, 0, 0).unwrap()
    }

    fn uni() -> HashSet<String> {
        ["AAPL".to_string()].into_iter().collect()
    }

    #[test]
    fn claude_ok_first_try() {
        let llm = MockLlm {
            payload: r#"{"name":"AAPL","side":"LONG","horizon_bars":20,"conf":0.5,"why":"x"}"#
                .into(),
        };
        let s = suggest_via_claude(&llm, "AAPL", &feat(), &uni());
        assert_eq!(s.side, SuggestSide::Long);
    }

    #[test]
    fn claude_transport_failure_holds() {
        let llm = MockLlm { payload: "".into() }; // empty payload -> BrainDown, no retry spent
        let s = suggest_via_claude(&llm, "AAPL", &feat(), &uni());
        assert_eq!(s.side, SuggestSide::Hold);
    }

    #[test]
    fn claude_second_fail_also_holds_no_submit() {
        // Same broken payload both times — the retry is genuinely spent and
        // still fails closed, never a submit.
        let llm = SeqLlm(RefCell::new(vec![
            Ok("not json".into()),
            Ok("not json".into()),
        ]));
        let s = suggest_via_claude(&llm, "AAPL", &feat(), &uni());
        assert_eq!(s.side, SuggestSide::Hold);
    }

    #[test]
    fn claude_retry_recovers() {
        let llm =
            SeqLlm(RefCell::new(vec![
            // rejected: extra `qty` — Rust ignores/rejects any sizing field
            Ok(r#"{"name":"AAPL","side":"LONG","horizon_bars":20,"conf":0.5,"why":"x","qty":1}"#
                .into()),
            Ok(r#"{"name":"AAPL","side":"SHORT","horizon_bars":5,"conf":0.3,"why":"fixed"}"#
                .into()),
        ]));
        let s = suggest_via_claude(&llm, "AAPL", &feat(), &uni());
        assert_eq!(s.side, SuggestSide::Short);
    }
}
