use std::collections::HashMap;


use crate::models::*;

/// Analyze tool call frequency and patterns
pub fn analyze_tools(sessions: &[Session]) -> ToolsResult {
    let mut tool_counts: HashMap<String, usize> = HashMap::new();
    let mut exec_commands: HashMap<String, usize> = HashMap::new();
    let mut sequence_tracker: Vec<Vec<String>> = Vec::new();

    for session in sessions {
        let mut current_seq: Vec<String> = Vec::new();

        for event in &session.events {
            if let SessionEvent::ToolCall { tool_name, arguments, .. } = event {
                *tool_counts.entry(tool_name.clone()).or_insert(0) += 1;

                // Track exec command patterns
                if tool_name == "exec" {
                    if let Some(cmd) = extract_command(arguments) {
                        let normalized = normalize_command(&cmd);
                        *exec_commands.entry(normalized).or_insert(0) += 1;
                    }
                }

                // Track sequences — accumulate consecutive tool calls
                current_seq.push(tool_name.clone());
            } else {
                if current_seq.len() >= 2 {
                    sequence_tracker.push(current_seq.clone());
                }
                current_seq.clear();
            }
        }
        if current_seq.len() >= 2 {
            sequence_tracker.push(current_seq);
        }
    }

    // Top tools
    let mut top_tools: Vec<ToolCount> = tool_counts
        .into_iter()
        .map(|(tool, count)| ToolCount { tool, count })
        .collect();
    top_tools.sort_by(|a, b| b.count.cmp(&a.count));

    // Top exec patterns
    let mut exec_patterns: Vec<ExecPattern> = exec_commands
        .into_iter()
        .map(|(command, count)| ExecPattern { command, count })
        .collect();
    exec_patterns.sort_by(|a, b| b.count.cmp(&a.count));

    // Find repeating tool sequences (length 2-4)
    let mut seq_counts: HashMap<String, usize> = HashMap::new();
    for seq in &sequence_tracker {
        for len in 2..=4.min(seq.len()) {
            for window in seq.windows(len) {
                let key = window.join(" → ");
                *seq_counts.entry(key).or_insert(0) += 1;
            }
        }
    }
    let mut tool_sequences: Vec<ToolSequence> = seq_counts
        .into_iter()
        .filter(|(_, count)| *count >= 2)
        .map(|(seq_str, count)| ToolSequence {
            sequence: seq_str.split(" → ").map(String::from).collect(),
            count,
        })
        .collect();
    tool_sequences.sort_by(|a, b| b.count.cmp(&a.count));

    ToolsResult {
        top_tools: top_tools.into_iter().take(20).collect(),
        exec_patterns: exec_patterns.into_iter().take(20).collect(),
        tool_sequences: tool_sequences.into_iter().take(15).collect(),
    }
}

/// Detect repeated multi-step workflows
pub fn analyze_workflows(sessions: &[Session]) -> WorkflowsResult {
    let mut workflow_raw: HashMap<String, Vec<Vec<String>>> = HashMap::new();

    for session in sessions {
        let mut current_flow: Vec<String> = Vec::new();

        for event in &session.events {
            if let SessionEvent::ToolCall { tool_name, arguments, .. } = event {
                let step = if tool_name == "exec" {
                    extract_command(arguments)
                        .map(|c| normalize_workflow_step(&c))
                        .unwrap_or_else(|| tool_name.clone())
                } else {
                    tool_name.clone()
                };
                current_flow.push(step);
            }
        }

        if current_flow.len() >= 2 {
            // Hash the flow pattern to find repeats
            let pattern_key = current_flow.join("|");
            workflow_raw.entry(pattern_key).or_default().push(current_flow);
        }
    }

    // Find common sub-patterns
    let mut sub_pattern_counts: HashMap<Vec<String>, usize> = HashMap::new();
    for (_pattern, flows) in &workflow_raw {
        if flows.len() < 1 {
            continue;
        }
        // Extract common sub-sequences of length 3-8
        for flow in flows {
            for len in 3..=6.min(flow.len()) {
                for window in flow.windows(len) {
                    *sub_pattern_counts.entry(window.to_vec()).or_insert(0) += 1;
                }
            }
        }
    }

    let mut workflows: Vec<Workflow> = sub_pattern_counts
        .into_iter()
        .filter(|(_, count)| *count >= 2)
        .map(|(pattern, frequency)| {
            let name = derive_workflow_name(&pattern);
            let avg_steps = pattern.len() as f64;
            let example_commands = pattern.clone();
            Workflow {
                name,
                pattern,
                frequency,
                avg_steps,
                example_commands,
            }
        })
        .collect();

    workflows.sort_by(|a, b| b.frequency.cmp(&a.frequency));
    workflows.truncate(20);

    WorkflowsResult { workflows }
}

