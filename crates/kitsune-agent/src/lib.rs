#![allow(warnings)]
// ARCHITECTURE: kitsune-agent is the AI agent runtime.
// Agents are structured, auditable configurations that execute browser
// automation tasks within strict safety constraints.
//
// Key security properties:
// 1. Agents can NEVER bypass HIL for consequential actions
// 2. Agent capabilities are declared in AgentConstraints (not soft instructions)
// 3. Cost accounting is mandatory for all external interactions
// 4. Agents receive opaque tokens from the vault, never raw secrets
// 5. Agent lineage is tracked — sub-agents inherit intersection of parent constraints

pub mod action;
pub mod agents;
pub mod ai_client;
pub mod captcha;
pub use captcha::{CaptchaAgent, CaptchaKind, CaptchaSolverConfig};
pub use ai_client::{AgentAiClient, AiProviderConfig, ModelSlots, ModelTier};
pub mod budget;
pub mod dom_access;
pub mod dom_observer;
pub mod error;
pub mod executor;
pub mod loop_runtime;
pub mod ollama_client;
pub mod profile;
pub use profile::{EducationEntry, LanguageEntry, ProfileIndexer, ProfileSummary};
pub mod runtime;
pub mod spec;
pub mod tools;

pub use action::{parse_action_json, AgentAction};
pub use budget::*;
pub use error::{AgentError, AgentResult};
pub use loop_runtime::{AgentEvent, FilePermSlot, LlmAgentRuntime, StopFlag};
pub use ollama_client::{OllamaClient, DEFAULT_OLLAMA_MODEL, DEFAULT_OLLAMA_URL};
pub use runtime::*;
pub use spec::*;
pub use tools::*;
