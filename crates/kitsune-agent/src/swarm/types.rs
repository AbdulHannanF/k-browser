use serde::{Deserialize, Serialize};

pub type SwarmId = String;
pub type WorkerId = String;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SwarmConfig {
    pub max_workers: usize,
    pub mode: SwarmMode,
    pub enable_reconciliation: bool,
    pub enable_disagreement: bool,
    pub worker_timeout_seconds: u64,
    pub nav_lock_timeout_seconds: u64,
}

impl Default for SwarmConfig {
    fn default() -> Self {
        Self {
            max_workers: 10,
            mode: SwarmMode::PerspectiveAtScale,
            enable_reconciliation: true,
            enable_disagreement: true,
            worker_timeout_seconds: 120,
            nav_lock_timeout_seconds: 30,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SwarmMode {
    DiscoveryAtScale,
    OutputAtScale,
    PerspectiveAtScale,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum WorkerRole {
    Coordinator,
    Researcher,
    Analyst,
    FactChecker,
    Writer,
    Reviewer,
    Skeptic,
    Synthesizer,
    Custom(String),
}

impl WorkerRole {
    pub fn as_str(&self) -> &str {
        match self {
            WorkerRole::Coordinator => "Coordinator",
            WorkerRole::Researcher => "Researcher",
            WorkerRole::Analyst => "Analyst",
            WorkerRole::FactChecker => "FactChecker",
            WorkerRole::Writer => "Writer",
            WorkerRole::Reviewer => "Reviewer",
            WorkerRole::Skeptic => "Skeptic",
            WorkerRole::Synthesizer => "Synthesizer",
            WorkerRole::Custom(s) => s.as_str(),
        }
    }

    pub fn from_label(s: &str) -> Self {
        match s {
            "Coordinator" => WorkerRole::Coordinator,
            "Researcher" => WorkerRole::Researcher,
            "Analyst" => WorkerRole::Analyst,
            "FactChecker" => WorkerRole::FactChecker,
            "Writer" => WorkerRole::Writer,
            "Reviewer" => WorkerRole::Reviewer,
            "Skeptic" => WorkerRole::Skeptic,
            "Synthesizer" => WorkerRole::Synthesizer,
            s if s.starts_with("Custom:") => WorkerRole::Custom(s[7..].to_string()),
            other => WorkerRole::Custom(other.to_string()),
        }
    }

    pub fn persona_prompt(&self) -> String {
        match self {
            WorkerRole::Researcher => "You are a Researcher. Your specialty is finding authoritative sources. Prefer Download and ReadFile over navigation. Cite everything.".into(),
            WorkerRole::Analyst => "You are an Analyst. Synthesize data into structured insights. Be concise and evidence-based.".into(),
            WorkerRole::FactChecker => "You are a Fact-Checker. Challenge every claim. Look for primary sources. Flag unverified assertions explicitly.".into(),
            WorkerRole::Skeptic => "You are a Skeptic. Find weaknesses, counterarguments, and missing evidence in any conclusion. Be rigorous.".into(),
            WorkerRole::Writer => "You are a Writer. Produce clear, well-structured prose. Organize findings into logical sections with headers.".into(),
            WorkerRole::Reviewer => "You are a Reviewer. Evaluate content quality. Flag logical gaps, unsupported claims, and structural problems.".into(),
            WorkerRole::Synthesizer => "You are a Synthesizer. Merge multiple perspectives into a unified, balanced report that explicitly acknowledges disagreements.".into(),
            _ => String::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed(String),
    Failed(String),
    Cancelled,
}

impl TaskStatus {
    pub fn emoji(&self) -> &str {
        match self {
            TaskStatus::Pending => "🟡",
            TaskStatus::Running => "🔵",
            TaskStatus::Completed(_) => "✅",
            TaskStatus::Failed(_) => "🔴",
            TaskStatus::Cancelled => "⬛",
        }
    }

    pub fn label(&self) -> &str {
        match self {
            TaskStatus::Pending => "Pending",
            TaskStatus::Running => "Running",
            TaskStatus::Completed(_) => "Completed",
            TaskStatus::Failed(_) => "Failed",
            TaskStatus::Cancelled => "Cancelled",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SwarmTask {
    pub id: String,
    pub role: WorkerRole,
    pub prompt: String,
    pub depends_on: Vec<String>,
    pub status: TaskStatus,
    pub worker_id: Option<WorkerId>,
    pub tool_calls_used: u32,
    pub last_message: Option<String>,
}

pub struct SwarmState {
    pub swarm_id: SwarmId,
    pub goal: String,
    pub config: SwarmConfig,
    pub tasks: Vec<SwarmTask>,
    pub final_answer: Option<String>,
    pub total_tool_calls: u32,
    pub started_at: std::time::Instant,
}

impl SwarmState {
    pub fn active_count(&self) -> usize {
        self.tasks.iter().filter(|t| t.status == TaskStatus::Running).count()
    }
    pub fn completed_count(&self) -> usize {
        self.tasks.iter().filter(|t| matches!(t.status, TaskStatus::Completed(_))).count()
    }
    pub fn pending_count(&self) -> usize {
        self.tasks.iter().filter(|t| t.status == TaskStatus::Pending).count()
    }
}
