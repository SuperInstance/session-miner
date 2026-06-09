use session_miner::{analysis, cache, jsonl, models, output};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

use crate::analysis::*;
use crate::cache::SessionCache;
use crate::models::Session;

#[derive(Parser)]
#[command(name = "session-miner", version, about = "Mine OpenClaw sessions for behavioral patterns")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Analyze tool call frequency and patterns
    Tools {
        /// Number of recent sessions to analyze (default: 10)
        #[arg(long, short, default_value = "10")]
        sessions: usize,
        /// Output format
        #[arg(long, short, default_value = "table")]
        output: String,
    },
    /// Detect repeated multi-step workflows
    Workflows {
        /// Number of recent sessions to analyze
        #[arg(long, short, default_value = "10")]
        sessions: usize,
    },
    /// Find patterns in failing commands
    Errors {
        /// Number of recent sessions to analyze
        #[arg(long, short, default_value = "10")]
        sessions: usize,
    },
    /// Generate automation recommendations
    Recommendations,
    /// Output a timeline of activity
    Timeline {
        /// Number of recent sessions to analyze
        #[arg(long, short, default_value = "10")]
        sessions: usize,
    },
    /// Estimate token costs per session
    Cost {
        /// Number of recent sessions to analyze
        #[arg(long, short, default_value = "10")]
        sessions: usize,
    },
}

fn default_sessions_dir() -> PathBuf {
    dirs_home().join(".openclaw/agents/main/sessions")
}

fn dirs_home() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/root"))
}

fn load_sessions(count: usize) -> Result<Vec<Session>> {
    let sessions_dir = default_sessions_dir();
    let cache = SessionCache::new();

    // List .jsonl files (not .trajectory.jsonl, not .deleted)
    let mut entries: Vec<_> = std::fs::read_dir(&sessions_dir)
        .with_context(|| format!("Cannot read sessions dir: {}", sessions_dir.display()))?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            name.ends_with(".jsonl") && !name.ends_with(".trajectory.jsonl") && !name.contains(".deleted.")
        })
        .collect();

    // Sort by mtime descending
    entries.sort_by(|a, b| {
        let ma = a.metadata().and_then(|m| m.modified()).ok();
        let mb = b.metadata().and_then(|m| m.modified()).ok();
        mb.cmp(&ma)
    });

    let selected: Vec<_> = entries.into_iter().take(count).collect();
    let mut sessions = Vec::with_capacity(selected.len());

    for entry in &selected {
        let path = entry.path();
        let file_name = path.file_name().unwrap().to_string_lossy().to_string();

        if let Some(cached) = cache.get(&file_name) {
            sessions.push(cached);
            continue;
        }

        let file = std::fs::File::open(&path)
            .with_context(|| format!("Cannot open {}", path.display()))?;
        let session = jsonl::parse_session(file, &file_name)?;
        cache.put(&file_name, &session);
        sessions.push(session);
    }

    Ok(sessions)
}

fn load_all_sessions() -> Result<Vec<Session>> {
    let sessions_dir = default_sessions_dir();
    let cache = SessionCache::new();

    let mut entries: Vec<_> = std::fs::read_dir(&sessions_dir)
        .with_context(|| format!("Cannot read sessions dir: {}", sessions_dir.display()))?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            name.ends_with(".jsonl") && !name.ends_with(".trajectory.jsonl") && !name.contains(".deleted.")
        })
        .collect();

    entries.sort_by(|a, b| {
        let ma = a.metadata().and_then(|m| m.modified()).ok();
        let mb = b.metadata().and_then(|m| m.modified()).ok();
        mb.cmp(&ma)
    });

    let mut sessions = Vec::new();
    for entry in &entries {
        let path = entry.path();
        let file_name = path.file_name().unwrap().to_string_lossy().to_string();

        if let Some(cached) = cache.get(&file_name) {
            sessions.push(cached);
            continue;
        }

        let file = std::fs::File::open(&path)
            .with_context(|| format!("Cannot open {}", path.display()))?;
        let session = jsonl::parse_session(file, &file_name)?;
        cache.put(&file_name, &session);
        sessions.push(session);
    }

    Ok(sessions)
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Tools { sessions, output } => {
            let sess = load_sessions(sessions)?;
            let result = analyze_tools(&sess);
            output::print_tools(&result, &output)?;
        }
        Commands::Workflows { sessions } => {
            let sess = load_sessions(sessions)?;
            let result = analyze_workflows(&sess);
            output::print_workflows(&result)?;
        }
        Commands::Errors { sessions } => {
            let sess = load_sessions(sessions)?;
            let result = analyze_errors(&sess);
            output::print_errors(&result)?;
        }
        Commands::Recommendations => {
            let sess = load_all_sessions()?;
            let result = generate_recommendations(&sess);
            output::print_recommendations(&result)?;
        }
        Commands::Timeline { sessions } => {
            let sess = load_sessions(sessions)?;
            let result = analyze_timeline(&sess);
            output::print_timeline(&result)?;
        }
        Commands::Cost { sessions } => {
            let sess = load_sessions(sessions)?;
            let result = analyze_costs(&sess);
            output::print_costs(&result)?;
        }
    }

    Ok(())
}
