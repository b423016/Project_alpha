use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::Mutex;
use std::time::Duration;

use neural_router_config::Settings;
use neural_router_domain::{NewOrder, Reject, RejectCode, TimeInForce};
use serde_json::json;

use crate::ExecutionError;

pub const PAPER_BASE: &str = "https://paper-api.alpaca.markets";
pub const LIVE_BASE: &str = "https://api.alpaca.markets";

#[derive(Debug, Clone)]
pub struct SubmitAck {
    pub broker_id: String,
    pub duplicate: bool,
}

/// LLD `Broker`. Only `gate` output is submitted.
pub trait Broker {
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

impl Broker for MockPaperBroker {
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

/// Alpaca paper REST. Keys stay in memory; Debug redacts. Tests inject `base`.
pub struct AlpacaOverlay {
    key: String,
    secret: String,
    base: String,
}

impl fmt::Debug for AlpacaOverlay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AlpacaOverlay")
            .field("key", &"[REDACTED]")
            .field("secret", &"[REDACTED]")
            .field("base", &self.base)
            .finish()
    }
}

impl AlpacaOverlay {
    pub fn from_settings(settings: &Settings) -> Result<Self, ExecutionError> {
        let key = settings
            .alpaca_api_key
            .clone()
            .filter(|k| !k.is_empty())
            .ok_or(ExecutionError::MissingCredentials)?;
        let secret = settings
            .alpaca_secret_key
            .clone()
            .filter(|k| !k.is_empty())
            .ok_or(ExecutionError::MissingCredentials)?;
        if !settings.alpaca_paper && !settings.allow_live {
            return Err(ExecutionError::NotPaper);
        }
        let base = if settings.alpaca_paper {
            PAPER_BASE
        } else {
            LIVE_BASE
        };
        Ok(Self {
            key,
            secret,
            base: base.into(),
        })
    }

    pub fn with_base(
        base: impl Into<String>,
        key: impl Into<String>,
        secret: impl Into<String>,
    ) -> Self {
        Self {
            key: key.into(),
            secret: secret.into(),
            base: base.into(),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base
    }

    fn agent() -> ureq::Agent {
        ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(5))
            .build()
    }

    fn headers(&self, req: ureq::Request) -> ureq::Request {
        req.set("APCA-API-KEY-ID", &self.key)
            .set("APCA-API-SECRET-KEY", &self.secret)
    }
}

impl Broker for AlpacaOverlay {
    fn submit(&self, order: &NewOrder) -> Result<SubmitAck, ExecutionError> {
        let tif = match order.tif {
            TimeInForce::Ioc => "ioc",
            TimeInForce::Fok => "fok",
        };
        let limit = format!("{:.2}", order.limit_cents as f64 / 100.0);
        let url = format!("{}/v2/orders", self.base);
        let body = json!({
            "symbol": order.occ.as_str(),
            "qty": order.qty.to_string(),
            "side": "buy",
            "type": "limit",
            "time_in_force": tif,
            "limit_price": limit,
            "client_order_id": order.client_order_id,
        });
        let req = self.headers(Self::agent().post(&url));
        match req.send_json(body) {
            Ok(resp) => {
                let v: serde_json::Value = resp.into_json().unwrap_or(json!({}));
                let broker_id = v
                    .get("id")
                    .and_then(|x| x.as_str())
                    .unwrap_or(&order.client_order_id)
                    .to_string();
                Ok(SubmitAck {
                    broker_id,
                    duplicate: false,
                })
            }
            Err(ureq::Error::Status(409, _)) => Ok(SubmitAck {
                broker_id: order.client_order_id.clone(),
                duplicate: true,
            }),
            Err(ureq::Error::Status(code, _)) => Err(ExecutionError::Http(code)),
            Err(_) => Err(ExecutionError::Http(0)),
        }
    }

