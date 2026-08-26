use neural_router_domain::{
    NewOrder, Policy, Reject, RejectCode, RiskState, TicketProposal, Top20,
};

pub fn client_order_id(proposal: &TicketProposal) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(proposal.snapshot_id.as_str().as_bytes());
    hasher.update(proposal.policy_id.as_str().as_bytes());
    hasher.update(proposal.occ_symbol.as_str().as_bytes());
    hasher.update(&proposal.qty.to_le_bytes());
    hasher.update(b"BUY");
    hasher.finalize().to_hex().to_string()
}

pub fn kernel_qty(equity_cents: i64, mid_cents: i64, risk_frac: f64) -> u32 {
    if mid_cents <= 0 || equity_cents <= 0 {
        return 0;
    }
    let cap = (equity_cents as f64 * risk_frac) as i64;
    (cap / mid_cents).clamp(0, 1000) as u32
}

pub struct GateLimits {
    pub rth: bool,
    pub max_slippage: f64,
    pub risk_frac: f64,
    pub max_daily_loss: f64,
}

pub fn gate(
    proposal: TicketProposal,
    top20: &Top20,
    policy: &Policy,
    risk: &RiskState,
    limits: GateLimits,
) -> Result<NewOrder, Reject> {
    if !limits.rth {
        return Err(Reject::new(RejectCode::Rth, "rth", "closed", "outside RTH"));
    }
    if risk.breaker {
        return Err(Reject::new(
            RejectCode::Breaker,
            "breaker",
            risk.rejection_count.to_string(),
            "circuit breaker",
        ));
    }
    if risk.equity_cents <= 0 {
        return Err(Reject::new(
            RejectCode::PremiumCap,
            "equity_cents",
            risk.equity_cents.to_string(),
            "non-positive equity",
        ));
    }
    let loss_lim = (risk.equity_cents as f64 * limits.max_daily_loss) as i64;
    if risk.daily_pnl_cents <= -loss_lim {
        return Err(Reject::new(
            RejectCode::DailyLoss,
            "daily_pnl_cents",
            risk.daily_pnl_cents.to_string(),
            "daily loss limit",
        ));
    }
    if proposal.snapshot_id != top20.snapshot_id {
        return Err(Reject::new(
            RejectCode::StaleSnap,
            "snapshot_id",
            proposal.snapshot_id.as_str(),
            "snapshot mismatch",
        ));
    }
    if proposal.policy_id != policy.policy_id || proposal.policy_id != top20.policy_id {
        return Err(Reject::new(
            RejectCode::StalePolicy,
            "policy_id",
            proposal.policy_id.as_str(),
            "policy mismatch",
        ));
    }
    let row = top20.get(proposal.occ_symbol.as_str()).ok_or_else(|| {
        Reject::new(
            RejectCode::NotInTop20,
            "occ_symbol",
            proposal.occ_symbol.as_str(),
            "not in live top-20",
        )
    })?;
    let mid_cents = row.contract.mid_cents().unwrap_or(proposal.limit_cents);
    let kqty = kernel_qty(risk.equity_cents, mid_cents, limits.risk_frac);
    if proposal.qty > kqty {
        return Err(Reject::new(
            RejectCode::QtyRecompute,
            "qty",
            proposal.qty.to_string(),
            "qty exceeds 1% kernel size",
        ));
    }
    let premium = i64::from(proposal.qty) * proposal.limit_cents;
    let cap = (risk.equity_cents as f64 * limits.risk_frac) as i64;
    let cap = cap.min(policy.max_premium_cents);
    if premium > cap {
        return Err(Reject::new(
            RejectCode::PremiumCap,
            "premium",
            premium.to_string(),
            "premium over 1% / policy cap",
        ));
    }
    let max_limit = ((mid_cents as f64) * (1.0 + limits.max_slippage)).round() as i64;
    if proposal.limit_cents > max_limit {
        return Err(Reject::new(
            RejectCode::LimitAway,
            "limit_cents",
            proposal.limit_cents.to_string(),
            "limit beyond slippage",
        ));
    }
    proposal.validate_shape()?;
    Ok(NewOrder {
        client_order_id: client_order_id(&proposal),
        occ: proposal.occ_symbol,
        side: proposal.side,
        qty: proposal.qty,
        limit_cents: proposal.limit_cents,
        tif: proposal.tif,
        snapshot_id: proposal.snapshot_id,
        policy_id: proposal.policy_id,
    })
}

