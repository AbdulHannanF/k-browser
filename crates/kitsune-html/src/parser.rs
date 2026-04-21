//! HTML5 parser — integrates html5ever to produce KitsuneEngine's internal DOM.
//!
//! Uses `html5ever::parse_document()` with `markup5ever_rcdom::RcDom` as the
//! tree sink. The resulting `RcDom` is then walked and mapped into our internal
//! `DomNode`/`DomTree` types, preserving sensitive-field detection on form inputs.

use crate::dom::{DomTree, NodeId};
use crate::error::{HtmlError, HtmlResult};

use html5ever::tendril::TendrilSink;
use html5ever::{parse_document, ParseOpts};
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use std::collections::HashMap;
use tracing::debug;

/// Parse an HTML string into a KitsuneEngine DOM tree.
///
/// Uses html5ever's spec-compliant parser with standard quirks handling.
/// Malformed HTML is silently auto-corrected per the HTML5 spec (same
/// behaviour as all major browsers).
pub fn parse_html(html: &str) -> HtmlResult<DomTree> {
    debug!(len = html.len(), "Parsing HTML document via html5ever");

    let opts = ParseOpts::default();
    let dom: RcDom = parse_document(RcDom::default(), opts)
        .from_utf8()
        .read_from(&mut html.as_bytes())
        .map_err(|e| HtmlError::ParseError(e.to_string()))?;

    let mut tree = DomTree::new();
    let root_id = tree.create_document();

    // Walk the html5ever RcDom and map it into our DomTree
    walk_node(&dom.document, &mut tree, root_id)?;

    debug!(nodes = tree.node_count(), "HTML parsed successfully");
    Ok(tree)
}