/// Find patterns in failing commands
pub fn analyze_errors(sessions: &[Session]) -> ErrorsResult {
    let mut error_counts: HashMap<(String, String), usize> = HashMap::new();
    let mut fix_tracker: Vec<(String, String, String)> = Vec::new();

    for session in sessions {
        let events: Vec<&SessionEvent> = session.events.iter().collect();

        for (i, event) in events.iter().enumerate() {
            if let SessionEvent::Error { tool, message, .. } = event {
                let snippet = extract_error_snippet(message);
                let key = (tool.clone(), snippet.clone());
                *error_counts.entry(key).or_insert(0) += 1;

                // Look for the next tool call after error as potential fix
                for j in (i + 1)..events.len().min(i + 4) {
                    if let SessionEvent::ToolCall { tool_name, arguments, .. } = events[j] {
                        let fix_action = if tool_name == "exec" {
                            extract_command(arguments).unwrap_or_default()
                        } else {
                            format!("{}(...)", tool_name)
                        };
                        fix_tracker.push((snippet.clone(), tool_name.clone(), fix_action));
                        break;
                    }
                }
            }
        }
    }

    let mut error_patterns: Vec<ErrorPattern> = error_counts
        .into_iter()
        .map(|((tool, error_snippet), count)| ErrorPattern {
            tool,
            error_snippet,
            count,
        })
        .collect::<Vec<_>>();
    error_patterns.sort_by(|a, b| b.count.cmp(&a.count));
    let error_patterns = error_patterns.into_iter().take(20).collect();

    // Aggregate fix patterns
    let mut fix_counts: HashMap<(String, String, String), usize> = HashMap::new();
    for (error_type, fix_tool, fix_action) in &fix_tracker {
        let error_cat = categorize_error(error_type);
        *fix_counts
            .entry((error_cat, fix_tool.clone(), fix_action.clone()))
            .or_insert(0) += 1;
    }

    let mut fix_patterns: Vec<FixPattern> = fix_counts
        .into_iter()
        .map(|((error_type, fix_tool, fix_action), occurrences)| FixPattern {
            error_type,
            fix_tool,
            fix_action,
            occurrences,
        })
        .collect::<Vec<_>>();
    fix_patterns.sort_by(|a, b| b.occurrences.cmp(&a.occurrences));
    let fix_patterns = fix_patterns.into_iter().take(15).collect();

    ErrorsResult {
        error_patterns,
        fix_patterns,
    }
}

