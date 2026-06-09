//! # Session Miner Tutorial
//!
//! A comprehensive guide to using `session_miner` as a library for mining
//! OpenClaw session transcripts to extract behavioral patterns, workflow
//! insights, error patterns, and cost estimates.
//!
//! ## Overview
//!
//! Session Miner parses `.jsonl` session transcripts produced by OpenClaw
//! agents and exposes structured analysis through a clean library API.
//!
//! The data pipeline is:
//! 1. **Parse** — [`jsonl::parse_session`] reads raw JSONL into [`models::Session`]
//! 2. **Cache** — [`cache::SessionCache`] avoids re-parsing unchanged files
//! 3. **Analyze** — [`analysis`] functions compute patterns, errors, costs, etc.
//! 4. **Output** — [`output`] functions format results as tables or JSON
//!
//! This tutorial demonstrates every layer of the pipeline and shows how to
//! compose them for custom analyses.
//!
//! ## Running
//!
//! ```sh
//! cargo run --example tutorial
//! ```

use anyhow::Result;
use session_miner::{
    analysis, cache, jsonl, models, output,
};

// ---------------------------------------------------------------------------
// 1. Models — The Core Data Types
// ---------------------------------------------------------------------------

/// The [`models::Session`] struct is the top-level parsed representation of a
/// session transcript. It holds metadata (id, timestamps, model) and a list of
/// [`models::SessionEvent`] entries.
///
/// Key fields:
/// - `id` — session identifier (derived from the filename)
/// - `start_time` / `end_time` — ISO timestamps
/// - `model` — the LLM model used (e.g., "zai/glm-5.1")
/// - `events` — ordered list of tool calls, errors, and compactions
/// - `total_input_tokens` / `total_output_tokens` / `total_cost` — usage stats
fn tutorial_construct_session() {
    let session = models::Session {
        id: "my-session".into(),
        file_name: "my-session.jsonl".into(),
        start_time: Some("2026-06-08T14:00:00Z".into()),
        end_time: Some("2026-06-08T14:30:00Z".into()),
        model: "zai/glm-5.1".into(),
        events: vec![
            models::SessionEvent::ToolCall {
                timestamp: Some("2026-06-08T14:01:00Z".into()),
                tool_name: "exec".into(),
                arguments: r#"{"command":"cargo test"}"#.into(),
            },
            models::SessionEvent::Error {
                timestamp: Some("2026-06-08T14:02:00Z".into()),
                tool: "exec".into(),
                message: "test failed: assertion failed".into(),
            },
            models::SessionEvent::ToolCall {
                timestamp: Some("2026-06-08T14:03:00Z".into()),
                tool_name: "edit".into(),
                arguments: r#"{"path":"src/lib.rs","oldText":"fn foo()","newText":"fn bar()"}"#.into(),
            },
            models::SessionEvent::Compaction {
                timestamp: Some("2026-06-08T14:10:00Z".into()),
            },
        ],
        total_input_tokens: 10_000,
        total_output_tokens: 2_500,
        total_cache_read: 5_000,
        total_cost: 0.045,
    };

    // Access event details via pattern matching
    for event in &session.events {
        match event {
            models::SessionEvent::ToolCall { tool_name, arguments, .. } => {
                println!("Tool call: {tool_name} with args: {arguments}");
            }
            models::SessionEvent::Error { tool, message, .. } => {
                println!("Error in {tool}: {message}");
            }
            models::SessionEvent::Compaction { timestamp } => {
                println!("Context compaction at {:?}", timestamp);
            }
        }
    }

    // Each event type exposes its timestamp via the `timestamp()` method
    assert!(session.events[0].timestamp().is_some());
}

// ---------------------------------------------------------------------------
// 2. JSONL Parsing — Turn raw transcripts into structured Sessions
// ---------------------------------------------------------------------------