#[cfg(test)]
mod tests {
    use neural_router_domain::{
        Enriched, Greeks, OccSymbol, OptionContract, OptionRight, Policy, PolicyId, RiskState,
        SnapshotId, TicketProposal, TicketSide, TimeInForce, Top20,
    };

    use super::*;

    fn row() -> Enriched {
        Enriched {
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
                theta: -0.05,
                vega: 0.1,
                iv: 0.2,
            },
            utility: 1.0,
        }
    }

    fn top() -> Top20 {
        Top20 {
            snapshot_id: SnapshotId::new("snap-0001").unwrap(),
            policy_id: PolicyId::file_default(),
            rows: vec![row()],
        }
    }

    fn limits(rth: bool) -> GateLimits {
        GateLimits {
            rth,
            max_slippage: 0.03,
            risk_frac: 0.01,
            max_daily_loss: 0.05,
        }
    }

    fn proposal(qty: u32) -> TicketProposal {
        TicketProposal {
            snapshot_id: SnapshotId::new("snap-0001").unwrap(),
            policy_id: PolicyId::file_default(),
            occ_symbol: OccSymbol::parse("SPY260417P00500000").unwrap(),
            side: TicketSide::Buy,
            qty,
            limit_cents: 810,
            tif: TimeInForce::Ioc,
            why: "test".into(),
        }
    }

    #[test]
    fn accept_small_qty() {
        let order = gate(
            proposal(1),
            &top(),
            &Policy::file_default(),
            &RiskState::paper_book(100_000_000),
            limits(true),
        )
        .unwrap();
        assert_eq!(order.qty, 1);
        assert_eq!(order.client_order_id, client_order_id(&proposal(1)));
    }

    #[test]
    fn client_order_id_is_stable() {
        assert_eq!(client_order_id(&proposal(2)), client_order_id(&proposal(2)));
        assert_ne!(client_order_id(&proposal(2)), client_order_id(&proposal(3)));
    }

    #[test]
    fn qty_blows_one_percent() {
        let err = gate(
            proposal(1000),
            &top(),
            &Policy::file_default(),
            &RiskState::paper_book(1_000_000),
            limits(true),
        )
        .unwrap_err();
        assert_eq!(err.code, RejectCode::QtyRecompute);
    }

    #[test]
    fn daily_loss_trips() {
        let mut risk = RiskState::paper_book(10_000_000);
        risk.daily_pnl_cents = -500_000;
        let err = gate(
            proposal(1),
            &top(),
            &Policy::file_default(),
            &risk,
            limits(true),
        )
        .unwrap_err();
        assert_eq!(err.code, RejectCode::DailyLoss);
    }

    #[test]
    fn not_in_top20() {
        let mut p = proposal(1);
        p.occ_symbol = OccSymbol::parse("SPY260417P00420000").unwrap();
        let err = gate(
            p,
            &top(),
            &Policy::file_default(),
            &RiskState::paper_book(100_000_000),
            limits(true),
        )
        .unwrap_err();
        assert_eq!(err.code, RejectCode::NotInTop20);
    }

    #[test]
    fn snapshot_mismatch() {
        let mut p = proposal(1);
        p.snapshot_id = SnapshotId::new("snap-other").unwrap();
        let err = gate(
            p,
            &top(),
            &Policy::file_default(),
            &RiskState::paper_book(100_000_000),
            limits(true),
        )
        .unwrap_err();
        assert_eq!(err.code, RejectCode::StaleSnap);
    }

    #[test]
    fn rth_closed() {
        let err = gate(
            proposal(1),
            &top(),
            &Policy::file_default(),
            &RiskState::paper_book(100_000_000),
            limits(false),
        )
        .unwrap_err();
        assert_eq!(err.code, RejectCode::Rth);
    }

    #[test]
    fn three_rejects_trip_breaker() {
        let mut risk = RiskState::paper_book(100_000_000);
        for _ in 0..3 {
            risk.bump_reject(3);
        }
        assert!(risk.breaker);
        let err = gate(
            proposal(1),
            &top(),
            &Policy::file_default(),
            &risk,
            limits(true),
        )
        .unwrap_err();
        assert_eq!(err.code, RejectCode::Breaker);
    }
}
