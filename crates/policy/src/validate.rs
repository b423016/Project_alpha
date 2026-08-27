use neural_router_domain::{Policy, Reject, RejectCode, RiskState, TicketProposal, Top20};

#[derive(Clone, Copy)]
pub struct LiveRefs<'a> {
    pub top20: &'a Top20,
    pub policy: &'a Policy,
    pub risk: &'a RiskState,
    pub kernel_qty: u32,
    pub max_slippage: f64,
}

fn map_serde(err: serde_json::Error) -> Reject {
    Reject::new(
        RejectCode::Parse,
        "json",
        err.to_string(),
        "parse/unknown field/type",
    )
}

pub fn validate_policy(raw: &str, risk: &RiskState) -> Result<Policy, Reject> {
    let policy: Policy = serde_json::from_str(raw).map_err(map_serde)?;
    policy.validate_physics()?;
    let cap = (risk.equity_cents as f64 * 0.01) as i64;
    if policy.max_premium_cents > cap && cap > 0 {
        return Err(Reject::new(
            RejectCode::PremiumCap,
            "max_premium_cents",
            policy.max_premium_cents.to_string(),
            "over 1% equity",
        ));
    }
    Ok(policy)
}

pub fn validate_ticket(raw: &str, live: LiveRefs<'_>) -> Result<TicketProposal, Reject> {
    let p: TicketProposal = serde_json::from_str(raw).map_err(map_serde)?;
    p.validate_shape()?;
    if p.snapshot_id != live.top20.snapshot_id {
        return Err(Reject::new(
            RejectCode::StaleSnap,
            "snapshot_id",
            p.snapshot_id.as_str(),
            "stale snapshot",
        ));
    }
    if p.policy_id != live.policy.policy_id {
        return Err(Reject::new(
            RejectCode::StalePolicy,
            "policy_id",
            p.policy_id.as_str(),
            "stale policy",
        ));
    }
    let row = live.top20.get(p.occ_symbol.as_str()).ok_or_else(|| {
        Reject::new(
            RejectCode::NotInTop20,
            "occ_symbol",
            p.occ_symbol.as_str(),
            "not in top-20",
        )
    })?;
    if p.qty > live.kernel_qty {
        return Err(Reject::new(
            RejectCode::QtyRecompute,
            "qty",
            p.qty.to_string(),
            "qty exceeds kernel",
        ));
    }
    if let Some(mid) = row.contract.mid_cents() {
        let max_limit = ((mid as f64) * (1.0 + live.max_slippage)).round() as i64;
        if p.limit_cents > max_limit {
            return Err(Reject::new(
                RejectCode::LimitAway,
                "limit_cents",
                p.limit_cents.to_string(),
                "limit beyond slippage",
            ));
        }
    }
    Ok(p)
}

#[derive(Debug, Clone)]
pub struct LastGood {
    pub policy: Policy,
}

impl LastGood {
    pub fn file_default() -> Self {
        Self {
            policy: Policy::file_default(),
        }
    }

    pub fn try_accept(&mut self, raw: &str, risk: &RiskState) -> Result<&Policy, Reject> {
        let p = validate_policy(raw, risk)?;
        self.policy = p;
        Ok(&self.policy)
    }
}

/// Claude token budget. Exhaustion keeps last-good; no new ticket.
#[derive(Debug, Clone)]
pub struct TokenBudget {
    pub remaining: u32,
}

impl TokenBudget {
    pub fn new(remaining: u32) -> Self {
        Self { remaining }
    }

    pub fn try_spend(&mut self, n: u32) -> Result<(), Reject> {
        if self.remaining < n {
            return Err(Reject::new(
                RejectCode::BrainDown,
                "tokens",
                self.remaining.to_string(),
                "token budget exhausted — last-good only",
            ));
        }
        self.remaining -= n;
        Ok(())
    }
}

pub fn quant_with_one_retry(
    first: &str,
    retry: Option<&str>,
    live: LiveRefs<'_>,
) -> Result<TicketProposal, Reject> {
    match validate_ticket(first, LiveRefs { ..live }) {
        Ok(p) => Ok(p),
        Err(_e) => match retry {
            Some(raw) => validate_ticket(raw, live),
            None => Err(_e),
        },
    }
}

#[cfg(test)]
mod tests {
    use neural_router_domain::{
        Enriched, Greeks, OccSymbol, OptionContract, OptionRight, Policy, PolicyId, RiskState,
        SnapshotId, Top20,
    };

    use super::*;

    fn live_top() -> Top20 {
        Top20 {
            snapshot_id: SnapshotId::new("snap-0001").unwrap(),
            policy_id: PolicyId::file_default(),
            rows: vec![Enriched {
                contract: OptionContract {
                    occ: OccSymbol::parse("SPY260417P00500000").unwrap(),
                    underlying: "SPY".into(),
                    expiry: "2026-04-17".into(),
                    right: OptionRight::Put,
                    strike: 500.0,
                    dte: 30,
                    bid: 8.0,
                    ask: 8.2,
                    last: Some(8.1),
                    oi: 1500,
                    volume: 400,
                },
                greeks: Greeks {
                    delta: -0.3,
                    gamma: 0.01,
                    theta: -0.04,
                    vega: 0.1,
                    iv: 0.2,
                },
                utility: 1.0,
            }],
        }
    }

