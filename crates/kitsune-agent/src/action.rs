//! Runtime agent actions — the decision surface returned by the LLM each loop turn.
//!
//! INVARIANT: this enum is the *complete* set of moves an in-process agent
//! can make. Anything that needs vault disclosure or executes a side effect
//! still has to go through `HilGate::checkpoint` inside the runtime — there
//! is no escape hatch on this enum.

use crate::error::AgentError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum AgentAction {
    Navigate { url: String },
    Click { element_id: usize },
    Fill { element_id: usize, value: String },
    Read { selector: String },
    /// Read a file from the local filesystem (requires user permission modal).
    ReadFile { path: String },
    /// Download a file from a URL and save it to the user's Downloads folder.
    Download { url: String, filename: Option<String> },
    Done { answer: String },
}

/// Parse a model response into an `AgentAction`.
///
/// Tolerates:
/// - Markdown code fences (```json ... ```).
/// - Leading/trailing whitespace and prose around the JSON object.
///
/// Rejects anything that isn't a strict JSON object with the documented shape.
pub fn parse_action_json(s: &str) -> Result<AgentAction, AgentError> {
    let cleaned = strip_fences(s);
    let json_str = extract_first_object(cleaned).unwrap_or(cleaned.to_string());

    let value: serde_json::Value =
        serde_json::from_str(&json_str).map_err(|e| AgentError::ExecutionError(format!(
            "LLM did not return valid JSON: {} (raw: {})",
            e,
            truncate(s, 200)
        )))?;

    // Accept either {"action":"navigate","url":"..."} (flat — what we ask for)
    // or {"action":"navigate","params":{"url":"..."}} (some models nest).
    let normalized = normalize_action(value);

    serde_json::from_value::<AgentAction>(normalized.clone()).map_err(|e| {
        AgentError::ExecutionError(format!(
            "LLM returned an unrecognized action shape: {} (json: {})",
            e, normalized
        ))
    })
}

fn strip_fences(s: &str) -> &str {
    let trimmed = s.trim();
    if let Some(rest) = trimmed.strip_prefix("```") {
        // Could be ```json\n...\n``` or ```\n...\n```
        let after_lang = rest
            .split_once('\n')
            .map(|(_, body)| body)
            .unwrap_or(rest);
        if let Some(end) = after_lang.rfind("```") {
            return after_lang[..end].trim();
        }
        return after_lang.trim();
    }
    trimmed
}

fn extract_first_object(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let start = bytes.iter().position(|&b| b == b'{')?;
    let mut depth = 0i32;
    let mut in_str = false;
    let mut esc = false;
    for (i, &b) in bytes.iter().enumerate().skip(start) {
        if in_str {
            if esc {
                esc = false;
            } else if b == b'\\' {
                esc = true;
            } else if b == b'"' {
                in_str = false;
            }
            continue;
        }
        match b {
            b'"' => in_str = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(s[start..=i].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

fn normalize_action(mut v: serde_json::Value) -> serde_json::Value {
    if let Some(obj) = v.as_object_mut() {
        if let Some(params) = obj.remove("params") {
            if let serde_json::Value::Object(map) = params {
                for (k, val) in map {
                    obj.entry(k).or_insert(val);
                }
            }
        }
    }
    v
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(n).collect();
        out.push('…');
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_flat_navigate() {
        let a = parse_action_json(r#"{"action":"navigate","url":"https://example.com"}"#).unwrap();
        assert_eq!(
            a,
            AgentAction::Navigate {
                url: "https://example.com".into()
            }
        );
    }

    #[test]
    fn parses_done() {
        let a = parse_action_json(r#"{"action":"done","answer":"hello"}"#).unwrap();
        assert_eq!(a, AgentAction::Done { answer: "hello".into() });
    }

    #[test]
    fn strips_markdown_fences() {
        let s = "```json\n{\"action\":\"click\",\"element_id\":3}\n```";
        let a = parse_action_json(s).unwrap();
        assert_eq!(a, AgentAction::Click { element_id: 3 });
    }

    #[test]
    fn handles_nested_params() {
        let s = r#"{"action":"fill","params":{"element_id":1,"value":"foo"}}"#;
        let a = parse_action_json(s).unwrap();
        assert_eq!(
            a,
            AgentAction::Fill {
                element_id: 1,
                value: "foo".into()
            }
        );
    }

    #[test]
    fn handles_preamble() {
        let s = "Sure! Here is the action:\n{\"action\":\"read\",\"selector\":\"h1\"}\nThat's it.";
        let a = parse_action_json(s).unwrap();
        assert_eq!(a, AgentAction::Read { selector: "h1".into() });
    }

    #[test]
    fn rejects_garbage() {
        let r = parse_action_json("definitely not json");
        assert!(r.is_err());
    }
}
