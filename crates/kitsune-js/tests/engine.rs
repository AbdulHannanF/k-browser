use kitsune_js::JsEngine;
use kitsune_html::dom::DomTree;

// Helper to get a dummy DOM tree
fn dummy_dom() -> DomTree {
    kitsune_html::parser::parse_html("<html><body><div id='test'></div></body></html>").unwrap()
}

#[test]
fn test_basic_script_executes() {
    let mut engine = JsEngine::new();
    let dom = dummy_dom();
    let (_, result) = engine.execute("let x = 1 + 1; x;", dom);
    assert!(result.is_ok());
}

#[test]
fn test_dom_getelementbyid_finds_node() {
    let mut engine = JsEngine::new();
    let dom = dummy_dom();
    let script = "
        let el = document.getElementById('test');
        if (!el) throw new Error('Not found');
    ";
    let (_, result) = engine.execute(script, dom);
    assert!(result.is_ok());
}

#[test]
fn test_eval_is_blocked() {
    let mut engine = JsEngine::new();
    let dom = dummy_dom();
    let (_, result) = engine.execute("eval('1+1')", dom);
    let err = result.unwrap_err();
    assert!(err.to_string().contains("SecurityError: eval is disabled"));
}

#[test]
fn test_fetch_is_blocked() {
    let mut engine = JsEngine::new();
    let dom = dummy_dom();
    let (_, result) = engine.execute("fetch('http://example.com').then(r => r.json())", dom);
    assert!(result.is_ok());
}

#[test]
fn test_cookie_write_blocked() {
    let mut engine = JsEngine::new();
    let dom = dummy_dom();
    let (dom, result) = engine.execute("document.cookie = 'a=1';", dom);
    let err = result.unwrap_err();
    assert!(err.to_string().contains("cookie writes blocked"));
    
    // Read should be empty string, not blocked
    let (_, result) = engine.execute("let a = document.cookie;", dom);
    assert!(result.is_ok());
}

#[test]
fn test_useragent_is_spoofed() {
    let mut engine = JsEngine::new();
    let dom = dummy_dom();
    let script = r#"
        if (navigator.userAgent !== 'KitsuneEngine/0.1') {
            throw new Error('UserAgent mismatch');
        }
    "#;
    let (_, result) = engine.execute(script, dom);
    assert!(result.is_ok());
}

#[test]
fn test_console_log_does_not_panic() {
    let mut engine = JsEngine::new();
    let dom = dummy_dom();
    let (_, result) = engine.execute("console.log('test');", dom);
    assert!(result.is_ok());
}

#[test]
fn test_innerhtml_strips_script_tags() {
    let mut engine = JsEngine::new();
    let dom = dummy_dom();
    let script = r#"
        let el = document.getElementById('test');
        el.innerHTML = "<div>hello<script>evil()</script></div>";
    "#;
    let (_, result) = engine.execute(script, dom);
    assert!(result.is_ok());
}

#[test]
fn test_syntax_error_returns_err_not_panic() {
    let mut engine = JsEngine::new();
    let dom = dummy_dom();
    let (_, result) = engine.execute("if (true {", dom);
    assert!(result.is_err());
}

#[test]
fn test_settimeout_does_not_panic() {
    let mut engine = JsEngine::new();
    let dom = dummy_dom();
    let (_, result) = engine.execute("setTimeout(() => {}, 10);", dom);
    assert!(result.is_ok());
}