/// [`jsonl::parse_session`] reads a `.jsonl` file line-by-line and builds a
/// `Session`. It handles:
/// - `session` lines (metadata + start timestamp)
/// - `model_change` lines (provider + model ID)
/// - `message` lines (tool calls, errors, token usage)
/// - `compaction` lines
///
/// It also exposes low-level helpers like [`jsonl::extract_json_string`] for
/// ad-hoc field extraction.
fn tutorial_jsonl_parsing() -> Result<()> {
    // Create a temporary JSONL file for the demo
    let dir = std::env::temp_dir().join("session-miner-tutorial");
    std::fs::create_dir_all(&dir)?;
    let file_path = dir.join("demo-session.jsonl");

    std::fs::write(&file_path, concat!(
        r#"{"type":"session","version":3,"id":"abc123","timestamp":"2026-06-01T12:00:00Z","cwd":"/home/user"}"#, "\n",
        r#"{"type":"model_change","provider":"zai","modelId":"glm-5.1"}"#, "\n",
        r#"{"type":"message","message":{"role":"assistant","content":[{"type":"toolCall","name":"exec","arguments":{"command":"cargo test"}}],"usage":{"input":500,"output":100,"cacheRead":2048,"totalTokens":2648,"cost":{"total":0.003}}}}"#, "\n",
        r#"{"type":"message","message":{"role":"toolResult","toolName":"exec","isError":true,"content":[{"type":"text","text":"test failed: assertion failed at src/lib.rs:42"}]}}"#, "\n",
        r#"{"type":"message","message":{"role":"assistant","content":[{"type":"toolCall","name":"edit","arguments":{"path":"src/lib.rs","oldText":"42","newText":"43"}}],"usage":{"input":800,"output":200,"cacheRead":0,"totalTokens":1000,"cost":{"total":0.005}}}}"#, "\n",
    ))?;

    // Parse the session
    let file = std::fs::File::open(&file_path)?;
    let session = jsonl::parse_session(file, "demo-session.jsonl")?;

    println!("Session ID:    {}", session.id);
    println!("Model:         {}", session.model);
    println!("Start time:    {:?}", session.start_time);
    println!("Events:        {}", session.events.len());
    println!("Input tokens:  {}", session.total_input_tokens);
    println!("Output tokens: {}", session.total_output_tokens);
    println!("Cost:          ${:.6}", session.total_cost);

    // Verify parsed data
    assert_eq!(session.model, "zai/glm-5.1");
    assert_eq!(session.total_input_tokens, 1300); // 500 + 800
    assert_eq!(session.events.len(), 3); // 2 tool calls + 1 error

    // Low-level JSON helpers are also public
    let json = r#"{"name":"Alice","age":30}"#;
    assert_eq!(jsonl::extract_json_string(json, "name"), Some("Alice".to_string()));
    assert!((jsonl::extract_json_number(json, "age") - 30.0).abs() < 0.01);

    Ok(())
}

// ---------------------------------------------------------------------------
// 3. Caching — Avoid re-parsing unchanged files
// ---------------------------------------------------------------------------

/// [`cache::SessionCache`] provides a simple file-based cache keyed by session
/// filename. It invalidates entries when the source file's mtime changes.
///
/// The cache is stored in `/tmp/session-miner-cache/` and serializes `Session`
/// structs as JSON.
fn tutorial_session_cache() {
    let cache = cache::SessionCache::new();

    let session = models::Session {
        id: "cached-demo".into(),
        file_name: "cached-demo.jsonl".into(),
        start_time: Some("2026-06-08T10:00:00Z".into()),
        end_time: None,
        model: "zai/glm-5.1".into(),
        events: vec![],
        total_input_tokens: 100,
        total_output_tokens: 50,
        total_cache_read: 0,
        total_cost: 0.005,
    };

    // Store a session in the cache
    cache.put("cached-demo.jsonl", &session);

    // Retrieve it (returns None if file is newer or doesn't exist)
    if let Some(cached) = cache.get("cached-demo.jsonl") {
        println!("Cache hit! Session ID: {}", cached.id);
        assert_eq!(cached.total_input_tokens, 100);
    }
}

// ---------------------------------------------------------------------------
// 4. Analysis — Extract patterns from sessions
// ---------------------------------------------------------------------------

/// All analysis functions take `&[Session]` and return structured result types.
/// They work on any collection of sessions — whether loaded from files,
/// constructed manually, or filtered from a larger set.

