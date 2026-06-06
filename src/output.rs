use anyhow::Result;
use comfy_table::{Cell, Table, ContentArrangement};

use crate::models::*;

pub fn print_tools(result: &ToolsResult, format: &str) -> Result<()> {
    if format == "json" {
        println!("{}", serde_json::to_string_pretty(result)?);
        return Ok(());
    }

    // Top Tools table
    println!("\n🔧 Top Tools\n");
    let mut table = Table::new();
    table.set_content_arrangement(ContentArrangement::Dynamic);
    table.set_header(vec!["Tool", "Count"]);
    for tc in &result.top_tools {
        table.add_row(vec![Cell::new(&tc.tool), Cell::new(&tc.count)]);
    }
    println!("{table}");

    // Exec Patterns
    if !result.exec_patterns.is_empty() {
        println!("\n⚡ Exec Command Patterns\n");
        let mut table = Table::new();
        table.set_content_arrangement(ContentArrangement::Dynamic);
        table.set_header(vec!["Command", "Count"]);
        for ep in result.exec_patterns.iter().take(15) {
            let cmd = if ep.command.len() > 80 {
                format!("{}...", &ep.command[..77])
            } else {
                ep.command.clone()
            };
            table.add_row(vec![Cell::new(&cmd), Cell::new(&ep.count)]);
        }
        println!("{table}");
    }

    // Tool Sequences
    if !result.tool_sequences.is_empty() {
        println!("\n🔗 Repeating Tool Sequences\n");
        let mut table = Table::new();
        table.set_content_arrangement(ContentArrangement::Dynamic);
        table.set_header(vec!["Sequence", "Count"]);
        for seq in result.tool_sequences.iter().take(10) {
            table.add_row(vec![Cell::new(seq.sequence.join(" → ")), Cell::new(&seq.count)]);
        }
        println!("{table}");
    }

    Ok(())
}

pub fn print_workflows(result: &WorkflowsResult) -> Result<()> {
    if result.workflows.is_empty() {
        println!("No repeated workflows detected.");
        return Ok(());
    }

    println!("\n🔄 Detected Workflows\n");
    let mut table = Table::new();
    table.set_content_arrangement(ContentArrangement::Dynamic);
    table.set_header(vec!["Name", "Steps", "Frequency", "Pattern"]);
    for wf in &result.workflows {
        let pattern_str = wf.pattern.iter().take(5).cloned().collect::<Vec<_>>().join(" → ");
        let ellipsis = if wf.pattern.len() > 5 { "..." } else { "" };
        table.add_row(vec![
            Cell::new(&wf.name),
            Cell::new(wf.avg_steps as usize),
            Cell::new(wf.frequency),
            Cell::new(format!("{}{}", pattern_str, ellipsis)),
        ]);
    }
    println!("{table}");
    Ok(())
}

pub fn print_errors(result: &ErrorsResult) -> Result<()> {
    if !result.error_patterns.is_empty() {
        println!("\n❌ Error Patterns\n");
        let mut table = Table::new();
        table.set_content_arrangement(ContentArrangement::Dynamic);
        table.set_header(vec!["Tool", "Error", "Count"]);
        for ep in &result.error_patterns {
            let msg = if ep.error_snippet.len() > 60 {
                format!("{}...", &ep.error_snippet[..57])
            } else {
                ep.error_snippet.clone()
            };
            table.add_row(vec![Cell::new(&ep.tool), Cell::new(&msg), Cell::new(&ep.count)]);
        }
        println!("{table}");
    }

    if !result.fix_patterns.is_empty() {
        println!("\n🩹 Fix Patterns\n");
        let mut table = Table::new();
        table.set_content_arrangement(ContentArrangement::Dynamic);
        table.set_header(vec!["Error Type", "Fix Tool", "Fix Action", "Occurrences"]);
        for fp in &result.fix_patterns {
            let action = if fp.fix_action.len() > 50 {
                format!("{}...", &fp.fix_action[..47])
            } else {
                fp.fix_action.clone()
            };
            table.add_row(vec![
                Cell::new(&fp.error_type),
                Cell::new(&fp.fix_tool),
                Cell::new(&action),
                Cell::new(fp.occurrences),
            ]);
        }
        println!("{table}");
    }

    Ok(())
}

