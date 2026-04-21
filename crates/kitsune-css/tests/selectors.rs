use kitsune_css::{ComputedStyle, DisplayType, CssColor};
use kitsune_css::selector::SelectorEngine;
use kitsune_html::parser::parse_html;
use kitsune_css::selector::DomElement;

fn assert_selector(html: &str, css: &str, target_id: &str, verify: impl Fn(&ComputedStyle)) {
    let dom = parse_html(html).unwrap();
    let rules = SelectorEngine::parse_stylesheet(css);
    
    // Find node with target_id
    let mut target_node_id = None;
    for node in dom.nodes() {
        let id = node.id;
        if let kitsune_html::dom::NodeType::Element(ref data) = node.node_type {
            if data.get_attribute("id") == Some(target_id) {
                target_node_id = Some(id);
                break;
            }
        }
    }
    
    let target_node_id = target_node_id.expect("Target node not found");
    let element = DomElement { tree: &dom, id: target_node_id };
    
    let matched = SelectorEngine::match_rules(&element, &rules);
    let parent_style = ComputedStyle::default();
    let computed = SelectorEngine::compute_style(&element, &matched, &parent_style);
    
    verify(&computed);
}

#[test]
fn test_type_selector() {
    assert_selector(
        "<p id='target'></p>",
        "p { color: #ff0000; }",
        "target",
        |style| assert_eq!(style.color.r, 255)
    );
}

#[test]
fn test_class_selector() {
    assert_selector(
        "<div id='target' class='box'></div>",
        ".box { display: flex; }",
        "target",
        |style| assert!(matches!(style.display, DisplayType::Flex))
    );
}

#[test]
fn test_id_selector() {
    assert_selector(
        "<span id='unique'></span>",
        "#unique { font-weight: 700; }",
        "unique",
        |style| assert_eq!(style.font_weight, 700)
    );
}

#[test]
fn test_descendant_selector() {
    assert_selector(
        "<div><p id='target'></p></div>",
        "div p { font-size: 20px; }",
        "target",
        |style| assert_eq!(style.font_size, 20.0)
    );
}

#[test]
fn test_child_selector() {
    assert_selector(
        "<ul><li id='target'></li></ul>",
        "ul > li { margin-top: 10px; }",
        "target",
        |style| assert_eq!(style.margin.top, 10.0)
    );
}

#[test]
fn test_multiple_classes() {
    assert_selector(
        "<button id='target' class='btn primary'></button>",
        ".btn.primary { background-color: #00ff00; }",
        "target",
        |style| assert_eq!(style.background_color.g, 255)
    );
}

#[test]
fn test_specificity_class_overrides_type() {
    assert_selector(
        "<p id='target' class='text'></p>",
        "p { font-size: 12px; } .text { font-size: 16px; }",
        "target",
        |style| assert_eq!(style.font_size, 16.0)
    );
}

#[test]
fn test_specificity_id_overrides_class() {
    assert_selector(
        "<div id='target' class='box'></div>",
        ".box { color: blue; } #target { color: red; }",
        "target",
        |style| assert_eq!(style.color.r, 255)
    );
}

#[test]
fn test_cascade_order() {
    assert_selector(
        "<div id='target'></div>",
        "div { margin-top: 5px; } div { margin-top: 10px; }",
        "target",
        |style| assert_eq!(style.margin.top, 10.0)
    );
}

#[test]
fn test_sibling_selector_noop() {
    // Current dom engine doesn't track siblings for selectors properly, ensuring it doesn't crash
    assert_selector(
        "<div><h1 id='target'></h1><p></p></div>",
        "h1 + p { color: red; }",
        "target",
        |style| assert_eq!(style.color.r, 0)
    );
}

#[test]
fn test_inheritance_font_family() {
    let css = "body { font-family: custom_font; }";
    let html = "<body id='body'><p id='target'></p></body>";
    
    let dom = parse_html(html).unwrap();
    let rules = SelectorEngine::parse_stylesheet(css);
    
    // Find node with target_id
    let mut target_node_id = None;
    let mut body_node_id = None;
    for node in dom.nodes() {
        let id = node.id;
        if let kitsune_html::dom::NodeType::Element(ref data) = node.node_type {
            if data.get_attribute("id") == Some("target") {
                target_node_id = Some(id);
            }
            if data.get_attribute("id") == Some("body") {
                body_node_id = Some(id);
            }
        }
    }
    
    let target_node_id = target_node_id.unwrap();
    let body_node_id = body_node_id.unwrap();
    
    let body_element = DomElement { tree: &dom, id: body_node_id };
    let matched_body = SelectorEngine::match_rules(&body_element, &rules);
    let body_style = SelectorEngine::compute_style(&body_element, &matched_body, &ComputedStyle::default());
    
    let target_element = DomElement { tree: &dom, id: target_node_id };
    let matched_target = SelectorEngine::match_rules(&target_element, &rules);
    let target_style = SelectorEngine::compute_style(&target_element, &matched_target, &body_style);
    
    assert_eq!(target_style.font_family, "custom_font");
}

