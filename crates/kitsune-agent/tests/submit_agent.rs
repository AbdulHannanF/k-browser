use kitsune_agent::agents::form::FormResult;

#[test]
fn form_result_builds() {
    let r = FormResult {
        site: "https://daad.de".into(),
        filled_count: 12,
        submit_selector: Some("button#submit".into()),
        confirmation_text: None,
    };
    assert_eq!(r.filled_count, 12);
    assert!(r.submit_selector.is_some());
}
