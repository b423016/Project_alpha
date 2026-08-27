use neural_router_data::ChainSnapshot;
use neural_router_domain::{
    Enriched, OptionRight, Policy, TicketProposal, TicketSide, TimeInForce, Top20,
};

use crate::iv::{greeks_put, implied_vol_put};

const R: f64 = 0.04;
const Q: f64 = 0.012;
const MAX_SPREAD: f64 = 0.05;
const MIN_OI: u64 = 100;
const MIN_VOL: u64 = 10;
const MIN_DTE: u32 = 7;

pub fn funnel(chain: &ChainSnapshot, policy: &Policy) -> Top20 {
    let s = chain.under_price;
    let mut rows = Vec::new();
    for c in chain.puts() {
        if c.dte < MIN_DTE || c.dte < policy.dte_min || c.dte > policy.dte_max {
            continue;
        }
        if c.right != OptionRight::Put {
            continue;
        }
        let Some(mid) = c.mid() else { continue };
        let Some(spread) = c.spread_pct() else {
            continue;
        };
        if spread > MAX_SPREAD {
            continue;
        }
        if c.oi < MIN_OI || c.volume < MIN_VOL {
            continue;
        }
        let t = f64::from(c.dte) / 365.0;
        let Ok(iv) = implied_vol_put(s, c.strike, t, R, Q, mid) else {
            continue;
        };
        let Ok(mut greeks) = greeks_put(s, c.strike, t, R, Q, iv) else {
            continue;
        };
        greeks.iv = iv;
        if greeks.require_finite().is_err() {
            continue;
        }
        if !(greeks.delta >= -1.0 && greeks.delta <= 0.0) {
            continue;
        }
        if greeks.delta < policy.delta_min || greeks.delta > policy.delta_max {
            continue;
        }
        // Layer 6 SVI skipped: surface_svi is off until a surface job exists.
        let utility = policy.lambda_eff * greeks.delta.abs() / mid.max(1e-6);
        rows.push(Enriched {
            contract: c.clone(),
            greeks,
            utility,
        });
    }
    zscore_utility(&mut rows);
    rows.sort_by(|a, b| {
        b.utility
            .partial_cmp(&a.utility)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    rows.truncate(20);
    Top20 {
        snapshot_id: chain.stamps.snapshot_id.clone(),
        policy_id: policy.policy_id.clone(),
        rows,
    }
}

fn zscore_utility(rows: &mut [Enriched]) {
    let n = rows.len();
    if n < 2 {
        return;
    }
    let mean = rows.iter().map(|r| r.utility).sum::<f64>() / n as f64;
    let var = rows
        .iter()
        .map(|r| {
            let d = r.utility - mean;
            d * d
        })
        .sum::<f64>()
        / n as f64;
    let std = var.sqrt();
    if std < 1e-12 {
        return;
    }
    for r in rows.iter_mut() {
        r.utility = (r.utility - mean) / std;
    }
}

/// File-policy pick when LLM_QUANT is off. Still a proposal; Gate decides.
pub fn argmax_utility(top: &Top20, qty: u32) -> Option<TicketProposal> {
    let best = top.rows.first()?;
    let limit_cents = best.contract.mid_cents()?;
    Some(TicketProposal {
        snapshot_id: top.snapshot_id.clone(),
        policy_id: top.policy_id.clone(),
        occ_symbol: best.contract.occ.clone(),
        side: TicketSide::Buy,
        qty,
        limit_cents,
        tif: TimeInForce::Ioc,
        why: "argmax-utility".into(),
    })
}

pub fn decide_cpu_ms(chain: &ChainSnapshot, policy: &Policy) -> (Top20, u128) {
    let start = std::time::Instant::now();
    let top = funnel(chain, policy);
    (top, start.elapsed().as_millis())
}

#[cfg(test)]
mod tests {
    use neural_router_data::load_fixture;
    use neural_router_domain::Policy;

    use super::*;
    use crate::band::{BandStatus, band_status, dollar_delta};

    #[test]
    fn fixture_chain_to_top20_and_band() {
        let chain = load_fixture().unwrap();
        let policy = Policy::file_default();
        assert_eq!(policy.dte_min, 30);
        assert_eq!(policy.dte_max, 60);
        assert_eq!(policy.delta_min, -0.50);
        assert_eq!(policy.delta_max, -0.20);
        let top = funnel(&chain, &policy);
        assert_eq!(top.rows.len(), 20);
        assert_eq!(top.snapshot_id.as_str(), chain.stamps.snapshot_id.as_str());
        assert_eq!(top.policy_id.as_str(), policy.policy_id.as_str());
        assert!(
            top.rows
                .iter()
                .all(|r| r.contract.right == OptionRight::Put)
        );
        assert!(top.rows.iter().all(|r| r.contract.dte >= 7));
        assert!(
            top.rows
                .iter()
                .all(|r| r.contract.dte >= 30 && r.contract.dte <= 60)
        );
        assert!(
            top.rows.iter().all(|r| {
                r.greeks.delta >= policy.delta_min && r.greeks.delta <= policy.delta_max
            })
        );
        assert!(top.rows.windows(2).all(|w| w[0].utility >= w[1].utility));
        let pick = argmax_utility(&top, 1).unwrap();
        assert_eq!(pick.occ_symbol.as_str(), top.rows[0].contract.occ.as_str());
        let status = band_status(dollar_delta(1.0, chain.under_price), -10_000.0, 10_000.0);
        assert!(matches!(status, BandStatus::Hold | BandStatus::Breach));
    }

    #[test]
    fn short_dte_is_dropped_even_if_in_chain() {
        let mut chain = load_fixture().unwrap();
        let put = chain
            .contracts
            .iter_mut()
            .find(|c| c.right == OptionRight::Put)
            .expect("fixture puts");
        put.dte = 5;
        let occ = put.occ.as_str().to_string();
        let top = funnel(&chain, &Policy::file_default());
        assert!(!top.contains_occ(&occ));
    }

    #[test]
    fn hold_and_breach_on_dollar_delta() {
        assert_eq!(
            band_status(dollar_delta(1.0, 500.0), -10_000.0, 10_000.0),
            BandStatus::Hold
        );
        assert_eq!(
            band_status(dollar_delta(1_000.0, 500.0), -10_000.0, 10_000.0),
            BandStatus::Breach
        );
    }

    #[test]
    fn decide_ms_is_cpu_only() {
        let chain = load_fixture().unwrap();
        let policy = Policy::file_default();
        let (_top, ms) = decide_cpu_ms(&chain, &policy);
        assert!(
            ms < 5_000,
            "fixture funnel should be in-process, got {ms}ms"
        );
    }
}
