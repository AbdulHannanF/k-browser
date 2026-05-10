use kitsune_agent::agents::form::{FieldAction, FieldMappingPlan};

#[test]
fn field_mapping_plan_serializes_round_trip() {
    let plan = FieldMappingPlan {
        fields: vec![
            FieldAction::FillStatic { selector: "#name".into(), value: "John Doe".into() },
            FieldAction::Click { selector: "button[type=submit]".into() },
        ],
    };
    let json = serde_json::to_string(&plan).unwrap();
    let restored: FieldMappingPlan = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.fields.len(), 2);
}

#[test]
fn captcha_check_action_serializes() {
    let action = FieldAction::CaptchaCheck;
    let json = serde_json::to_string(&action).unwrap();
    assert!(json.contains("CaptchaCheck"));
}