    fn refs<'a>(top: &'a Top20, policy: &'a Policy, risk: &'a RiskState) -> LiveRefs<'a> {
        LiveRefs {
            top20: top,
            policy,
            risk,
            kernel_qty: 10,
            max_slippage: 0.03,
        }
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
    fn golden_policy_ok() {
        let risk = RiskState::paper_book(1_000_000_000);
        validate_policy(&read("policy_ok.json"), &risk).unwrap();
    }

    #[test]
    fn golden_policy_extra_field() {
        let risk = RiskState::paper_book(1_000_000_000);
        let err = validate_policy(&read("policy_extra_field.json"), &risk).unwrap_err();
        assert_eq!(err.code, RejectCode::Parse);
    }

    #[test]
    fn golden_policy_nan_lambda() {
        let risk = RiskState::paper_book(1_000_000_000);
        let err = validate_policy(&read("policy_nan_lambda.json"), &risk).unwrap_err();
        assert_eq!(err.code, RejectCode::Lambda);
    }

    #[test]
    fn golden_ticket_sell() {
        let top = live_top();
        let policy = Policy::file_default();
        let risk = RiskState::paper_book(1_000_000_000);
        let err =
            validate_ticket(&read("ticket_sell.json"), refs(&top, &policy, &risk)).unwrap_err();
        assert_eq!(err.code, RejectCode::Parse);
    }

    #[test]
    fn golden_ticket_not_in_top20() {
        let top = live_top();
        let policy = Policy::file_default();
        let risk = RiskState::paper_book(1_000_000_000);
        let err = validate_ticket(
            &read("ticket_not_in_top20.json"),
            refs(&top, &policy, &risk),
        )
        .unwrap_err();
        assert_eq!(err.code, RejectCode::NotInTop20);
    }

    #[test]
    fn golden_ticket_stale_snapshot() {
        let top = live_top();
        let policy = Policy::file_default();
        let risk = RiskState::paper_book(1_000_000_000);
        let err = validate_ticket(
            &read("ticket_stale_snapshot.json"),
            refs(&top, &policy, &risk),
        )
        .unwrap_err();
        assert_eq!(err.code, RejectCode::StaleSnap);
    }

    #[test]
    fn golden_ticket_qty_too_big() {
        let top = live_top();
        let policy = Policy::file_default();
        let risk = RiskState::paper_book(1_000_000_000);
        let err = validate_ticket(&read("ticket_qty_too_big.json"), refs(&top, &policy, &risk))
            .unwrap_err();
        assert_eq!(err.code, RejectCode::QtyRecompute);
    }

    #[test]
    fn pass_k_same_bytes() {
        let top = live_top();
        let policy = Policy::file_default();
        let risk = RiskState::paper_book(1_000_000_000);
        let raw = read("ticket_not_in_top20.json");
        let a = validate_ticket(&raw, refs(&top, &policy, &risk)).unwrap_err();
        let b = validate_ticket(&raw, refs(&top, &policy, &risk)).unwrap_err();
        let c = validate_ticket(&raw, refs(&top, &policy, &risk)).unwrap_err();
        assert_eq!(a.code, b.code);
        assert_eq!(b.code, c.code);
    }

    #[test]
    fn strategist_fail_leaves_last_good() {
        let mut lg = LastGood::file_default();
        let before = lg.policy.clone();
        let risk = RiskState::paper_book(1_000_000_000);
        assert!(
            lg.try_accept(&read("policy_extra_field.json"), &risk)
                .is_err()
        );
        assert_eq!(lg.policy, before);
    }

    #[test]
    fn quant_second_fail_no_ticket() {
        let top = live_top();
        let policy = Policy::file_default();
        let risk = RiskState::paper_book(1_000_000_000);
        let err = quant_with_one_retry(
            &read("ticket_sell.json"),
            Some(&read("ticket_qty_too_big.json")),
            refs(&top, &policy, &risk),
        )
        .unwrap_err();
        assert!(matches!(
            err.code,
            RejectCode::Parse | RejectCode::QtyRecompute
        ));
    }

    #[test]
    fn missing_key_is_brain_down() {
        let err = crate::PolicyError::BrainDown("missing ANTHROPIC_API_KEY").reject();
        assert_eq!(err.code, RejectCode::BrainDown);
    }

    #[test]
    fn token_budget_exhaust_keeps_last_good() {
        let lg = LastGood::file_default();
        let before = lg.policy.clone();
        let mut budget = TokenBudget::new(1);
        budget.try_spend(1).unwrap();
        let err = budget.try_spend(1).unwrap_err();
        assert_eq!(err.code, RejectCode::BrainDown);
        assert_eq!(lg.policy, before);
    }
}
