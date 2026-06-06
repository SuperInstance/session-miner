use std::io::{BufRead, BufReader};
use std::fs::File;

use crate::models::*;

/// Custom streaming JSONL parser — no external parsing deps.
/// Reads line-by-line and extracts relevant fields from JSON objects.
pub fn parse_session(file: File, file_name: &str) -> anyhow::Result<Session> {
    let reader = BufReader::new(file);
    let mut session = Session {
        id: file_name.trim_end_matches(".jsonl").to_string(),
        file_name: file_name.to_string(),
        start_time: None,
        end_time: None,
        model: String::new(),
        events: Vec::new(),
        total_input_tokens: 0,
        total_output_tokens: 0,
        total_cache_read: 0,
        total_cost: 0.0,
    };

    for line in reader.lines() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Minimal JSON type extraction
        let event_type = extract_json_string(line, "type");

        match event_type.as_deref() {
            Some("session") => {
                if let Some(ts) = extract_json_string(line, "timestamp") {
                    session.start_time = Some(ts);
                }
            }
            Some("model_change") => {
                let provider = extract_json_string(line, "provider").unwrap_or_default();
                let model = extract_json_string(line, "modelId").unwrap_or_default();
                if !model.is_empty() {
                    session.model = format!("{}/{}", provider, model);
                }
            }
            Some("message") => {
                parse_message(line, &mut session)?;
            }
            Some("compaction") => {
                session.events.push(SessionEvent::Compaction {
                    timestamp: extract_json_string(line, "timestamp"),
                });
            }
            _ => {}
        }
    }

    // Last timestamp is end time
    if let Some(last) = session.events.last() {
        session.end_time = last.timestamp().cloned();
    }

    Ok(session)
}

fn parse_message(line: &str, session: &mut Session) -> anyhow::Result<()> {
    let msg_timestamp = extract_json_string(line, "\"timestamp\"");
    let role = extract_json_string(line, "\"role\"");

    // Extract usage data if present (from assistant messages)
    if role.as_deref() == Some("assistant") {
        if let Some(usage) = extract_usage(line) {
            session.total_input_tokens += usage.input;
            session.total_output_tokens += usage.output;
            session.total_cache_read += usage.cache_read;
            session.total_cost += usage.cost;
        }
    }

    // Find toolCall entries — look for {"type":"toolCall" patterns
    let tool_calls = extract_tool_calls(line);
    for tc in tool_calls {
        session.events.push(SessionEvent::ToolCall {
            timestamp: msg_timestamp.clone(),
            tool_name: tc.name,
            arguments: tc.args,
        });
    }

    // Find error tool results
    if role.as_deref() == Some("toolResult") {
        let is_error = line.contains("\"isError\":true");
        if is_error {
            let tool_name = extract_json_string(line, "toolName").unwrap_or_default();
            let error_text = extract_text_from_content(line);
            session.events.push(SessionEvent::Error {
                timestamp: msg_timestamp.clone(),
                tool: tool_name,
                message: error_text,
            });
        }
    }

    Ok(())
}

struct ToolCallExtract {
    name: String,
    args: String,
}

fn extract_tool_calls(line: &str) -> Vec<ToolCallExtract> {
    let mut results = Vec::new();
    let mut search_from = 0;

    while let Some(pos) = line[search_from..].find("\"type\":\"toolCall\"") {
        let region_start = search_from + pos;
        // Find the boundaries of this JSON object
        let obj_start = line[..region_start].rfind('{').unwrap_or(0);
        // Find the matching closing brace
        let obj_end = find_closing_brace(line, obj_start);

        if obj_end > obj_start {
            let obj_str = &line[obj_start..=obj_end];
            let name = extract_json_string(obj_str, "name").unwrap_or_default();
            let args = extract_json_string(obj_str, "arguments").unwrap_or_default();
            if !name.is_empty() {
                results.push(ToolCallExtract { name, args });
            }
        }

        search_from = region_start + 20;
        if search_from >= line.len() {
            break;
        }
    }

    results
}

