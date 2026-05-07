use std::time::Instant;

use kitsune_core::{config::EngineConfig, navigation::NavigationHistory, KitsuneEngine};
use url::Url;

#[test]
fn navigation_history_pushes_large_sequences_quickly() {
    let mut history = NavigationHistory::new();
    let start = Instant::now();

    for i in 0..1_000 {
        history.push(
            Url::parse(&format!("https://example.com/page/{i}")).unwrap(),
            format!("Page {i}"),
        );
    }

    assert!(history.current().is_some());
    assert!(
        start.elapsed().as_millis() < 1_000,
        "history push path regressed past 1s in debug"
    );
}

#[test]
fn opening_many_tabs_remains_linear_and_stable() {
    let mut engine = KitsuneEngine::new(EngineConfig::default());
    let start = Instant::now();

    for _ in 0..200 {
        engine.new_tab();
    }

    assert_eq!(engine.tabs.len(), 200);
    assert!(
        start.elapsed().as_millis() < 1_000,
        "tab allocation regressed past 1s in debug"
    );
}