#[test]
fn test_inheritance_color() {
    let css = "body { color: #123456; }";
    let html = "<body id='body'><span id='target'></span></body>";
    
    let dom = parse_html(html).unwrap();
    let rules = SelectorEngine::parse_stylesheet(css);
    
    let mut target_node_id = None;
    let mut body_node_id = None;
    for node in dom.nodes() {
        let id = node.id;
        if let kitsune_html::dom::NodeType::Element(ref data) = node.node_type {
            if data.get_attribute("id") == Some("target") { target_node_id = Some(id); }
            if data.get_attribute("id") == Some("body") { body_node_id = Some(id); }
        }
    }
    
    let target_node_id = target_node_id.unwrap();
    let body_node_id = body_node_id.unwrap();
    
    let body_element = DomElement { tree: &dom, id: body_node_id };
    let matched_body = SelectorEngine::match_rules(&body_element, &rules);
    let body_style = SelectorEngine::compute_style(&body_element, &matched_body, &ComputedStyle::default());
    
    let target_element = DomElement { tree: &dom, id: target_node_id };
    let matched_target = SelectorEngine::match_rules(&target_element, &rules);
    let target_style = SelectorEngine::compute_style(&target_element, &matched_target, &body_style);
    
    assert_eq!(target_style.color.r, 0x12);
    assert_eq!(target_style.color.g, 0x34);
    assert_eq!(target_style.color.b, 0x56);
}

#[test]
fn test_no_inherit_margin() {
    let css = "body { margin-top: 50px; }";
    let html = "<body id='body'><span id='target'></span></body>";
    
    let dom = parse_html(html).unwrap();
    let rules = SelectorEngine::parse_stylesheet(css);
    
    let mut target_node_id = None;
    let mut body_node_id = None;
    for node in dom.nodes() {
        let id = node.id;
        if let kitsune_html::dom::NodeType::Element(ref data) = node.node_type {
            if data.get_attribute("id") == Some("target") { target_node_id = Some(id); }
            if data.get_attribute("id") == Some("body") { body_node_id = Some(id); }
        }
    }
    
    let target_node_id = target_node_id.unwrap();
    let body_node_id = body_node_id.unwrap();
    
    let body_element = DomElement { tree: &dom, id: body_node_id };
    let matched_body = SelectorEngine::match_rules(&body_element, &rules);
    let body_style = SelectorEngine::compute_style(&body_element, &matched_body, &ComputedStyle::default());
    
    let target_element = DomElement { tree: &dom, id: target_node_id };
    let matched_target = SelectorEngine::match_rules(&target_element, &rules);
    let target_style = SelectorEngine::compute_style(&target_element, &matched_target, &body_style);
    
    assert_eq!(body_style.margin.top, 50.0);
    assert_eq!(target_style.margin.top, 0.0); // Does not inherit
}

#[test]
fn test_universal_selector() {
    assert_selector(
        "<div id='target'></div>",
        "* { margin-bottom: 25px; }",
        "target",
        |style| assert_eq!(style.margin.bottom, 25.0)
    );
}

#[test]
fn test_pseudo_class_ignored_gracefully() {
    assert_selector(
        "<a id='target' href='#'></a>",
        "a:hover { color: red; } a { color: blue; }",
        "target",
        |style| assert_eq!(style.color.b, 255)
    );
}

#[test]
fn test_attribute_selector_exact() {
    assert_selector(
        "<input id='target' type='text'>",
        "input[type=\"text\"] { font-weight: 700; }",
        "target",
        |style| assert_eq!(style.font_weight, 700)
    );
}

#[test]
fn test_attribute_selector_miss() {
    assert_selector(
        "<input id='target' type='radio'>",
        "input[type=\"text\"] { font-weight: 700; }",
        "target",
        |style| assert_eq!(style.font_weight, 400) // Default
    );
}

#[test]
fn test_complex_class_id_combo() {
    assert_selector(
        "<div id='target' class='container active'></div>",
        "div.container#target.active { display: flex; }",
        "target",
        |style| assert!(matches!(style.display, DisplayType::Flex))
    );
}
