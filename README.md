# session-miner

A Rust CLI tool that mines OpenClaw session transcripts for behavioral patterns and generates automation recommendations.

## Installation

```bash
cargo build --release
# Binary at target/release/session-miner
```

## Usage

```bash
# Analyze tool call frequency and patterns
session-miner tools
session-miner tools --sessions 20 --output json

# Detect repeated multi-step workflows
session-miner workflows --sessions 15

# Find patterns in failing commands
session-miner errors --sessions 10

# Generate automation recommendations (analyzes ALL sessions)
session-miner recommendations

# Timeline of activity with peak hours and model usage
session-miner timeline --sessions 20

# Estimate token costs per session
session-miner cost --sessions 10
```

## Subcommands

### `tools [--sessions N] [--output json|table]`

Analyzes tool call frequency and patterns across sessions:

- **Top Tools**: Which tools you use most (exec, write, edit, read, etc.)
- **Exec Command Patterns**: Repeated shell commands (cargo test, git push, python3 scripts)
- **Tool Sequences**: Consecutive tool call patterns that repeat (e.g., `exec → exec → edit`)

Example output:
```
🔧 Top Tools

+----------------+-------+
| Tool           | Count |
+========================+
| exec           | 333   |
| sessions_spawn | 55    |
| edit           | 40    |
| write          | 28    |
+----------------+-------+

⚡ Exec Command Patterns

+---------------------------+-------+
| Command                   | Count |
+===================================+
| cargo test && git push    | 78    |
| cargo build               | 45    |
| python3 -m pytest         | 32    |
+---------------------------+-------+
```

### `workflows [--sessions N]`

Detects repeated multi-step workflows — common sequences of operations you perform:

- **build-test-push**: `cargo test → git push` (the classic CI loop)
- **read-edit cycle**: `read → edit → exec` (investigate, fix, verify)
- **batch repo loop**: Repeated operations across multiple repositories

Each workflow shows frequency and average step count.

### `errors [--sessions N]`

Finds patterns in failing commands:

- What errors repeat most often
- What categories they fall into (compilation, test failure, timeout, file-not-found)
- What fixes typically follow each error type

### `recommendations`

Analyzes all sessions and outputs concrete automation suggestions:

```
💡 Automation Recommendations

1. 🔴 Create alias for 'cargo test && git push'
   You run 'cargo test && git push' 78 times. Consider creating a CLI alias or script.
   Evidence: Exec pattern repeated 78 times

2. 🔴 Create workflow tool: build-test-push
   This 3-step workflow repeats 45 times. Automate it.
   Evidence: Pattern: cargo test → git push → read (45 occurrences)
```

Recommendations are ranked by impact: HIGH (20+ repetitions), MEDIUM (5+), LOW.

### `timeline [--sessions N]`

Activity timeline showing:

- Session start/end times and durations
- Peak activity hours with bar chart visualization
- Model usage breakdown

```
📊 Activity by Hour (UTC)

14:00 ██████████████████████████████ (342)
15:00 ██████████████████ (196)
17:00 ██████████ (112)
```

### `cost [--sessions N]`

Token cost estimates per session, extracted from usage data in messages:

```
💰 Token Cost Estimates

+------------+-------------+--------------+---------------+------------+
| Session ID | Model       | Input Tokens | Output Tokens | Est. Cost  |
+======================================================================+
| abc12345   | zai/glm-5.1 | 4162826      | 211303        | $12.864838 |
+------------+-------------+--------------+---------------+------------+

📈 Totals
  Total Input Tokens:  4215124
  Total Output Tokens: 244918
  Total Est. Cost:     $13.483257
```

## How It Works

### JSONL Streaming Parser

The tool implements its own JSONL streaming parser — no external parsing dependencies. It reads session files line-by-line and extracts relevant fields using a custom JSON string/number extractor:

- Identifies `type` fields to classify entries (session, message, compaction)
- Extracts tool calls from nested message content arrays
- Pulls usage/cost data from assistant messages
- Detects error results from tool result entries

### Session Discovery

Sessions are read from `~/.openclaw/agents/main/sessions/`. Files are filtered to include only `.jsonl` files (excluding `.trajectory.jsonl` and `.deleted.` files), sorted by modification time (newest first).

### Caching

Parsed results are cached in `/tmp/session-miner-cache/` as serialized JSON. Cache entries are invalidated when the source file is newer than the cache file.

## Architecture

```
src/
├── main.rs       # CLI entry point with clap
├── jsonl.rs      # Custom JSONL streaming parser
├── models.rs     # Data structures for sessions and analysis results
├── cache.rs      # File-based caching layer
├── analysis.rs   # All analysis logic (tools, workflows, errors, etc.)
└── output.rs     # Table and JSON output formatting
```

## Dependencies

- **clap** — CLI argument parsing
- **serde** / **serde_json** — Serialization (for caching only, not parsing)
- **anyhow** — Error handling
- **chrono** — Timestamp handling
- **comfy-table** — Pretty table output

## Development

```bash
# Run tests (24 tests)
cargo test

# Build release
cargo build --release

# Run with logging
RUST_LOG=debug cargo run -- tools --sessions 5
```

## License

MIT
