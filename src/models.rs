use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub file_name: String,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub model: String,
    pub events: Vec<SessionEvent>,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cache_read: u64,
    pub total_cost: f64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum SessionEvent {
    ToolCall {
        timestamp: Option<String>,
        tool_name: String,
        arguments: String,
    },
    Error {
        timestamp: Option<String>,
        tool: String,
        message: String,
    },
    Compaction {
        timestamp: Option<String>,
    },
}

impl SessionEvent {
    pub fn timestamp(&self) -> Option<&String> {
        match self {
            SessionEvent::ToolCall { timestamp, .. } => timestamp.as_ref(),
            SessionEvent::Error { timestamp, .. } => timestamp.as_ref(),
            SessionEvent::Compaction { timestamp } => timestamp.as_ref(),
        }
    }
}

// Analysis result types

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsResult {
    pub top_tools: Vec<ToolCount>,
    pub exec_patterns: Vec<ExecPattern>,
    pub tool_sequences: Vec<ToolSequence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCount {
    pub tool: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecPattern {
    pub command: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSequence {
    pub sequence: Vec<String>,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowsResult {
    pub workflows: Vec<Workflow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    pub name: String,
    pub pattern: Vec<String>,
    pub frequency: usize,
    pub avg_steps: f64,
    pub example_commands: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorsResult {
    pub error_patterns: Vec<ErrorPattern>,
    pub fix_patterns: Vec<FixPattern>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorPattern {
    pub tool: String,
    pub error_snippet: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixPattern {
    pub error_type: String,
    pub fix_tool: String,
    pub fix_action: String,
    pub occurrences: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecommendationsResult {
    pub recommendations: Vec<Recommendation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recommendation {
    pub title: String,
    pub description: String,
    pub evidence: String,
    pub impact: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineResult {
    pub sessions: Vec<SessionTimeline>,
    pub peak_hours: Vec<HourActivity>,
    pub model_usage: Vec<ModelUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionTimeline {
    pub session_id: String,
    pub start: Option<String>,
    pub end: Option<String>,
    pub event_count: usize,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HourActivity {
    pub hour: u32,
    pub event_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelUsage {
    pub model: String,
    pub session_count: usize,
    pub total_events: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostResult {
    pub sessions: Vec<SessionCost>,
    pub total_cost: f64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionCost {
    pub session_id: String,
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read: u64,
    pub estimated_cost: f64,
}
