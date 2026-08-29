use neural_router_domain::{Reject, RejectCode};
use serde_json::Value;

/// V1: pull `tool_use.input` from a Messages envelope. Bare policy/ticket JSON passes through.
pub fn extract_tool_input(raw: &str, want_name: &str) -> Result<String, Reject> {
    let v: Value = serde_json::from_str(raw).map_err(|e| {
        Reject::new(
            RejectCode::Parse,
            "json",
            e.to_string(),
            "parse/unknown field/type",
        )
    })?;
    let Some(blocks) = v.get("content").and_then(|c| c.as_array()) else {
        return Ok(raw.to_string());
    };
    for b in blocks {
        if b.get("type").and_then(|t| t.as_str()) != Some("tool_use") {
            continue;
        }
        let name = b.get("name").and_then(|n| n.as_str()).unwrap_or("");
        if name != want_name {
            continue;
        }
        let input = b.get("input").ok_or_else(|| {
            Reject::new(
                RejectCode::Parse,
                "tool_use.input",
                name,
                "tool_use missing input",
            )
        })?;
        return Ok(input.to_string());
    }
    Err(Reject::new(
        RejectCode::Parse,
        "tool_use",
        want_name,
        "no matching tool_use",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_object_passes_through() {
        let raw = r#"{"policy_id":"file-default-policy"}"#;
        assert_eq!(extract_tool_input(raw, "emit_policy").unwrap(), raw);
    }

    #[test]
    fn pulls_emit_policy_input() {
        let raw = r#"{
          "content": [
            {"type":"text","text":"ok"},
            {"type":"tool_use","id":"1","name":"emit_policy","input":{"policy_id":"file-default-policy"}}
          ]
        }"#;
        let got = extract_tool_input(raw, "emit_policy").unwrap();
        assert!(got.contains("file-default-policy"));
        assert!(!got.contains("tool_use"));
    }

    #[test]
    fn wrong_tool_is_parse() {
        let raw = r#"{"content":[{"type":"tool_use","name":"emit_ticket","input":{}}]}"#;
        let err = extract_tool_input(raw, "emit_policy").unwrap_err();
        assert_eq!(err.code, RejectCode::Parse);
    }
}