/// Generate automation recommendations
pub fn generate_recommendations(sessions: &[Session]) -> RecommendationsResult {
    let tools = analyze_tools(sessions);
    let workflows = analyze_workflows(sessions);
    let errors = analyze_errors(sessions);

    let mut recommendations = Vec::new();

    // Analyze repeated exec command patterns
    for pattern in &tools.exec_patterns {
        if pattern.count >= 5 {
            let cmd_short = if pattern.command.len() > 60 {
                format!("{}...", &pattern.command[..60])
            } else {
                pattern.command.clone()
            };
            recommendations.push(Recommendation {
                title: format!("Create alias for '{}'", cmd_short),
                description: format!(
                    "You run '{}' {} times. Consider creating a CLI alias or script.",
                    cmd_short, pattern.count
                ),
                evidence: format!("Exec pattern repeated {} times", pattern.count),
                impact: if pattern.count >= 20 {
                    "HIGH".into()
                } else {
                    "MEDIUM".into()
                },
            });
        }
    }

    // Analyze repeated tool sequences
    for seq in &tools.tool_sequences {
        if seq.count >= 3 {
            recommendations.push(Recommendation {
                title: format!("Automate {} workflow", seq.sequence.join(" → ")),
                description: format!(
                    "The sequence {} appears {} times. Consider a combined tool or script.",
                    seq.sequence.join(" → "),
                    seq.count
                ),
                evidence: format!("Sequence repeated {} times", seq.count),
                impact: if seq.count >= 10 {
                    "HIGH".into()
                } else {
                    "MEDIUM".into()
                },
            });
        }
    }

    // Workflow-based recommendations
    for wf in &workflows.workflows {
        if wf.frequency >= 3 {
            recommendations.push(Recommendation {
                title: format!("Create workflow tool: {}", wf.name),
                description: format!(
                    "This {}-step workflow repeats {} times. Automate it.",
                    wf.avg_steps as usize, wf.frequency
                ),
                evidence: format!(
                    "Pattern: {} ({} occurrences)",
                    wf.pattern.join(" → "),
                    wf.frequency
                ),
                impact: if wf.frequency >= 10 {
                    "HIGH".into()
                } else {
                    "MEDIUM".into()
                },
            });
        }
    }

    // Error-based recommendations
    for ep in &errors.error_patterns {
        if ep.count >= 3 {
            recommendations.push(Recommendation {
                title: format!("Fix recurring {} error", ep.tool),
                description: format!(
                    "Error '{}' in {} occurs {} times. Add error handling or pre-checks.",
                    ep.error_snippet, ep.tool, ep.count
                ),
                evidence: format!("{} errors in {}", ep.count, ep.tool),
                impact: "MEDIUM".into(),
            });
        }
    }

    // Deduplicate and sort by impact
    recommendations.sort_by(|a, b| {
        let order = |s: &str| if s == "HIGH" { 0 } else if s == "MEDIUM" { 1 } else { 2 };
        order(&a.impact).cmp(&order(&b.impact))
    });

    RecommendationsResult {
        recommendations: recommendations.into_iter().take(15).collect(),
    }
}

/// Timeline analysis
pub fn analyze_timeline(sessions: &[Session]) -> TimelineResult {
    let mut hour_counts: HashMap<u32, usize> = HashMap::new();
    let mut model_counts: HashMap<String, (usize, usize)> = HashMap::new(); // (sessions, events)

    let session_timelines: Vec<SessionTimeline> = sessions
        .iter()
        .map(|s| {
            // Count hours
            if let Some(ref start) = s.start_time {
                if let Some(hour) = extract_hour(start) {
                    *hour_counts.entry(hour).or_insert(0) += s.events.len();
                }
            }

            // Model usage
            let entry = model_counts.entry(s.model.clone()).or_insert((0, 0));
            entry.0 += 1;
            entry.1 += s.events.len();

            SessionTimeline {
                session_id: s.id.clone(),
                start: s.start_time.clone(),
                end: s.end_time.clone(),
                event_count: s.events.len(),
                model: s.model.clone(),
            }
        })
        .collect();

    let mut peak_hours: Vec<HourActivity> = hour_counts
        .into_iter()
        .map(|(hour, event_count)| HourActivity { hour, event_count })
        .collect();
    peak_hours.sort_by(|a, b| a.hour.cmp(&b.hour));

    let model_usage: Vec<ModelUsage> = model_counts
        .into_iter()
        .map(|(model, (session_count, total_events))| ModelUsage {
            model,
            session_count,
            total_events,
        })
        .collect();

    TimelineResult {
        sessions: session_timelines,
        peak_hours,
        model_usage,
    }
}

