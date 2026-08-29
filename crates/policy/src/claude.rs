use std::fmt;
use std::time::Duration;

use neural_router_config::Settings;
use serde_json::json;

use crate::PolicyError;

pub const ANTHROPIC_BASE: &str = "https://api.anthropic.com";

pub struct LlmReq {
    pub prompt_version: &'static str,
    pub user: String,
    pub cache_control: Option<&'static str>,
    pub tool: &'static str,
}

pub trait Llm {
    fn complete(&self, req: &LlmReq) -> Result<String, PolicyError>;
}

pub struct MockLlm {
    pub payload: String,
}

impl Llm for MockLlm {
    fn complete(&self, _req: &LlmReq) -> Result<String, PolicyError> {
        if self.payload.is_empty() {
            Err(PolicyError::BrainDown("empty".into()))
        } else {
            Ok(self.payload.clone())
        }
    }
}

/// Messages API client. Missing key → BRAIN_DOWN. Tests inject `base` (loopback).
pub struct ClaudeClient {
    key: String,
    base: String,
    model: String,
}

impl fmt::Debug for ClaudeClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClaudeClient")
            .field("key", &"[REDACTED]")
            .field("base", &self.base)
            .field("model", &self.model)
            .finish()
    }
}

impl ClaudeClient {
    pub fn from_settings(settings: &Settings) -> Result<Self, PolicyError> {
        match &settings.anthropic_api_key {
            Some(k) if !k.is_empty() => Ok(Self {
                key: k.clone(),
                base: ANTHROPIC_BASE.into(),
                model: "claude-sonnet-4-20250514".into(),
            }),
            _ => Err(PolicyError::BrainDown("missing ANTHROPIC_API_KEY".into())),
        }
    }

    pub fn with_base(base: impl Into<String>, key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            base: base.into(),
            model: "claude-sonnet-4-20250514".into(),
        }
    }
}

fn tools_json() -> serde_json::Value {
    json!([
        {
            "name": "emit_policy",
            "description": "Emit overlay policy JSON. Numbers only; no tickets.",
            "input_schema": {
                "type": "object",
                "additionalProperties": false,
                "required": [
                    "policy_id", "regime", "dte_min", "dte_max",
                    "delta_min", "delta_max", "max_premium_cents",
                    "lambda_svi", "lambda_pca", "lambda_eff", "reason"
                ],
                "properties": {
                    "policy_id": {"type": "string"},
                    "regime": {"type": "string", "enum": ["calm", "vol_expanding", "stress", "unknown"]},
                    "dte_min": {"type": "integer"},
                    "dte_max": {"type": "integer"},
                    "delta_min": {"type": "number"},
                    "delta_max": {"type": "number"},
                    "max_premium_cents": {"type": "integer"},
                    "lambda_svi": {"type": "number"},
                    "lambda_pca": {"type": "number"},
                    "lambda_eff": {"type": "number"},
                    "reason": {"type": "string"}
                }
            }
        },
        {
            "name": "emit_ticket",
            "description": "Emit a BUY ticket proposal. Rust recomputes qty/limit.",
            "input_schema": {
                "type": "object",
                "additionalProperties": false,
                "required": [
                    "snapshot_id", "policy_id", "occ_symbol", "side",
                    "qty", "limit_cents", "tif", "why"
                ],
                "properties": {
                    "snapshot_id": {"type": "string"},
                    "policy_id": {"type": "string"},
                    "occ_symbol": {"type": "string"},
                    "side": {"type": "string", "enum": ["BUY"]},
                    "qty": {"type": "integer"},
                    "limit_cents": {"type": "integer"},
                    "tif": {"type": "string", "enum": ["IOC", "FOK"]},
                    "why": {"type": "string"}
                }
            }
        }
    ])
}

impl Llm for ClaudeClient {
    fn complete(&self, req: &LlmReq) -> Result<String, PolicyError> {
        let url = format!("{}/v1/messages", self.base);
        let mut user = json!([{"type": "text", "text": req.user}]);
        if let Some(ctl) = req.cache_control {
            user = json!([{
                "type": "text",
                "text": req.user,
                "cache_control": {"type": ctl}
            }]);
        }
        let body = json!({
            "model": self.model,
            "max_tokens": 1024,
            "temperature": 0,
            "tools": tools_json(),
            "tool_choice": {"type": "tool", "name": req.tool},
            "messages": [{"role": "user", "content": user}],
        });
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(30))
            .build();
        let resp = agent
            .post(&url)
            .set("x-api-key", &self.key)
            .set("anthropic-version", "2023-06-01")
            .set("content-type", "application/json")
            .send_json(body)
            .map_err(|e| match e {
                ureq::Error::Status(code, _) => {
                    PolicyError::BrainDown(format!("anthropic http {code}"))
                }
                _ => PolicyError::BrainDown("anthropic transport".into()),
            })?;
        resp.into_string()
            .map_err(|_| PolicyError::BrainDown("anthropic empty body".into()))
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    use neural_router_config::Settings;

    use super::*;
    use crate::{extract_tool_input, validate_policy};
    use neural_router_domain::RiskState;

    #[test]
    fn missing_anthropic_key_is_brain_down() {
        assert!(matches!(
            ClaudeClient::from_settings(&Settings::default()),
            Err(PolicyError::BrainDown(_))
        ));
    }

    #[test]
    fn debug_redacts_key() {
        let c = ClaudeClient::with_base("http://127.0.0.1:1", "sk-ant-secret");
        let s = format!("{c:?}");
        assert!(!s.contains("sk-ant-secret"));
        assert!(s.contains("[REDACTED]"));
    }

    #[test]
    fn mock_returns_payload() {
        let m = MockLlm {
            payload: "{}".into(),
        };
        assert_eq!(
            m.complete(&LlmReq {
                prompt_version: "v1",
                user: "x".into(),
                cache_control: Some("ephemeral"),
                tool: "emit_policy",
            })
            .unwrap(),
            "{}"
        );
    }

    fn spawn_messages_ok(body: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut s) = stream else { continue };
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
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = s.write_all(resp.as_bytes());
            }
        });
        format!("http://{addr}")
    }

    #[test]
    fn messages_http_then_v1_extract_validates() {
        const BODY: &str = r#"{"content":[{"type":"tool_use","id":"1","name":"emit_policy","input":{"policy_id":"file-default-policy","regime":"unknown","dte_min":30,"dte_max":60,"delta_min":-0.5,"delta_max":-0.2,"max_premium_cents":100000,"lambda_svi":1.0,"lambda_pca":1.0,"lambda_eff":1.0,"reason":"ok"}}]}"#;
        let base = spawn_messages_ok(BODY);
        let client = ClaudeClient::with_base(base, "test-key");
        let raw = client
            .complete(&LlmReq {
                prompt_version: "v1",
                user: "emit".into(),
                cache_control: Some("ephemeral"),
                tool: "emit_policy",
            })
            .unwrap();
        let inner = extract_tool_input(&raw, "emit_policy").unwrap();
        assert!(inner.contains("file-default-policy"));
        let risk = RiskState::paper_book(1_000_000_000);
        validate_policy(&raw, &risk).unwrap();
    }
}