fn find_closing_brace(s: &str, start: usize) -> usize {
    let bytes = s.as_bytes();
    let mut depth = 0;
    let mut in_string = false;
    let mut escape = false;

    for i in start..bytes.len() {
        let ch = bytes[i];
        if escape {
            escape = false;
            continue;
        }
        if ch == b'\\' && in_string {
            escape = true;
            continue;
        }
        if ch == b'"' {
            in_string = !in_string;
            continue;
        }
        if in_string {
            continue;
        }
        if ch == b'{' {
            depth += 1;
        } else if ch == b'}' {
            depth -= 1;
            if depth == 0 {
                return i;
            }
        }
    }
    s.len().saturating_sub(1)
}

struct UsageExtract {
    input: u64,
    output: u64,
    cache_read: u64,
    cost: f64,
}

fn extract_usage(line: &str) -> Option<UsageExtract> {
    // Find "usage":{...} block
    let usage_start = line.find("\"usage\":{")?;
    let brace_start = usage_start + 8; // point to the '{'
    let brace_end = find_closing_brace(line, brace_start);
    let usage_str = &line[brace_start..=brace_end];

    let input = extract_json_number(usage_str, "input") as u64;
    let output = extract_json_number(usage_str, "output") as u64;
    let cache_read = extract_json_number(usage_str, "cacheRead") as u64;

    // Cost is nested: "cost":{"total":...}
    let cost = if let Some(cost_start) = usage_str.find("\"cost\":{") {
        let cost_brace = cost_start + 7;
        let cost_end = find_closing_brace(usage_str, cost_brace);
        let cost_str = &usage_str[cost_brace..=cost_end];
        extract_json_float(cost_str, "total")
    } else {
        0.0
    };

    Some(UsageExtract {
        input,
        output,
        cache_read,
        cost,
    })
}

fn extract_text_from_content(line: &str) -> String {
    // Try to find "text":"..." in content array
    if let Some(text) = extract_json_string(line, "\"text\"") {
        if text.len() < 500 {
            return text;
        }
        return format!("{}...", &text[..500]);
    }
    String::new()
}

/// Extract a string value for a given key from JSON text.
/// Handles "key":"value" patterns.
pub fn extract_json_string(json: &str, key: &str) -> Option<String> {
    let search_key = if key.starts_with('"') {
        key.to_string()
    } else {
        format!("\"{}\"", key)
    };

    let pattern = format!("{}:", search_key);
    let start = json.find(&pattern)?;

    let val_start = start + pattern.len();
    let rest = &json[val_start..];
    let rest = rest.trim_start();

    if !rest.starts_with('"') {
        return None;
    }

    let bytes = rest.as_bytes();
    let mut result = String::new();
    let mut i = 1; // skip opening quote
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            match bytes[i + 1] {
                b'"' => result.push('"'),
                b'\\' => result.push('\\'),
                b'n' => result.push('\n'),
                b'r' => result.push('\r'),
                b't' => result.push('\t'),
                _ => {
                    result.push(bytes[i] as char);
                    result.push(bytes[i + 1] as char);
                }
            }
            i += 2;
            continue;
        }
        if bytes[i] == b'"' {
            return Some(result);
        }
        result.push(bytes[i] as char);
        i += 1;
    }
    Some(result)
}

/// Extract a numeric value for a given key.
pub fn extract_json_number(json: &str, key: &str) -> f64 {
    let pattern = format!("\"{}\":", key);
    let Some(start) = json.find(&pattern) else {
        return 0.0;
    };
    let val_start = start + pattern.len();
    let rest = json[val_start..].trim_start();

    let end = rest
        .find(|c: char| !c.is_ascii_digit() && c != '.' && c != '-' && c != 'e' && c != 'E' && c != '+')
        .unwrap_or(rest.len().min(20));

    rest[..end].parse().unwrap_or(0.0)
}

