use crate::ai_client::{AgentAiClient, ModelTier};
use crate::error::AgentError;
use crate::swarm::types::{SwarmMode, WorkerRole};

pub async fn reconcile(
    inputs: Vec<(WorkerRole, String)>,
    goal: String,
    mode: SwarmMode,
    ai_client: &AgentAiClient,
) -> Result<String, AgentError> {
    if inputs.is_empty() {
        return Ok(String::new());
    }
    if inputs.len() == 1 {
        return Ok(inputs.into_iter().next().unwrap().1);
    }

    let n = inputs.len();
    let mode_instruction = match mode {
        SwarmMode::DiscoveryAtScale => format!(
            "You are a senior analyst. {} researchers each gathered information on a different \
             slice of the topic: '{}'.\n\
             Instructions:\n\
             1. Deduplicate overlapping findings.\n\
             2. Merge into one unified, comprehensive list.\n\
             3. Preserve unique findings from each agent.\n\
             4. Cite which agent found each key point.",
            n, goal
        ),
        SwarmMode::OutputAtScale => format!(
            "You are a senior editor. {} writers each produced a different section of a \
             document on: '{}'.\n\
             Instructions:\n\
             1. Assemble sections in logical order.\n\
             2. Smooth transitions between sections.\n\
             3. Preserve all citations and data points.\n\
             4. Produce a coherent, publication-ready document.",
            n, goal
        ),
        SwarmMode::PerspectiveAtScale => format!(
            "You are a senior analyst reviewing {} expert perspectives on: '{}'.\n\n\
             REQUIRED — address each of these four points explicitly:\n\
             (a) Points of AGREEMENT across all agents.\n\
             (b) Points of DISAGREEMENT — do NOT paper over them; name them.\n\
             (c) Evaluate which position is better supported by evidence and why.\n\
             (d) Final balanced conclusion that acknowledges dissenting views.",
            n, goal
        ),
    };

    let mut prompt = mode_instruction;
    for (role, output) in &inputs {
        prompt.push_str(&format!("\n\n=== {} ===\n{}", role.as_str(), output));
    }

    ai_client
        .complete(&prompt, ModelTier::Orchestrator)
        .await
        .map_err(|e| AgentError::SwarmCoordinatorFailed(format!("Reconciliation failed: {e}")))
}
