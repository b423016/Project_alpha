use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

use neural_router_domain::{NewOrder, Reject};
use serde::Serialize;

use crate::overlay_broker::{Broker, SubmitAck};

#[derive(Debug, Clone, Serialize)]
pub struct AuditEvent {
    pub ts_ms: i64,
    pub role: String,
    pub tool: String,
    pub accept: bool,
    pub code: Option<String>,
    pub snapshot_id: Option<String>,
    pub policy_id: Option<String>,
    pub prompt_version: Option<String>,
    pub model: Option<String>,
    pub raw: String,
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

impl AuditEvent {
    pub fn reject(r: &Reject, snapshot_id: Option<String>, policy_id: Option<String>) -> Self {
        Self {
            ts_ms: now_ms(),
            role: "gate".into(),
            tool: "gate".into(),
            accept: false,
            code: Some(r.code.as_str().into()),
            snapshot_id,
            policy_id,
            prompt_version: None,
            model: None,
            raw: r.to_string(),
        }
    }

    pub fn accept(order: &NewOrder) -> Self {
        Self {
            ts_ms: now_ms(),
            role: "gate".into(),
            tool: "submit".into(),
            accept: true,
            code: None,
            snapshot_id: Some(order.snapshot_id.as_str().into()),
            policy_id: Some(order.policy_id.as_str().into()),
            prompt_version: None,
            model: None,
            raw: order.client_order_id.clone(),
        }
    }
}

#[derive(Debug, Default)]
pub struct MemoryAudit {
    pub rows: Vec<AuditEvent>,
}

impl MemoryAudit {
    pub fn append(&mut self, ev: AuditEvent) {
        self.rows.push(ev);
    }

    pub fn jsonl(&self) -> String {
        self.rows
            .iter()
            .map(|e| serde_json::to_string(e).expect("audit json"))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

pub fn append_file(path: &Path, ev: &AuditEvent) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut f = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(f, "{}", serde_json::to_string(ev).expect("audit json"))
}

/// Journal before mutate: audit then submit. Reject never reaches the broker.
pub fn submit_after_audit<B: Broker>(
    audit: &mut MemoryAudit,
    broker: &B,
    gated: Result<NewOrder, Reject>,
) -> Result<SubmitAck, Reject> {
    match gated {
        Ok(order) => {
            audit.append(AuditEvent::accept(&order));
            broker.submit(&order).map_err(|e| {
                Reject::new(
                    neural_router_domain::RejectCode::BrainDown,
                    "broker",
                    e.to_string(),
                    "ems error",
                )
            })
        }
        Err(r) => {
            audit.append(AuditEvent::reject(&r, None, None));
            Err(r)
        }
    }
}

const BUCKETS: [u64; 6] = [1, 5, 10, 25, 50, 100];

#[derive(Debug, Default)]
pub struct DecideHist {
    counts: [u64; 6],
    inf: u64,
    sum: u64,
    n: u64,
}

impl DecideHist {
    const BUCKET_LABELS: [&'static str; 7] = ["1", "5", "10", "25", "50", "100", "+Inf"];

    /// Bucket counts for UI histograms; labels align with BUCKETS + Inf.
    pub fn json(&self) -> serde_json::Value {
        let mut counts: Vec<u64> = self.counts.to_vec();
        counts.push(self.inf);
        serde_json::json!({
            "labels": Self::BUCKET_LABELS,
            "counts": counts,
            "sum_ms": self.sum,
            "n": self.n,
        })
    }

    pub fn record(&mut self, ms: u64) {
        self.sum += ms;
        self.n += 1;
        for (i, b) in BUCKETS.iter().enumerate() {
            if ms <= *b {
                self.counts[i] += 1;
                return;
            }
        }
        self.inf += 1;
    }

    pub fn prometheus(&self) -> String {
        let mut out = String::from(
            "# HELP nr_decide_ms overlay kernel decision time (HTTP excluded)\n# TYPE nr_decide_ms histogram\n",
        );
        let mut cum = 0u64;
        for (i, b) in BUCKETS.iter().enumerate() {
            cum += self.counts[i];
            out.push_str(&format!("nr_decide_ms_bucket{{le=\"{b}\"}} {cum}\n"));
        }
        cum += self.inf;
        out.push_str(&format!("nr_decide_ms_bucket{{le=\"+Inf\"}} {cum}\n"));
        out.push_str(&format!("nr_decide_ms_sum {}\n", self.sum));
        out.push_str(&format!("nr_decide_ms_count {}\n", self.n));
        out
    }
}

#[cfg(test)]
mod tests {
    use neural_router_domain::{Reject, RejectCode};

    use super::*;
    use crate::overlay_broker::MockPaperBroker;

    #[test]
    fn hist_json_buckets_align_with_labels() {
        let mut h = DecideHist::default();
        h.record(0); // <= 1ms bucket
        h.record(3); // <= 5ms bucket
        h.record(500); // +Inf bucket
        let v = h.json();
        assert_eq!(v["labels"][6], "+Inf");
        assert_eq!(v["counts"][0], 1);
        assert_eq!(v["counts"][1], 1);
        assert_eq!(v["counts"][6], 1);
        assert_eq!(v["n"], 3);
    }

    #[test]
    fn reject_is_audited_and_never_submitted() {
        let mut audit = MemoryAudit::default();
        let broker = MockPaperBroker::default();
        let err = submit_after_audit(
            &mut audit,
            &broker,
            Err(Reject::new(
                RejectCode::NotInTop20,
                "occ_symbol",
                "X",
                "test",
            )),
        )
        .unwrap_err();
        assert_eq!(err.code, RejectCode::NotInTop20);
        assert_eq!(broker.submit_count(), 0);
        assert_eq!(audit.rows.len(), 1);
        assert!(audit.rows[0].ts_ms > 1_000_000_000_000);
        assert!(!audit.rows[0].accept);
        assert_eq!(audit.rows[0].code.as_deref(), Some("NOT_IN_TOP20"));
        assert!(!audit.jsonl().contains("sk-"));
    }

    #[test]
    fn metrics_include_nr_decide_ms() {
        let mut h = DecideHist::default();
        h.record(12);
        let body = h.prometheus();
        assert!(body.contains("nr_decide_ms"));
        assert!(body.contains("nr_decide_ms_bucket"));
        assert!(body.contains("le=\"50\""));
    }

    #[test]
    fn audit_append_is_monotonic() {
        let mut audit = MemoryAudit::default();
        audit.append(AuditEvent::reject(
            &Reject::new(RejectCode::Parse, "x", "1", "a"),
            None,
            None,
        ));
        audit.append(AuditEvent::reject(
            &Reject::new(RejectCode::Parse, "x", "2", "b"),
            None,
            None,
        ));
        assert_eq!(audit.rows.len(), 2);
        assert!(audit.rows[1].ts_ms >= audit.rows[0].ts_ms);
        assert!(audit.jsonl().contains("PARSE"));
    }
}
