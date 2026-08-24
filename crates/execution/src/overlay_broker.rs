use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use neural_router_config::Settings;
use neural_router_domain::{NewOrder, Reject, RejectCode};

use crate::ExecutionError;

#[derive(Debug, Clone)]
pub struct SubmitAck {
    pub broker_id: String,
    pub duplicate: bool,
}

pub trait OverlayBroker {
    fn submit(&self, order: &NewOrder) -> Result<SubmitAck, ExecutionError>;
    fn position(&self, occ: &str) -> Result<i64, ExecutionError>;
}

/// In-memory paper EMS. No sockets. Duplicate client_order_id is not a second fill.
pub struct MockPaperBroker {
    submitted: Mutex<HashSet<String>>,
    orders: Mutex<Vec<NewOrder>>,
    positions: Mutex<HashMap<String, i64>>,
}

impl Default for MockPaperBroker {
    fn default() -> Self {
        Self {
            submitted: Mutex::new(HashSet::new()),
            orders: Mutex::new(Vec::new()),
            positions: Mutex::new(HashMap::new()),
        }
    }
}

impl MockPaperBroker {
    pub fn submit_count(&self) -> usize {
        self.submitted.lock().expect("mock lock").len()
    }

    pub fn set_position(&self, occ: &str, qty: i64) {
        self.positions
            .lock()
            .expect("mock lock")
            .insert(occ.into(), qty);
    }
}

impl OverlayBroker for MockPaperBroker {
    fn submit(&self, order: &NewOrder) -> Result<SubmitAck, ExecutionError> {
        let mut ids = self.submitted.lock().expect("mock lock");
        if ids.contains(&order.client_order_id) {
            return Ok(SubmitAck {
                broker_id: order.client_order_id.clone(),
                duplicate: true,
            });
        }
        ids.insert(order.client_order_id.clone());
        self.orders.lock().expect("mock lock").push(order.clone());
        Ok(SubmitAck {
            broker_id: order.client_order_id.clone(),
            duplicate: false,
        })
    }

    fn position(&self, occ: &str) -> Result<i64, ExecutionError> {
        Ok(self
            .positions
            .lock()
            .expect("mock lock")
            .get(occ)
            .copied()
            .unwrap_or(0))
    }
}

/// Alpaca REST placeholder. Fail closed without keys; never called from tests.
pub struct AlpacaOverlay {
    paper: bool,
}

impl AlpacaOverlay {
    pub fn from_settings(settings: &Settings) -> Result<Self, ExecutionError> {
        if settings.alpaca_api_key.is_none() || settings.alpaca_secret_key.is_none() {
            return Err(ExecutionError::MissingCredentials);
        }
        if !settings.alpaca_paper && !settings.allow_live {
            return Err(ExecutionError::NotPaper);
        }
        Ok(Self {
            paper: settings.alpaca_paper,
        })
    }
}

impl OverlayBroker for AlpacaOverlay {
    fn submit(&self, order: &NewOrder) -> Result<SubmitAck, ExecutionError> {
        tracing::info!(
            paper = self.paper,
            id = %order.client_order_id,
            occ = %order.occ.as_str(),
            "alpaca overlay placeholder"
        );
        Err(ExecutionError::NotImplemented {
            feature: "alpaca_options_submit",
        })
    }

    fn position(&self, occ: &str) -> Result<i64, ExecutionError> {
        tracing::info!(%occ, "alpaca overlay position placeholder");
        Err(ExecutionError::NotImplemented {
            feature: "alpaca_options_position",
        })
    }
}

pub fn recon_position(blotter_qty: i64, broker_qty: i64) -> Result<(), Reject> {
    if blotter_qty != broker_qty {
        return Err(Reject::new(
            RejectCode::StalePos,
            "position",
            format!("{blotter_qty} vs {broker_qty}"),
            "blotter/broker mismatch",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use neural_router_config::Settings;
    use neural_router_domain::{
        NewOrder, OccSymbol, PolicyId, SnapshotId, TicketSide, TimeInForce,
    };

    use super::*;

    fn order(id: &str) -> NewOrder {
        NewOrder {
            client_order_id: id.into(),
            occ: OccSymbol::parse("SPY260417P00500000").unwrap(),
            side: TicketSide::Buy,
            qty: 1,
            limit_cents: 810,
            tif: TimeInForce::Ioc,
            snapshot_id: SnapshotId::new("snap-0001").unwrap(),
            policy_id: PolicyId::file_default(),
        }
    }

    #[test]
    fn duplicate_client_order_id_is_not_a_second_order() {
        let b = MockPaperBroker::default();
        let a1 = b.submit(&order("abc")).unwrap();
        let a2 = b.submit(&order("abc")).unwrap();
        assert!(!a1.duplicate);
        assert!(a2.duplicate);
        assert_eq!(b.submit_count(), 1);
    }

    #[test]
    fn recon_mismatch_is_stale_pos() {
        let err = recon_position(2, 0).unwrap_err();
        assert_eq!(err.code, RejectCode::StalePos);
        recon_position(2, 2).unwrap();
    }

    #[test]
    fn alpaca_overlay_fails_closed_without_keys() {
        assert!(matches!(
            AlpacaOverlay::from_settings(&Settings::default()),
            Err(ExecutionError::MissingCredentials)
        ));
    }

    #[test]
    fn live_without_allow_live_is_not_paper() {
        let mut s = Settings::default();
        s.alpaca_api_key = Some("k".into());
        s.alpaca_secret_key = Some("s".into());
        s.alpaca_paper = false;
        s.allow_live = false;
        assert!(matches!(
            AlpacaOverlay::from_settings(&s),
            Err(ExecutionError::NotPaper)
        ));
    }
}