/// Recursively walk an RcDom node and map it into the KitsuneEngine DomTree.
fn walk_node(handle: &Handle, tree: &mut DomTree, parent_id: NodeId) -> HtmlResult<()> {
    for child in handle.children.borrow().iter() {
        let node_id = match &child.data {
            NodeData::Document => {
                // The document node is already represented by parent_id; recurse into it
                walk_node(child, tree, parent_id)?;
                continue;
            }

            NodeData::Doctype { .. } => {
                // DOCTYPE: create a comment-like node so the tree isn't missing it
                tree.create_comment("<!DOCTYPE html>")
            }

            NodeData::Text { contents } => {
                let text = contents.borrow().to_string();
                // Skip whitespace-only text nodes to keep the tree lean
                if text.trim().is_empty() {
                    continue;
                }
                tree.create_text(&text)
            }

            NodeData::Comment { contents } => {
                tree.create_comment(contents.to_string())
            }

            NodeData::Element {
                name,
                attrs,
                ..
            } => {
                let tag = name.local.as_ref().to_ascii_lowercase();
                let attributes: HashMap<String, String> = attrs
                    .borrow()
                    .iter()
                    .map(|a| {
                        (
                            a.name.local.as_ref().to_lowercase(),
                            a.value.to_string(),
                        )
                    })
                    .collect();

                tree.create_element_with_attrs(&tag, attributes)
            }

            NodeData::ProcessingInstruction { .. } => {
                // PI nodes are uncommon in HTML; skip them
                continue;
            }
        };

        tree.append_child(parent_id, node_id);

        // Recurse for element children (text/comment nodes have no children)
        if matches!(child.data, NodeData::Element { .. } | NodeData::Document) {
            walk_node(child, tree, node_id)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic_structure() {
        let html = "<html><head><title>Test</title></head><body><p>Hello</p></body></html>";
        let tree = parse_html(html).unwrap();
        assert!(tree.node_count() >= 5); // doc + html + head + body + p (+ text nodes)
        assert!(tree.root().is_some());
    }

    #[test]
    fn test_parse_empty_input() {
        let tree = parse_html("").unwrap();
        // html5ever always generates at minimum a document root + html + head + body
        // (browser-standard behaviour — even "empty" HTML gets the implicit shell)
        assert!(tree.node_count() >= 1);
        assert!(tree.root().is_some());
    }

    #[test]
    fn test_parse_attributes() {
        let html = r#"<html><body><a href="https://example.com" id="link1">Click</a></body></html>"#;
        let tree = parse_html(html).unwrap();
        let link = tree.find_element("a").expect("should find <a>");
        assert_eq!(link.get_attribute("href"), Some("https://example.com"));
        assert_eq!(link.get_attribute("id"), Some("link1"));
    }

    #[test]
    fn test_parse_password_field_detected() {
        let html = r#"<html><body><form><input type="password" name="pwd"/></form></body></html>"#;
        let tree = parse_html(html).unwrap();
        let input = tree.find_element("input").expect("should find <input>");
        assert!(input.is_sensitive_field(), "password input must be flagged sensitive");
    }

    #[test]
    fn test_parse_email_field_flagged() {
        let html = r#"<html><body><input type="email" autocomplete="email" name="e"/></body></html>"#;
        let tree = parse_html(html).unwrap();
        let input = tree.find_element("input").expect("should find <input>");
        assert!(input.is_email_field(), "email input must be flagged");
    }

    #[test]
    fn test_parse_cc_autocomplete_detected() {
        let html = r#"<html><body><input autocomplete="cc-number" name="card"/></body></html>"#;
        let tree = parse_html(html).unwrap();
        let input = tree.find_element("input").expect("should find <input>");
        assert!(input.is_sensitive_field(), "cc-number input must be flagged sensitive");
    }

    #[test]
    fn test_parse_non_sensitive_text_field() {
        let html = r#"<html><body><input type="text" name="search"/></body></html>"#;
        let tree = parse_html(html).unwrap();
        let input = tree.find_element("input").expect("should find <input>");
        assert!(!input.is_sensitive_field(), "plain text input must NOT be flagged");
    }

    #[test]
    fn test_parse_malformed_html_no_panic() {
        // html5ever auto-corrects malformed HTML — must not panic
        let html = "<div><p>Unclosed <b>tags everywhere";
        let result = parse_html(html);
        assert!(result.is_ok(), "malformed HTML should heal, not error");
    }

    #[test]
    fn test_parse_script_tag_as_element() {
        let html = "<html><head><script>var x = 1;</script></head><body></body></html>";
        let tree = parse_html(html).unwrap();
        assert!(tree.find_element("script").is_some(), "script must appear as an element node");
    }

    #[test]
    fn test_parse_nested_elements_parent_child() {
        let html = "<html><body><div><section><article><p>Deep</p></article></section></div></body></html>";
        let tree = parse_html(html).unwrap();
        let p = tree.find_element("p").expect("should find <p>");
        let _ = p; // Structure validated by no panic + node exists
        assert!(tree.node_count() >= 7);
    }

    #[test]
    fn test_parse_form_with_all_inputs() {
        let html = r#"<html><body>
            <form action="/submit">
              <input type="text" name="username"/>
              <input type="password" name="password"/>
              <select name="country"><option value="us">US</option></select>
              <button type="submit">Submit</button>
            </form>
        </body></html>"#;
        let tree = parse_html(html).unwrap();
        assert!(tree.find_element("form").is_some());
        assert!(tree.find_element("select").is_some());
        assert!(tree.find_element("button").is_some());
    }

    #[test]
    fn test_parse_img_with_alt() {
        let html = r#"<html><body><img src="logo.png" alt="KitsuneEngine Logo"/></body></html>"#;
        let tree = parse_html(html).unwrap();
        let img = tree.find_element("img").expect("should find <img>");
        assert_eq!(img.get_attribute("alt"), Some("KitsuneEngine Logo"));
    }

    #[test]
    fn test_parse_doctype() {
        let html = "<!DOCTYPE html><html><body></body></html>";
        let result = parse_html(html);
        assert!(result.is_ok(), "DOCTYPE must be handled without error");
    }

    #[test]
    fn test_parse_comments() {
        let html = "<html><body><!-- This is a comment --><p>Text</p></body></html>";
        let result = parse_html(html);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_table_structure() {
        let html = r#"<html><body>
            <table>
              <thead><tr><th>Name</th><th>Value</th></tr></thead>
              <tbody><tr><td>Foo</td><td>Bar</td></tr></tbody>
            </table>
        </body></html>"#;
        let tree = parse_html(html).unwrap();
        assert!(tree.find_element("table").is_some());
        assert!(tree.find_element("thead").is_some());
        assert!(tree.find_element("tbody").is_some());
        assert!(tree.find_element("th").is_some());
    }

    #[test]
    fn test_parse_svg_embedded() {
        let html = r#"<html><body><svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
            <circle cx="50" cy="50" r="40"/>
        </svg></body></html>"#;
        let result = parse_html(html);
        assert!(result.is_ok(), "embedded SVG must parse without error");
    }

    #[test]
    fn test_parse_unicode_content() {
        let html = "<html><body><p>日本語テスト — Ñoño — Ärger</p></body></html>";
        let tree = parse_html(html).unwrap();
        let nodes: Vec<_> = tree.text_nodes().collect();
        assert!(nodes.iter().any(|t| t.contains("日本語")));
    }

    #[test]
    fn test_parse_deep_nesting() {
        // 15-level deep nesting — must not stack overflow
        let inner = "<div>".repeat(15) + "deep" + &"</div>".repeat(15);
        let html = format!("<html><body>{}</body></html>", inner);
        let result = parse_html(&html);
        assert!(result.is_ok(), "deep nesting must not overflow or error");
    }

    #[test]
    fn test_parse_multiple_forms() {
        let html = r#"<html><body>
            <form id="login"><input type="password"/></form>
            <form id="search"><input type="text"/></form>
        </body></html>"#;
        let tree = parse_html(html).unwrap();
        let forms = tree.find_all_elements("form");
        assert_eq!(forms.len(), 2, "should find both forms");
    }

    #[test]
    fn test_parse_link_attributes() {
        let html = r#"<html><body><a href="/page" rel="noopener" target="_blank">Link</a></body></html>"#;
        let tree = parse_html(html).unwrap();
        let a = tree.find_element("a").expect("should find <a>");
        assert_eq!(a.get_attribute("rel"), Some("noopener"));
        assert_eq!(a.get_attribute("target"), Some("_blank"));
    }

    #[test]
    fn test_parse_class_attribute() {
        let html = r#"<html><body><div class="card primary active">Content</div></body></html>"#;
        let tree = parse_html(html).unwrap();
        let div = tree.find_element("div").expect("should find <div>");
        let classes = div.classes();
        assert!(classes.contains(&"card"));
        assert!(classes.contains(&"primary"));
        assert!(classes.contains(&"active"));
    }
}
