use kitsune_core::{config::EngineConfig, navigation::NavigationHistory, KitsuneEngine, TabState};

#[test]
fn engine_starts_in_stopped_state() {
    let engine = KitsuneEngine::new(EngineConfig::default());
    assert!(!engine.is_running());
    assert!(engine.tabs.is_empty());
}

#[test]
fn empty_navigation_history_cannot_go_forward() {
    let history = NavigationHistory::new();
    assert!(!history.can_go_forward());
}

#[tokio::test]
async fn navigate_marks_existing_tab_as_loading() {
    let mut engine = KitsuneEngine::new(EngineConfig::default());
    let tab_id = engine.new_tab();

    engine
        .navigate(tab_id, "https://example.com")
        .await
        .unwrap();

    let tab = &engine.tabs[tab_id];
    assert_eq!(tab.url.as_deref(), Some("https://example.com"));
    assert_eq!(tab.state, TabState::Loading);
    assert!(tab.is_loading);
}

#[test]
fn close_tab_removes_requested_entry() {
    let mut engine = KitsuneEngine::new(EngineConfig::default());
    let first = engine.new_tab();
    let second = engine.new_tab();
    assert_eq!(first, 0);
    assert_eq!(second, 1);

    engine.close_tab(first);

    assert_eq!(engine.tabs.len(), 1);
}
