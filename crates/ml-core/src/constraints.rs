use neural_router_domain::{OrderBookSnapshot, Prediction};

use crate::MlError;

/// Fail closed on NaN/out-of-range probabilities and a crossed book.
pub fn apply_constraints(
    book: &OrderBookSnapshot,
    prediction: Prediction,
) -> Result<Prediction, MlError> {
    match book.spread() {
        Some(spread) if spread >= 0.0 => {}
        Some(_) => return Err(MlError::Constraint("negative spread")),
        None => return Err(MlError::Constraint("empty book")),
    }
    Ok(Prediction {
        spread_widening_prob: unit_interval(prediction.spread_widening_prob)?,
        spread_narrowing_prob: unit_interval(prediction.spread_narrowing_prob)?,
        adverse_selection_prob: unit_interval(prediction.adverse_selection_prob)?,
        confidence: unit_interval(prediction.confidence)?,
        horizon_us: prediction.horizon_us,
    })
}

fn unit_interval(value: f64) -> Result<f64, MlError> {
    if value.is_finite() && (0.0..=1.0).contains(&value) {
        Ok(value)
    } else {
        Err(MlError::Constraint("probability outside [0, 1]"))
    }
}

#[cfg(test)]
mod tests {
    use neural_router_domain::{OrderBookSnapshot, Prediction, PriceLevel};

    use super::*;

    fn book() -> OrderBookSnapshot {
        OrderBookSnapshot {
            symbol: "SPY".into(),
            timestamp_us: 1,
            bids: vec![PriceLevel {
                price: 100.0,
                size: 1.0,
            }],
            asks: vec![PriceLevel {
                price: 100.1,
                size: 1.0,
            }],
        }
    }

    fn pred(p: f64) -> Prediction {
        Prediction {
            spread_widening_prob: p,
            spread_narrowing_prob: 0.2,
            adverse_selection_prob: 0.1,
            confidence: 0.8,
            horizon_us: 500,
        }
    }

    #[test]
    fn accepts_unit_interval() {
        assert!(apply_constraints(&book(), pred(0.7)).is_ok());
    }

    #[test]
    fn rejects_out_of_range() {
        assert!(apply_constraints(&book(), pred(1.2)).is_err());
        assert!(apply_constraints(&book(), pred(f64::NAN)).is_err());
    }
}