/// Cost analysis
pub fn analyze_costs(sessions: &[Session]) -> CostResult {
    let session_costs: Vec<SessionCost> = sessions
        .iter()
        .map(|s| SessionCost {
            session_id: s.id.clone(),
            model: s.model.clone(),
            input_tokens: s.total_input_tokens,
            output_tokens: s.total_output_tokens,
            cache_read: s.total_cache_read,
            estimated_cost: s.total_cost,
        })
        .collect();

    let total_cost = sessions.iter().map(|s| s.total_cost).sum();
    let total_input = sessions.iter().map(|s| s.total_input_tokens).sum();
    let total_output = sessions.iter().map(|s| s.total_output_tokens).sum();

    CostResult {
        sessions: session_costs,
        total_cost,
        total_input_tokens: total_input,
        total_output_tokens: total_output,
    }
}

// Helper functions

fn extract_command(arguments: &str) -> Option<String> {
    crate::jsonl::extract_json_string(arguments, "command")
}

fn normalize_command(cmd: &str) -> String {
    // Strip variable parts: paths, specific filenames, UUIDs
    let cmd = cmd.trim();

    // Remove common path prefixes
    let mut result = cmd.to_string();

    // Normalize home dir references
    result = result.replace(&std::env::var("HOME").unwrap_or_default(), "~");

    // Replace specific file/UUID patterns with placeholders
    let _uuid_re = regex_free_replace(&result, |c: char| c.is_ascii_hexdigit() || c == '-');
    // Simple normalization: truncate to first pipe or && if too long
    if result.len() > 120 {
        if let Some(pos) = result.find("&&").or(result.find("||")).or(result.find("|")) {
            result = format!("{}...", &result[..pos]);
        }
    }

    result
}

fn regex_free_replace(s: &str, _is_match: impl Fn(char) -> bool) -> String {
    // We don't actually replace UUIDs for simplicity — the command itself is the pattern
    s.to_string()
}

fn normalize_workflow_step(cmd: &str) -> String {
    let cmd = cmd.trim();

    // Extract the base command
    let base = if let Some(space) = cmd.find(' ') {
        &cmd[..space]
    } else {
        cmd
    };

    // Categorize common commands
    match base {
        "cargo" => {
            if cmd.contains("test") {
                "cargo test".into()
            } else if cmd.contains("build") {
                "cargo build".into()
            } else if cmd.contains("run") {
                "cargo run".into()
            } else if cmd.contains("check") {
                "cargo check".into()
            } else if cmd.contains("clippy") {
                "cargo clippy".into()
            } else {
                format!("cargo {}", extract_subcommand(cmd))
            }
        }
        "git" => {
            if cmd.contains("push") {
                "git push".into()
            } else if cmd.contains("commit") {
                "git commit".into()
            } else if cmd.contains("add") {
                "git add".into()
            } else {
                format!("git {}", extract_subcommand(cmd))
            }
        }
        "python3" | "python" => "python".into(),
        "npm" => format!("npm {}", extract_subcommand(cmd)),
        "npx" => format!("npx {}", extract_subcommand(cmd)),
        _ => base.to_string(),
    }
}

fn extract_subcommand(cmd: &str) -> String {
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    if parts.len() >= 2 {
        parts[1].to_string()
    } else {
        String::new()
    }
}

fn extract_error_snippet(msg: &str) -> String {
    // Take first line or first 100 chars
    let first_line = msg.lines().next().unwrap_or(msg);
    if first_line.len() > 100 {
        format!("{}...", &first_line[..100])
    } else {
        first_line.to_string()
    }
}