    fn position(&self, occ: &str) -> Result<i64, ExecutionError> {
        let url = format!("{}/v2/positions/{}", self.base, occ);
        let req = self.headers(Self::agent().get(&url));
        match req.call() {
            Ok(resp) => {
                let v: serde_json::Value = resp.into_json().unwrap_or(json!({}));
                let qty = v
                    .get("qty")
                    .and_then(|q| q.as_str())
                    .and_then(|s| s.parse::<f64>().ok())
                    .unwrap_or(0.0);
                Ok(qty as i64)
            }
            Err(ureq::Error::Status(404, _)) => Ok(0),
            Err(ureq::Error::Status(code, _)) => Err(ExecutionError::Http(code)),
            Err(_) => Err(ExecutionError::Http(0)),
        }
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
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;

    use neural_router_config::Settings;
    use neural_router_domain::{OccSymbol, PolicyId, SnapshotId, TicketSide, TimeInForce};

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

    /// Loopback HTTP. Sequential POST /v2/orders statuses; GET positions returns qty.
    fn spawn_alpaca_mock(order_codes: Vec<u16>, pos_qty: i64) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let n = Arc::new(AtomicUsize::new(0));
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut s) = stream else { continue };
                let head = read_http_head(&mut s);
                let (code, body) = if head.starts_with("POST /v2/orders") {
                    let i = n.fetch_add(1, Ordering::SeqCst);
                    let code = *order_codes.get(i).unwrap_or(&500);
                    let body = match code {
                        200 => r#"{"id":"br-1","status":"accepted"}"#,
                        409 => r#"{"message":"duplicate client_order_id"}"#,
                        _ => r#"{"message":"forbidden"}"#,
                    };
                    (code, body.to_string())
                } else if head.contains("GET /v2/positions/") {
                    if pos_qty == 0 {
                        (404, r#"{"message":"not found"}"#.into())
                    } else {
                        (
                            200,
                            format!(r#"{{"qty":"{pos_qty}","symbol":"SPY260417P00500000"}}"#),
                        )
                    }
                } else {
                    (404, "{}".into())
                };
                let resp = format!(
                    "HTTP/1.1 {code} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = s.write_all(resp.as_bytes());
            }
        });
        format!("http://{addr}")
    }

    fn read_http_head(s: &mut impl Read) -> String {
        let mut buf = Vec::new();
        let mut b = [0u8; 1];
        while buf.len() < 8192 {
            if s.read(&mut b).unwrap_or(0) == 0 {
                break;
            }
            buf.push(b[0]);
            if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
        }
        String::from_utf8_lossy(&buf).into_owned()
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
    fn debug_redacts_alpaca_keys() {
        let c = AlpacaOverlay::with_base("http://127.0.0.1:1", "sk-live", "sec-live");
        let s = format!("{c:?}");
        assert!(!s.contains("sk-live"));
        assert!(!s.contains("sec-live"));
        assert!(s.contains("[REDACTED]"));
    }

    fn submit_retry(b: &AlpacaOverlay, o: &NewOrder) -> Result<SubmitAck, ExecutionError> {
        let mut last = ExecutionError::Http(0);
        for _ in 0..20 {
            match b.submit(o) {
                Err(ExecutionError::Http(0)) => {
                    last = ExecutionError::Http(0);
                    thread::sleep(Duration::from_millis(15));
                }
                other => return other,
            }
        }
        Err(last)
    }

    #[test]
    fn http_mock_200_409_403_and_position() {
        let base = spawn_alpaca_mock(vec![200, 409, 403], 2);
        let b = AlpacaOverlay::with_base(base, "k", "s");
        let o = order("clo-1");
        assert!(!submit_retry(&b, &o).unwrap().duplicate);
        assert!(submit_retry(&b, &o).unwrap().duplicate);
        assert!(matches!(
            submit_retry(&b, &o),
            Err(ExecutionError::Http(403))
        ));
        assert_eq!(b.position("SPY260417P00500000").unwrap(), 2);
        let err = recon_position(1, b.position("SPY260417P00500000").unwrap()).unwrap_err();
        assert_eq!(err.code, RejectCode::StalePos);
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
