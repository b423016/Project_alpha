//! Operator tick: Claude proposes policy/ticket; Rust gates; Alpaca paper submits.

use neural_router_domain::{Policy, Reject, RejectCode, RiskState, TicketProposal, Top20};
use neural_router_execution::{GateLimits, gate, kernel_qty, submit_after_audit};
use neural_router_ml::argmax_utility;
use neural_router_policy::{LastGood, LiveRefs, Llm, LlmReq, validate_ticket};
use serde::Serialize;

use crate::http::AppState;

#[derive(Serialize)]
pub struct HedgeOut {
    pub ok: bool,
    pub client_order_id: Option<String>,
    pub occ: Option<String>,
    pub qty: Option<u32>,
    pub duplicate: Option<bool>,
    pub reject: Option<String>,
    pub policy_id: String,
    pub quant: &'static str,
}

fn fail(pid: String, reject: &str, quant: &'static str) -> HedgeOut {
    HedgeOut {
        ok: false,
        client_order_id: None,
        occ: None,
        qty: None,
        duplicate: None,
        reject: Some(reject.into()),
        policy_id: pid,
        quant,
    }
}

pub fn refresh_policy(state: &AppState) {
    let Some(claude) = &state.claude else {
        return;
    };
    if !state.llm_strategist {
        return;
    }
    let snap_id = state
        .snapshot
        .lock()
        .ok()
        .and_then(|g| {
            g.as_ref()
                .map(|s| s.stamps.snapshot_id.as_str().to_string())
        })
        .unwrap_or_else(|| "none".into());
    let req = LlmReq {
        prompt_version: "v1",
        user: format!(
            "SPY put overlay. Delayed fixture. snapshot_id={snap_id}. \
             Call emit_policy only. DTE 30-60, put delta -0.50 to -0.20, finite lambdas >= 0."
        ),
        cache_control: Some("ephemeral"),
        tool: "emit_policy",
    };
    let raw = match claude.complete(&req) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "strategist transport — last-good file policy");
            return;
        }
    };
    let risk = state
        .risk
        .lock()
        .ok()
        .map(|g| g.clone())
        .unwrap_or_else(|| RiskState::paper_book(10_000_000));
    let mut lg = LastGood::file_default();
    match lg.try_accept(&raw, &risk) {
        Ok(p) => {
            if let Ok(mut g) = state.policy.lock() {
                *g = p.clone();
            }
            if let (Ok(snap), Ok(mut top), Ok(mut hist)) = (
                state.snapshot.lock(),
                state.top20.lock(),
                state.metrics.lock(),
            ) {
                if let Some(s) = snap.as_ref() {
                    let (t, ms) = neural_router_ml::decide_cpu_ms(s, p);
                    hist.record(ms as u64);
                    if t.rows.is_empty() {
                        tracing::warn!("strategist funnel empty — keep prior top20");
                    } else {
                        *top = Some(t);
                    }
                }
            }
            tracing::info!(policy_id = %p.policy_id.as_str(), "strategist policy accepted");
        }
        Err(e) => tracing::warn!(
            code = e.code.as_str(),
            "strategist rejected — last-good kept"
        ),
    }
}

pub fn hedge_once(state: &AppState) -> HedgeOut {
    let policy = state
        .policy
        .lock()
        .ok()
        .map(|g| g.clone())
        .unwrap_or_else(Policy::file_default);
    let pid = policy.policy_id.as_str().to_string();
    if state.inhibit() {
        return fail(pid, "BREAKER", "none");
    }
    let Some(broker) = state.broker.clone() else {
        return fail(pid, "MISSING_CREDS", "none");
    };
    let Some(top) = state.top20.lock().ok().and_then(|g| g.clone()) else {
        return fail(pid, "STALE_DATA", "none");
    };
    let risk = state
        .risk
        .lock()
        .ok()
        .map(|g| g.clone())
        .unwrap_or_else(|| RiskState::paper_book(10_000_000));
    let under = state
        .snapshot
        .lock()
        .ok()
        .and_then(|g| g.as_ref().map(|s| s.under_price))
        .unwrap_or(500.0);
    let kqty = kernel_qty(risk.equity_cents, 810, state.risk_frac).max(1);
    let (proposal, quant) = match propose(&top, &policy, &risk, kqty, state) {
        Ok(v) => v,
        Err(r) => return fail(pid, r.code.as_str(), "fail"),
    };
    let limits = GateLimits {
        rth: !state.rth_only,
        max_slippage: state.max_slippage,
        risk_frac: state.risk_frac,
        max_daily_loss: state.max_daily_loss,
        under_price: under,
        panic_hedge: false,
    };
    match gate(proposal, &top, &policy, &risk, limits) {
        Ok(order) => {
            let mut audit = match state.audit.lock() {
                Ok(g) => g,
                Err(_) => return fail(pid, "BRAIN_DOWN", quant),
            };
            match submit_after_audit(&mut audit, broker.as_ref(), Ok(order.clone())) {
                Ok(ack) => {
                    drop(audit);
                    if let Ok(mut b) = state.blotter.lock() {
                        b.insert_submitted(&order);
                    }
                    HedgeOut {
                        ok: true,
                        client_order_id: Some(order.client_order_id),
                        occ: Some(order.occ.as_str().into()),
                        qty: Some(order.qty),
                        duplicate: Some(ack.duplicate),
                        reject: None,
                        policy_id: pid,
                        quant,
                    }
                }
                Err(r) => fail(pid, &format!("{} {}", r.code.as_str(), r.got), quant),
            }
        }
        Err(r) => {
            if let Ok(mut audit) = state.audit.lock() {
                let _ = submit_after_audit(&mut audit, broker.as_ref(), Err(r.clone()));
            }
            fail(pid, r.code.as_str(), quant)
        }
    }
}

fn propose(
    top: &Top20,
    policy: &Policy,
    risk: &RiskState,
    kqty: u32,
    state: &AppState,
) -> Result<(TicketProposal, &'static str), Reject> {
    if state.llm_quant {
        if let Some(claude) = &state.claude {
            let occs: Vec<_> = top
                .rows
                .iter()
                .take(5)
                .map(|r| r.contract.occ.as_str())
                .collect();
            let req = LlmReq {
                prompt_version: "v1",
                user: format!(
                    "Call emit_ticket only. snapshot_id={} policy_id={} side BUY tif IOC qty 1. \
                     occ must be one of: {}. limit_cents near mid.",
                    top.snapshot_id.as_str(),
                    policy.policy_id.as_str(),
                    occs.join(",")
                ),
                cache_control: Some("ephemeral"),
                tool: "emit_ticket",
            };
            if let Ok(raw) = claude.complete(&req) {
                let live = LiveRefs {
                    top20: top,
                    policy,
                    risk,
                    kernel_qty: kqty,
                    max_slippage: state.max_slippage,
                };
                if let Ok(p) = validate_ticket(&raw, live) {
                    return Ok((p, "claude"));
                }
            }
        }
    }
    argmax_utility(top, 1)
        .map(|p| (p, "argmax"))
        .ok_or_else(|| Reject::new(RejectCode::NotInTop20, "funnel", "empty", "no put"))
}
