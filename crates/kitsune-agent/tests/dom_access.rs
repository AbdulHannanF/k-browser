//! Smoke tests for `DomAccessor`.
//!
//! Real DOM ops require a live WebView2 surface, so these tests cover
//! only the parts that can run headlessly: construction, URL state,
//! navigation, and the AgentError type returned for invalid inputs.
//! Full DOM behaviour is exercised through the running browser.

use kitsune_agent::dom_access::DomAccessor;
use kitsune_agent::executor::WebViewCommand;
use kitsune_hil::HilGate;
use kitsune_vault::VaultBackend;
use std::sync::Arc;
use tokio::sync::mpsc;
use url::Url;

fn setup_accessor() -> (DomAccessor, mpsc::Receiver<WebViewCommand>) {
    let vault = Arc::new(VaultBackend::new("password", &[0; 32]).unwrap());
    let hil_gate = Arc::new(HilGate::new_test_gate());
    let (tx, rx) = mpsc::channel(8);
    let accessor = DomAccessor::new(
        vault,
        hil_gate,
        Url::parse("https://kitsune.sh").unwrap(),
        tx,
    );
    (accessor, rx)
}

#[tokio::test]
async fn dom_accessor_starts_at_initial_url() {
    let (acc, _rx) = setup_accessor();
    assert_eq!(acc.get_current_url().await.unwrap(), "https://kitsune.sh/");
}

#[tokio::test]
async fn navigate_updates_current_url_and_emits_command() {
    let (acc, mut rx) = setup_accessor();
    acc.navigate("https://example.com").await.unwrap();
    assert_eq!(
        acc.get_current_url().await.unwrap(),
        "https://example.com/"
    );
    let cmd = rx.recv().await.expect("navigate should send a command");
    match cmd {
        WebViewCommand::Navigate(url) => assert_eq!(url, "https://example.com/"),
        WebViewCommand::EvalJs(_) => panic!("expected Navigate, got EvalJs"),
        WebViewCommand::EvalJsWithCallback(_, _) => {
            panic!("expected Navigate, got EvalJsWithCallback")
        }
    }
}

#[tokio::test]
async fn navigate_rejects_invalid_url() {
    let (acc, _rx) = setup_accessor();
    let err = acc
        .navigate("not a url")
        .await
        .expect_err("invalid url should error");
    assert!(matches!(
        err,
        kitsune_agent::AgentError::InvalidParameters { .. }
    ));
}
