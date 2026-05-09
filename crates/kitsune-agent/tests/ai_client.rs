use kitsune_agent::ai_client::{ModelSlots, ModelTier};

#[test]
fn model_slots_selects_correct_model() {
    let slots = ModelSlots {
        orchestrator: "llama3:70b".into(),
        worker: "llama3:70b".into(),
        fast: "llama3:8b".into(),
    };
    assert_eq!(slots.model_for(ModelTier::Fast), "llama3:8b");
    assert_eq!(slots.model_for(ModelTier::Orchestrator), "llama3:70b");
    assert_eq!(slots.model_for(ModelTier::Worker), "llama3:70b");
}