pub fn extract_json_float(json: &str, key: &str) -> f64 {
    extract_json_number(json, key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_string_simple() {
        let json = r#"{"type":"session","version":3}"#;
        assert_eq!(extract_json_string(json, "type"), Some("session".to_string()));
        assert_eq!(extract_json_string(json, "version"), None); // not a string
    }

    #[test]
    fn test_extract_string_with_escapes() {
        let json = r#"{"text":"hello \"world\" bye"}"#;
        assert_eq!(extract_json_string(json, "text"), Some("hello \"world\" bye".to_string()));
    }

    #[test]
    fn test_extract_string_with_newlines() {
        let json = r#"{"text":"line1\nline2"}"#;
        assert_eq!(extract_json_string(json, "text"), Some("line1\nline2".to_string()));
    }

    #[test]
    fn test_extract_number() {
        let json = r#"{"input":862,"output":214}"#;
        assert!((extract_json_number(json, "input") - 862.0).abs() < 0.01);
        assert!((extract_json_number(json, "output") - 214.0).abs() < 0.01);
    }

    #[test]
    fn test_extract_float() {
        let json = r#"{"total":0.00459376}"#;
        assert!((extract_json_float(json, "total") - 0.00459376).abs() < 0.0001);
    }

    #[test]
    fn test_find_closing_brace() {
        let s = r#"{"a":{"b":1},"c":2}"#;
        assert_eq!(find_closing_brace(s, 0), s.len() - 1);
    }

    #[test]
    fn test_extract_tool_calls() {
        let line = r#"{"type":"message","message":{"content":[{"type":"toolCall","name":"exec","arguments":{"command":"cargo test"}},{"type":"toolCall","name":"write","arguments":{"path":"foo.rs","content":"fn main(){}"}}]}}"#;
        let calls = extract_tool_calls(line);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].name, "exec");
        assert_eq!(calls[1].name, "write");
    }

    #[test]
    fn test_extract_usage() {
        let line = r#"{"type":"message","message":{"usage":{"input":862,"output":214,"cacheRead":11264,"cacheWrite":0,"reasoningTokens":13,"totalTokens":12340,"cost":{"input":0.0010344,"output":0.000856,"cacheRead":0.0027033599999999997,"cacheWrite":0,"total":0.00459376}}}}"#;
        let usage = extract_usage(line).unwrap();
        assert_eq!(usage.input, 862);
        assert_eq!(usage.output, 214);
        assert!((usage.cost - 0.00459376).abs() < 0.0001);
    }

    #[test]
    fn test_parse_session_basic() {
        let dir = std::env::temp_dir().join("session-miner-test-basic");
        std::fs::create_dir_all(&dir).ok();
        let file_path = dir.join("test.jsonl");
        std::fs::write(&file_path, r#"{"type":"session","version":3,"id":"abc123","timestamp":"2026-06-01T12:00:00Z","cwd":"/home"}
{"type":"model_change","provider":"zai","modelId":"glm-5.1"}
{"type":"message","message":{"role":"assistant","content":[{"type":"toolCall","name":"exec","arguments":{"command":"cargo test"}}],"usage":{"input":100,"output":50,"cacheRead":0,"totalTokens":150,"cost":{"total":0.001}}}}
"#).unwrap();

        let file = std::fs::File::open(&file_path).unwrap();
        let session = parse_session(file, "test.jsonl").unwrap();
        assert_eq!(session.model, "zai/glm-5.1");
        assert_eq!(session.total_input_tokens, 100);
        assert_eq!(session.events.len(), 1);
    }

    #[test]
    fn test_empty_lines_skipped() {
        let dir = std::env::temp_dir().join("session-miner-test-empty");
        std::fs::create_dir_all(&dir).ok();
        let file_path = dir.join("test.jsonl");
        std::fs::write(&file_path, "\n\n{\"type\":\"session\",\"version\":3,\"id\":\"abc\",\"timestamp\":\"2026-06-01T12:00:00Z\"}\n\n").unwrap();

        let file = std::fs::File::open(&file_path).unwrap();
        let session = parse_session(file, "test.jsonl").unwrap();
        assert_eq!(session.id, "test");
    }

    #[test]
    fn test_compaction_event() {
        let dir = std::env::temp_dir().join("session-miner-test-compaction");
        std::fs::create_dir_all(&dir).ok();
        let file_path = dir.join("test.jsonl");
        std::fs::write(&file_path, "{\"type\":\"session\",\"version\":3,\"id\":\"abc\",\"timestamp\":\"2026-06-01T12:00:00Z\"}\n{\"type\":\"compaction\",\"timestamp\":\"2026-06-01T12:05:00Z\"}\n").unwrap();

        let file = std::fs::File::open(&file_path).unwrap();
        let session = parse_session(file, "test.jsonl").unwrap();
        assert!(session.events.iter().any(|e| matches!(e, SessionEvent::Compaction { .. })));
    }
}
