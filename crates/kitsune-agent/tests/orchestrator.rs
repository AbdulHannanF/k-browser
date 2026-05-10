use kitsune_agent::orchestrator::{SubTask, TaskStatus};
use kitsune_agent::agents::booking::{BookingCriteria, BookingPriority};

#[test]
fn sub_task_variants_exist() {
    let _search = SubTask::Search { query: "DAAD scholarship".into(), eligibility_filter: Some("MSc".into()) };
    let _form = SubTask::Form { url: "https://daad.de/apply".into(), candidate_title: None };
    let _submit = SubTask::Submit { site: "https://daad.de".into(), filled_count: 10, submit_selector: None };
}

#[test]
fn task_status_default_is_pending() {
    assert_eq!(TaskStatus::default(), TaskStatus::Pending);
}