fn categorize_error(msg: &str) -> String {
    let msg_lower = msg.to_lowercase();
    if msg_lower.contains("not found") || msg_lower.contains("enoent") {
        "file-not-found".into()
    } else if msg_lower.contains("permission") || msg_lower.contains("eacces") {
        "permission-denied".into()
    } else if msg_lower.contains("timeout") || msg_lower.contains("timed out") {
        "timeout".into()
    } else if msg_lower.contains("syntax") || msg_lower.contains("parse") {
        "syntax-error".into()
    } else if msg_lower.contains("compilation") || msg_lower.contains("compile") {
        "compilation-error".into()
    } else if msg_lower.contains("test") && msg_lower.contains("fail") {
        "test-failure".into()
    } else if msg_lower.contains("connection") || msg_lower.contains("network") {
        "network-error".into()
    } else {
        "other".into()
    }
}

fn extract_hour(timestamp: &str) -> Option<u32> {
    // ISO timestamps: "2026-06-01T12:00:00Z" or "2026-06-01T12:00:00.000Z"
    let time_part = if timestamp.contains('T') {
        timestamp.split('T').nth(1)?
    } else {
        return None;
    };
    time_part.get(..2)?.parse().ok()
}

fn derive_workflow_name(pattern: &[String]) -> String {
    let keywords: Vec<&str> = pattern.iter().map(|s| s.as_str()).collect();

    if keywords.iter().any(|k| k.contains("cargo")) && keywords.iter().any(|k| k.contains("git push")) {
        return "build-test-push".into();
    }
    if keywords.iter().any(|k| k.contains("cargo test")) && keywords.iter().any(|k| k.contains("cargo build")) {
        return "build-test cycle".into();
    }
    if keywords.iter().any(|k| k.contains("git add")) && keywords.iter().any(|k| k.contains("git commit")) {
        return "git-commit workflow".into();
    }
    if keywords.iter().any(|k| k.contains("read")) && keywords.iter().any(|k| k.contains("edit")) {
        return "read-edit cycle".into();
    }
    if keywords.iter().any(|k| k.contains("exec")) && keywords.iter().any(|k| k.contains("write")) {
        return "exec-write cycle".into();
    }

    // Default: join first 3 steps
    pattern.iter().take(3).cloned().collect::<Vec<_>>().join(" → ")
}



#[cfg(test)]
mod tests {
    use super::*;

    fn make_session(events: Vec<SessionEvent>) -> Session {
        Session {
            id: "test".into(),
            file_name: "test.jsonl".into(),
            start_time: Some("2026-06-01T14:00:00Z".into()),
            end_time: Some("2026-06-01T14:30:00Z".into()),
            model: "zai/glm-5.1".into(),
            events,
            total_input_tokens: 0,
            total_output_tokens: 0,
            total_cache_read: 0,
            total_cost: 0.0,
        }
    }

    fn tc(name: &str, args: &str) -> SessionEvent {
        SessionEvent::ToolCall {
            timestamp: None,
            tool_name: name.into(),
            arguments: args.into(),
        }
    }

    fn err(tool: &str, msg: &str) -> SessionEvent {
        SessionEvent::Error {
            timestamp: None,
            tool: tool.into(),
            message: msg.into(),
        }
    }

