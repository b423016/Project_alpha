use neural_router_domain::{Prediction, Side};

use crate::risk::{RiskManager, RiskState};
use crate::{Broker, ExecutionError};

const SIGNAL_THRESHOLD: f64 = 0.7;

#[derive(Debug, Clone, PartialEq)]
pub enum RouteDecision {
    Hold,
    Submit(NewOrder),
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewOrder {
    pub symbol: String,
    pub side: Side,
    pub notional: f64,
}

pub fn decide(symbol: &str, signal: &Prediction, notional: f64) -> RouteDecision {
    if notional <= 0.0 {
        return RouteDecision::Hold;
    }
    if signal.spread_widening_prob > SIGNAL_THRESHOLD {
        RouteDecision::Submit(NewOrder {
            symbol: symbol.into(),
            side: Side::Buy,
            notional,
        })
    } else if signal.spread_narrowing_prob > SIGNAL_THRESHOLD {
        RouteDecision::Submit(NewOrder {
            symbol: symbol.into(),
            side: Side::Sell,
            notional,
        })
    } else {
        RouteDecision::Hold
    }
}

pub fn route(
    broker: &impl Broker,
    risk: &RiskManager,
    state: &RiskState,
    symbol: &str,
    signal: &Prediction,
) -> Result<RouteDecision, ExecutionError> {
    let decision = risk.check(state)?;
    match decide(symbol, signal, decision.size) {
        RouteDecision::Hold => Ok(RouteDecision::Hold),
        RouteDecision::Submit(order) => {
            broker.submit(&order)?;
            Ok(RouteDecision::Submit(order))
        }
    }
}

#[cfg(test)]
mod tests {
    use neural_router_domain::Prediction;

    use super::*;

    fn signal(widen: f64, narrow: f64) -> Prediction {
        Prediction {
            spread_widening_prob: widen,
            spread_narrowing_prob: narrow,
            adverse_selection_prob: 0.0,
            confidence: 1.0,
            horizon_us: 500,
        }
    }

    #[test]
    fn buys_on_widening() {
        match decide("SPY", &signal(0.8, 0.1), 1000.0) {
            RouteDecision::Submit(order) => {
                assert_eq!(order.side, Side::Buy);
                assert!((order.notional - 1000.0).abs() < 1e-9);
            }
            other => panic!("expected submit, got {other:?}"),
        }
    }

    #[test]
    fn holds_below_threshold() {
        assert_eq!(
            decide("SPY", &signal(0.5, 0.5), 1000.0),
            RouteDecision::Hold
        );
    }
}
