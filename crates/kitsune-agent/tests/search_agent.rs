use kitsune_agent::agents::search::Candidate;

#[test]
fn candidate_has_required_fields() {
    let c = Candidate {
        title: "DAAD Research Grant".into(),
        url: "https://www.daad.de/grants/123".into(),
        deadline: Some("2026-09-01".into()),
        requirements_summary: "MSc+, GPA >= 3.5".into(),
    };
    assert!(!c.title.is_empty());
    assert!(c.url.starts_with("https://"));
}