/// **Tool Analysis** — [`analysis::analyze_tools`]
///
/// Returns:
/// - `top_tools` — tools ranked by call frequency
/// - `exec_patterns` — normalized shell commands ranked by frequency
/// - `tool_sequences` — repeating 2-4 step tool chains
fn tutorial_analyze_tools() {
    let sessions = vec![make_session(vec![
        tool_call("exec", r#"{"command":"cargo test"}"#),
        tool_call("exec", r#"{"command":"cargo test"}"#),
        tool_call("write", r#"{"path":"src/lib.rs","content":"fn main(){}"}"#),
        tool_call("exec", r#"{"command":"cargo build"}"#),
        tool_call("exec", r#"{"command":"cargo test"}"#),
        tool_call("edit", r#"{"path":"src/main.rs","oldText":"x","newText":"y"}"#),
    ])];

    let result = analysis::analyze_tools(&sessions);

    // Top tools are sorted by count descending
    println!("Top tools:");
    for tc in &result.top_tools {
        println!("  {} × {}", tc.count, tc.tool);
    }

    // Exec patterns show normalized command frequencies
    println!("\nExec patterns:");
    for ep in &result.exec_patterns {
        println!("  {} × {}", ep.count, ep.command);
    }

    // Tool sequences show repeating chains (length 2-4)
    println!("\nRepeating sequences:");
    for seq in &result.tool_sequences {
        println!("  {} × {}", seq.count, seq.sequence.join(" → "));
    }
}

/// **Workflow Detection** — [`analysis::analyze_workflows`]
///
/// Detects recurring multi-step workflows by normalizing commands into
/// abstract steps and finding common sub-patterns of length 3-6.
///
/// Workflows are named heuristically (e.g., "build-test-push",
/// "read-edit cycle").
fn tutorial_analyze_workflows() {
    // Simulate a repeated build-test-push pattern
    let session = make_session(vec![
        tool_call("exec", r#"{"command":"cargo build"}"#),
        tool_call("exec", r#"{"command":"cargo test"}"#),
        tool_call("exec", r#"{"command":"git push"}"#),
        tool_call("exec", r#"{"command":"cargo build"}"#),
        tool_call("exec", r#"{"command":"cargo test"}"#),
        tool_call("exec", r#"{"command":"git push"}"#),
        tool_call("exec", r#"{"command":"cargo build"}"#),
        tool_call("exec", r#"{"command":"cargo test"}"#),
        tool_call("exec", r#"{"command":"git push"}"#),
    ]);

    let result = analysis::analyze_workflows(&[session]);

    println!("Detected {} workflows:", result.workflows.len());
    for wf in &result.workflows {
        println!(
            "  {} ({} steps, {} occurrences)",
            wf.name, wf.avg_steps as usize, wf.frequency
        );
        println!("    Pattern: {}", wf.pattern.join(" → "));
    }
}

/// **Error Analysis** — [`analysis::analyze_errors`]
///
/// Finds recurring error patterns and the tool calls that follow them
/// (potential "fix" patterns). Errors are categorized into types:
/// file-not-found, permission-denied, timeout, syntax-error,
/// compilation-error, test-failure, network-error, other.
fn tutorial_analyze_errors() {
    let sessions = vec![make_session(vec![
        error("exec", "compilation error: expected `;`"),
        tool_call("exec", r#"{"command":"cargo fix"}"#),
        error("exec", "compilation error: expected `;`"),
        tool_call("exec", r#"{"command":"cargo fix"}"#),
        error("exec", "permission denied: /root/secret"),
    ])];

    let result = analysis::analyze_errors(&sessions);

    // Error patterns: (tool, snippet, count)
    println!("Error patterns:");
    for ep in &result.error_patterns {
        println!("  {} × {} in {}", ep.count, ep.error_snippet, ep.tool);
    }

    // Fix patterns: (error_type, fix_tool, fix_action, occurrences)
    println!("\nFix patterns:");
    for fp in &result.fix_patterns {
        println!(
            "  When ({}) → fix with {}: {} ({} times)",
            fp.error_type, fp.fix_tool, fp.fix_action, fp.occurrences
        );
    }
}

/// **Recommendations** — [`analysis::generate_recommendations`]
///
/// Combines tool, workflow, and error analysis to suggest automation
/// opportunities. Each recommendation has an impact level (HIGH/MEDIUM/LOW).
fn tutorial_recommendations() {
    // Create a session with many repeated patterns to trigger recommendations
    let mut events = Vec::new();
    for _ in 0..12 {
        events.push(tool_call("exec", r#"{"command":"cargo test && git push"}"#));
    }
    let sessions = vec![make_session(events)];

    let result = analysis::generate_recommendations(&sessions);

    println!("{} recommendations:", result.recommendations.len());
    for rec in &result.recommendations {
        let icon = match rec.impact.as_str() {
            "HIGH" => "🔴",
            "MEDIUM" => "🟡",
            _ => "🟢",
        };
        println!("  {icon} {}", rec.title);
        println!("     {}", rec.description);
        println!("     Evidence: {}", rec.evidence);
    }
}

/// **Timeline** — [`analysis::analyze_timeline`]
///
/// Produces session timelines, activity-by-hour histograms, and model usage
/// summaries.
fn tutorial_timeline() {
    let sessions = vec![make_session_with_model(
        "2026-06-08T14:00:00Z",
        "zai/glm-5.1",
        vec![tool_call("exec", r#"{"command":"ls"}"#)],
    )];

    let result = analysis::analyze_timeline(&sessions);

    println!("Session timeline:");
    for st in &result.sessions {
        println!(
            "  {} | {} events | model: {}",
            st.session_id, st.event_count, st.model
        );
    }

    println!("\nPeak hours (UTC):");
    for hour in &result.peak_hours {
        let bar = "█".repeat((hour.event_count as f64 / 10.0).ceil() as usize);
        println!("  {:02}:00 {} ({})", hour.hour, bar, hour.event_count);
    }

    println!("\nModel usage:");
    for mu in &result.model_usage {
        println!("  {} — {} sessions, {} events", mu.model, mu.session_count, mu.total_events);
    }
}

/// **Cost Analysis** — [`analysis::analyze_costs`]
///
/// Estimates token costs per session based on the usage data extracted from
/// assistant messages during parsing.
fn tutorial_costs() {
    let session = models::Session {
        id: "cost-demo".into(),
        file_name: "cost-demo.jsonl".into(),
        start_time: Some("2026-06-08T12:00:00Z".into()),
        end_time: Some("2026-06-08T12:15:00Z".into()),
        model: "zai/glm-5.1".into(),
        events: vec![],
        total_input_tokens: 50_000,
        total_output_tokens: 12_000,
        total_cache_read: 20_000,
        total_cost: 0.125,
    };

    let result = analysis::analyze_costs(&[session]);

    println!("Cost analysis:");
    for sc in &result.sessions {
        println!(
            "  {} ({}) — in: {} out: {} cache: {} cost: ${:.6}",
            sc.session_id,
            sc.model,
            sc.input_tokens,
            sc.output_tokens,
            sc.cache_read,
            sc.estimated_cost,
        );
    }
    println!(
        "  Total: {} input, {} output, ${:.6}",
        result.total_input_tokens, result.total_output_tokens, result.total_cost
    );
}

// ---------------------------------------------------------------------------
// 5. Output — Formatted display
// ---------------------------------------------------------------------------

/// The `output` module provides ready-made table and JSON formatters for all
/// analysis result types. These use `comfy-table` for terminal output.
fn tutorial_output() -> Result<()> {
    let sessions = vec![make_session(vec![
        tool_call("exec", r#"{"command":"cargo test"}"#),
        tool_call("exec", r#"{"command":"cargo test"}"#),
        tool_call("write", r#"{"path":"foo.rs","content":""}"#),
    ])];

    let tools_result = analysis::analyze_tools(&sessions);

    // Table format (default)
    output::print_tools(&tools_result, "table")?;

    // JSON format — useful for piping to jq or APIs
    output::print_tools(&tools_result, "json")?;

    // Each analysis function has a matching output function:
    let workflows_result = analysis::analyze_workflows(&sessions);
    output::print_workflows(&workflows_result)?;

    let errors_result = analysis::analyze_errors(&sessions);
    output::print_errors(&errors_result)?;

    let timeline_result = analysis::analyze_timeline(&sessions);
    output::print_timeline(&timeline_result)?;

    let cost_result = analysis::analyze_costs(&sessions);
    output::print_costs(&cost_result)?;

    Ok(())
}

// ---------------------------------------------------------------------------
// 6. Custom Pipeline — Build your own analysis
// ---------------------------------------------------------------------------

/// Compose the primitives to build a custom analysis pipeline. For example,
/// find sessions with the highest error rate and suggest which tool to fix.
fn tutorial_custom_pipeline() -> Result<()> {
    // In a real scenario, you'd load sessions from disk:
    //   let sessions = load_sessions_from_disk()?;
    // For this demo, we construct them manually.
    let sessions = vec![
        make_session(vec![
            tool_call("exec", r#"{"command":"cargo test"}"#),
            error("exec", "test failed: expected 5, got 3"),
            tool_call("edit", r#"{"path":"src/lib.rs","oldText":"5","newText":"3"}"#),
            tool_call("exec", r#"{"command":"cargo test"}"#),
        ]),
        make_session(vec![
            tool_call("exec", r#"{"command":"cargo build"}"#),
            error("exec", "compilation error: mismatched types"),
            tool_call("exec", r#"{"command":"cargo fix"}"#),
        ]),
    ];

    // Step 1: Get error analysis
    let errors = analysis::analyze_errors(&sessions);

    println!("=== Custom Pipeline: Error-Prone Tools ===\n");
    println!("Top error patterns:");
    for ep in &errors.error_patterns {
        println!("  [{}] {} — \"{}\"", ep.count, ep.tool, ep.error_snippet);
    }

    // Step 2: Get tool frequency for context
    let tools = analysis::analyze_tools(&sessions);
    println!("\nTool usage:");
    for tc in &tools.top_tools {
        println!("  {} × {}", tc.count, tc.tool);
    }

    // Step 3: Compute error rate per tool
    let mut tool_calls: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut tool_errors: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    for tc in &tools.top_tools {
        tool_calls.insert(tc.tool.clone(), tc.count);
    }
    for ep in &errors.error_patterns {
        *tool_errors.entry(ep.tool.clone()).or_insert(0) += ep.count;
    }

    println!("\nError rates:");
    for (tool, calls) in &tool_calls {
        let errs = tool_errors.get(tool).copied().unwrap_or(0);
        let rate = if *calls > 0 { errs as f64 / *calls as f64 * 100.0 } else { 0.0 };
        println!("  {tool}: {rate:.1}% error rate ({errs}/{calls})");
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Helper functions for constructing test data
// ---------------------------------------------------------------------------

/// Create a test session with the given events.
fn make_session(events: Vec<models::SessionEvent>) -> models::Session {
    models::Session {
        id: "test-session".into(),
        file_name: "test-session.jsonl".into(),
        start_time: Some("2026-06-08T14:00:00Z".into()),
        end_time: Some("2026-06-08T14:30:00Z".into()),
        model: "zai/glm-5.1".into(),
        events,
        total_input_tokens: 1000,
        total_output_tokens: 500,
        total_cache_read: 0,
        total_cost: 0.01,
    }
}

/// Create a test session with a specific start time and model.
fn make_session_with_model(
    start: &str,
    model: &str,
    events: Vec<models::SessionEvent>,
) -> models::Session {
    models::Session {
        id: "timed-session".into(),
        file_name: "timed-session.jsonl".into(),
        start_time: Some(start.into()),
        end_time: None,
        model: model.into(),
        events,
        total_input_tokens: 500,
        total_output_tokens: 200,
        total_cache_read: 0,
        total_cost: 0.005,
    }
}

/// Helper to construct a `SessionEvent::ToolCall`.
fn tool_call(name: &str, args: &str) -> models::SessionEvent {
    models::SessionEvent::ToolCall {
        timestamp: None,
        tool_name: name.into(),
        arguments: args.into(),
    }
}

/// Helper to construct a `SessionEvent::Error`.
fn error(tool: &str, msg: &str) -> models::SessionEvent {
    models::SessionEvent::Error {
        timestamp: None,
        tool: tool.into(),
        message: msg.into(),
    }
}

// ---------------------------------------------------------------------------
// Main — Run all tutorials
// ---------------------------------------------------------------------------

fn main() -> Result<()> {
    println!("╔══════════════════════════════════════════════════╗");
    println!("║        Session Miner — Library Tutorial          ║");
    println!("╚══════════════════════════════════════════════════╝\n");

    println!("── 1. Models — Core Data Types ──");
    tutorial_construct_session();
    println!();

    println!("── 2. JSONL Parsing ──");
    tutorial_jsonl_parsing()?;
    println!();

    println!("── 3. Session Cache ──");
    tutorial_session_cache();
    println!();

    println!("── 4a. Tool Analysis ──");
    tutorial_analyze_tools();
    println!();

    println!("── 4b. Workflow Detection ──");
    tutorial_analyze_workflows();
    println!();

    println!("── 4c. Error Analysis ──");
    tutorial_analyze_errors();
    println!();

    println!("── 4d. Recommendations ──");
    tutorial_recommendations();
    println!();

    println!("── 4e. Timeline ──");
    tutorial_timeline();
    println!();

    println!("── 4f. Cost Analysis ──");
    tutorial_costs();
    println!();

    println!("── 5. Output Formatting ──");
    tutorial_output()?;
    println!();

    println!("── 6. Custom Pipeline ──");
    tutorial_custom_pipeline()?;
    println!();

    println!("✅ Tutorial complete!");
    Ok(())
}
