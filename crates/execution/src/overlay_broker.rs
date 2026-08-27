use std::collections::{HashMap, HashSet, VecDeque};
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

/// Scripted EMS statuses. No sockets. 200 accept, 409 duplicate, 403 forbidden.
pub struct ScriptedHttpBroker {
    statuses: Mutex<VecDeque<u16>>,
    inner: MockPaperBroker,
}

impl ScriptedHttpBroker {
    pub fn new(statuses: impl Into<Vec<u16>>) -> Self {
        Self {
            statuses: Mutex::new(VecDeque::from(statuses.into())),
            inner: MockPaperBroker::default(),
        }
    }

    pub fn submit_count(&self) -> usize {
        self.inner.submit_count()
    }
}

impl OverlayBroker for ScriptedHttpBroker {
    fn submit(&self, order: &NewOrder) -> Result<SubmitAck, ExecutionError> {
        let status = self
            .statuses
            .lock()
            .expect("script lock")
            .pop_front()
            .unwrap_or(200);
        match status {
            200 => self.inner.submit(order),
            409 => Ok(SubmitAck {
                broker_id: order.client_order_id.clone(),
                duplicate: true,
            }),
            403 => Err(ExecutionError::Http(403)),
            other => Err(ExecutionError::Http(other)),
        }
    }

    fn position(&self, occ: &str) -> Result<i64, ExecutionError> {
        self.inner.position(occ)
    }
}

/// Alpaca REST placeholder. Fail closed without keys; never called from tests.
pub struct AlpacaOverlay {
    paper: bool,
}

pub const PAPER_BASE: &str = "https://paper-api.alpaca.markets";
pub const LIVE_BASE: &str = "https://api.alpaca.markets";

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

    pub fn base_url(&self) -> &'static str {
        if self.paper { PAPER_BASE } else { LIVE_BASE }
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
    fn scripted_http_200_409_403_without_network() {
        let b = ScriptedHttpBroker::new(vec![200, 409, 403]);
        let o = order("clo-1");
        assert!(!b.submit(&o).unwrap().duplicate);
        assert!(b.submit(&o).unwrap().duplicate);
        assert!(matches!(b.submit(&o), Err(ExecutionError::Http(403))));
        assert_eq!(b.submit_count(), 1);
    }

    #[test]
    fn recon_uses_broker_position() {
        let b = MockPaperBroker::default();
        b.set_position("SPY260417P00500000", 2);
        assert_eq!(b.position("SPY260417P00500000").unwrap(), 2);
        let err = recon_position(1, b.position("SPY260417P00500000").unwrap()).unwrap_err();
        assert_eq!(err.code, RejectCode::StalePos);
    }

    #[test]
    fn live_without_allow_live_is_not_paper() {
        let s = Settings {
            alpaca_api_key: Some("k".into()),
            alpaca_secret_key: Some("s".into()),
            alpaca_paper: false,
            allow_live: false,
            ..Settings::default()
        };
        assert!(matches!(
            AlpacaOverlay::from_settings(&s),
            Err(ExecutionError::NotPaper)
        ));
    }

    #[test]
    fn paper_settings_use_paper_base_url() {
        let s = Settings {
            alpaca_api_key: Some("k".into()),
            alpaca_secret_key: Some("s".into()),
            alpaca_paper: true,
            allow_live: false,
            ..Settings::default()
        };
        let c = AlpacaOverlay::from_settings(&s).unwrap();
        assert_eq!(c.base_url(), PAPER_BASE);
    }
}