pub fn print_recommendations(result: &RecommendationsResult) -> Result<()> {
    if result.recommendations.is_empty() {
        println!("No automation recommendations at this time.");
        return Ok(());
    }

    println!("\n💡 Automation Recommendations\n");
    for (i, rec) in result.recommendations.iter().enumerate() {
        let impact_icon = match rec.impact.as_str() {
            "HIGH" => "🔴",
            "MEDIUM" => "🟡",
            _ => "🟢",
        };
        println!("{}. {} {} {}", i + 1, impact_icon, rec.title, "");
        println!("   {}", rec.description);
        println!("   Evidence: {}", rec.evidence);
        println!();
    }
    Ok(())
}

pub fn print_timeline(result: &TimelineResult) -> Result<()> {
    // Sessions timeline
    println!("\n📅 Session Timeline\n");
    let mut table = Table::new();
    table.set_content_arrangement(ContentArrangement::Dynamic);
    table.set_header(vec!["Session ID", "Start", "End", "Events", "Model"]);
    for st in &result.sessions {
        let id_short = if st.session_id.len() > 12 {
            &st.session_id[..12]
        } else {
            &st.session_id
        };
        table.add_row(vec![
            Cell::new(id_short),
            Cell::new(st.start.as_deref().unwrap_or("?")),
            Cell::new(st.end.as_deref().unwrap_or("?")),
            Cell::new(st.event_count),
            Cell::new(&st.model),
        ]);
    }
    println!("{table}");

    // Peak hours
    if !result.peak_hours.is_empty() {
        println!("\n📊 Activity by Hour (UTC)\n");
        let max_count = result.peak_hours.iter().map(|h| h.event_count).max().unwrap_or(1);
        for hour in &result.peak_hours {
            let bar_len = (hour.event_count as f64 / max_count as f64 * 40.0) as usize;
            let bar: String = "█".repeat(bar_len);
            println!("{:02}:00 {} ({})", hour.hour, bar, hour.event_count);
        }
    }

    // Model usage
    if !result.model_usage.is_empty() {
        println!("\n🤖 Model Usage\n");
        let mut table = Table::new();
        table.set_content_arrangement(ContentArrangement::Dynamic);
        table.set_header(vec!["Model", "Sessions", "Total Events"]);
        for mu in &result.model_usage {
            table.add_row(vec![Cell::new(&mu.model), Cell::new(mu.session_count), Cell::new(mu.total_events)]);
        }
        println!("{table}");
    }

    Ok(())
}

pub fn print_costs(result: &CostResult) -> Result<()> {
    println!("\n💰 Token Cost Estimates\n");
    let mut table = Table::new();
    table.set_content_arrangement(ContentArrangement::Dynamic);
    table.set_header(vec!["Session ID", "Model", "Input Tokens", "Output Tokens", "Cache Read", "Est. Cost ($)"]);
    for sc in &result.sessions {
        let id_short = if sc.session_id.len() > 12 {
            &sc.session_id[..12]
        } else {
            &sc.session_id
        };
        table.add_row(vec![
            Cell::new(id_short),
            Cell::new(&sc.model),
            Cell::new(sc.input_tokens),
            Cell::new(sc.output_tokens),
            Cell::new(sc.cache_read),
            Cell::new(format!("{:.6}", sc.estimated_cost)),
        ]);
    }
    println!("{table}");

    println!("\n📈 Totals");
    println!("  Total Input Tokens:  {}", result.total_input_tokens);
    println!("  Total Output Tokens: {}", result.total_output_tokens);
    println!("  Total Est. Cost:     ${:.6}", result.total_cost);
    Ok(())
}
