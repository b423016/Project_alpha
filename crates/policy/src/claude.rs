use neural_router_config::Settings;

use crate::PolicyError;

pub struct LlmReq {
    pub prompt_version: &'static str,
    pub user: String,
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
            Err(PolicyError::BrainDown("empty"))
        } else {
            Ok(self.payload.clone())
        }
    }
}

/// Placeholder Messages API client. Missing key → BRAIN_DOWN. No network in tests.
pub struct ClaudeClient {
    key: String,
}

impl ClaudeClient {
    pub fn from_settings(settings: &Settings) -> Result<Self, PolicyError> {
        match &settings.anthropic_api_key {
            Some(k) if !k.is_empty() => Ok(Self { key: k.clone() }),
            _ => Err(PolicyError::BrainDown("missing ANTHROPIC_API_KEY")),
        }
    }
}

impl Llm for ClaudeClient {
    fn complete(&self, _req: &LlmReq) -> Result<String, PolicyError> {
        let _ = self.key.len();
        Err(PolicyError::BrainDown("anthropic_messages_placeholder"))
    }
}

#[cfg(test)]
mod tests {
    use neural_router_config::Settings;

    use super::*;

    #[test]
    fn missing_anthropic_key_is_brain_down() {
        assert!(matches!(
            ClaudeClient::from_settings(&Settings::default()),
            Err(PolicyError::BrainDown(_))
        ));
    }

    #[test]
    fn mock_returns_payload() {
        let m = MockLlm {
            payload: "{}".into(),
        };
        assert_eq!(
            m.complete(&LlmReq {
                prompt_version: "v1",
                user: "x".into()
            })
            .unwrap(),
            "{}"
        );
    }
}