    #[test]
    fn test_analyze_tools_counts() {
        let sessions = vec![make_session(vec![
            tc("exec", r#"{"command":"cargo test"}"#),
            tc("exec", r#"{"command":"cargo test"}"#),
            tc("write", r#"{"path":"foo.rs","content":""}"#),
        ])];
        let result = analyze_tools(&sessions);
        assert_eq!(result.top_tools[0].tool, "exec");
        assert_eq!(result.top_tools[0].count, 2);
    }

    #[test]
    fn test_analyze_tools_exec_patterns() {
        let sessions = vec![make_session(vec![
            tc("exec", r#"{"command":"cargo test"}"#),
            tc("exec", r#"{"command":"cargo test"}"#),
            tc("exec", r#"{"command":"cargo build"}"#),
        ])];
        let result = analyze_tools(&sessions);
        assert!(result.exec_patterns.iter().any(|p| p.command.contains("cargo test")));
    }

    #[test]
    fn test_analyze_tools_sequences() {
        let sessions = vec![
            make_session(vec![
                tc("exec", r#"{"command":"a"}"#),
                tc("write", r#"{"path":"x"}"#),
            ]),
            make_session(vec![
                tc("exec", r#"{"command":"b"}"#),
                tc("write", r#"{"path":"y"}"#),
            ]),
        ];
        let result = analyze_tools(&sessions);
        assert!(result.tool_sequences.iter().any(|s| s.sequence == vec!["exec", "write"]));
    }

    #[test]
    fn test_analyze_errors_basic() {
        let sessions = vec![make_session(vec![
            err("exec", "cargo test failed: assertion failed"),
            err("exec", "cargo test failed: assertion failed"),
            err("exec", "permission denied: /root/secret"),
        ])];
        let result = analyze_errors(&sessions);
        assert_eq!(result.error_patterns.len(), 2);
    }

    #[test]
    fn test_analyze_errors_with_fix() {
        let sessions = vec![make_session(vec![
            err("exec", "compilation error"),
            tc("exec", r#"{"command":"cargo fix"}"#),
        ])];
        let result = analyze_errors(&sessions);
        assert!(!result.fix_patterns.is_empty());
    }

    #[test]
    fn test_analyze_timeline_hours() {
        let sessions = vec![make_session(vec![tc("exec", "{}")])];
        let result = analyze_timeline(&sessions);
        assert!(result.peak_hours.iter().any(|h| h.hour == 14));
    }

    #[test]
    fn test_analyze_costs_basic() {
        let sessions = vec![Session {
            id: "s1".into(),
            file_name: "s1.jsonl".into(),
            start_time: None,
            end_time: None,
            model: "zai/glm-5.1".into(),
            events: vec![],
            total_input_tokens: 1000,
            total_output_tokens: 500,
            total_cache_read: 2000,
            total_cost: 0.05,
        }];
        let result = analyze_costs(&sessions);
        assert_eq!(result.total_cost, 0.05);
        assert_eq!(result.total_input_tokens, 1000);
    }

    #[test]
    fn test_recommendations_from_patterns() {
        let mut events = Vec::new();
        for _ in 0..10 {
            events.push(tc("exec", r#"{"command":"cargo test && git push"}"#));
            events.push(tc("exec", r#"{"command":"cargo test && git push"}"#));
        }
        let sessions = vec![make_session(events)];
        let result = generate_recommendations(&sessions);
        assert!(!result.recommendations.is_empty());
    }

    #[test]
    fn test_normalize_workflow_step() {
        assert_eq!(normalize_workflow_step("cargo test --lib"), "cargo test");
        assert_eq!(normalize_workflow_step("git push origin main"), "git push");
        assert_eq!(normalize_workflow_step("python3 script.py"), "python");
    }

    #[test]
    fn test_categorize_error() {
        assert_eq!(categorize_error("file not found: foo.rs"), "file-not-found");
        assert_eq!(categorize_error("Permission denied"), "permission-denied");
        assert_eq!(categorize_error("test failed: expected 5"), "test-failure");
        assert_eq!(categorize_error("something weird happened"), "other");
    }

    #[test]
    fn test_extract_hour() {
        assert_eq!(extract_hour("2026-06-01T14:30:00Z"), Some(14));
        assert_eq!(extract_hour("2026-06-01T02:00:00.000Z"), Some(2));
        assert_eq!(extract_hour("not-a-timestamp"), None);
    }

    #[test]
    fn test_derive_workflow_name() {
        assert_eq!(derive_workflow_name(&["cargo test".into(), "git push".into()]), "build-test-push");
        assert_eq!(derive_workflow_name(&["read".into(), "edit".into()]), "read-edit cycle");
    }
}
