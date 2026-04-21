//! DOM tree representation for KitsuneEngine.
//!
//! Internal DOM used by the layout, rendering, and agent engines.
//! Deliberately simpler than the full W3C DOM spec — we implement
//! only what is needed for layout and agent interaction.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A unique identifier for a DOM node.
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct NodeId(pub u64);

/// A DOM node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomNode {
    /// Unique identifier for this node.
    pub id: NodeId,
    /// The type of node.
    pub node_type: NodeType,
    /// Child node IDs.
    pub children: Vec<NodeId>,
    /// Parent node ID.
    pub parent: Option<NodeId>,
}

impl DomNode {
    /// Get a mutable reference to the element data if this is an element node.
    pub fn as_element_mut(&mut self) -> Option<&mut ElementData> {
        match &mut self.node_type {
            NodeType::Element(data) => Some(data),
            _ => None,
        }
    }
}

/// Types of DOM nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeType {
    /// The document root.
    Document,
    /// An element node (e.g., `<div>`, `<p>`, `<input>`).
    Element(ElementData),
    /// A text node.
    Text(String),
    /// A comment node.
    Comment(String),
}

/// Data for an element node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElementData {
    /// Tag name (lowercased per HTML5 spec).
    pub tag_name: String,
    /// Element attributes (name lowercased, value as-is).
    pub attributes: HashMap<String, String>,
}

impl ElementData {
    /// Create a new element with the given tag name.
    pub fn new(tag_name: impl Into<String>) -> Self {
        Self {
            tag_name: tag_name.into().to_lowercase(),
            attributes: HashMap::new(),
        }
    }

    /// Create a new element with pre-parsed attributes.
    pub fn with_attributes(tag_name: impl Into<String>, attributes: HashMap<String, String>) -> Self {
        Self {
            tag_name: tag_name.into().to_lowercase(),
            attributes,
        }
    }

    /// Get an attribute value by name.
    pub fn get_attribute(&self, name: &str) -> Option<&str> {
        self.attributes.get(name).map(|s| s.as_str())
    }

    /// Set an attribute.
    pub fn set_attribute(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.attributes.insert(name.into(), value.into());
    }

    /// Get the element's `id` attribute.
    pub fn id(&self) -> Option<&str> {
        self.get_attribute("id")
    }

    /// Get CSS class tokens from the `class` attribute.
    pub fn classes(&self) -> Vec<&str> {
        self.get_attribute("class")
            .map(|c| c.split_whitespace().collect())
            .unwrap_or_default()
    }

    /// Return `true` if this element is any form-related element.
    pub fn is_form_element(&self) -> bool {
        matches!(
            self.tag_name.as_str(),
            "input" | "textarea" | "select" | "button" | "form"
        )
    }

    /// Return `true` if this is a field that contains sensitive credentials.
    ///
    /// Matches:
    /// - `<input type="password">` — passwords
    /// - `<input type="hidden">` — hidden tokens
    /// - `<input autocomplete="cc-number|cc-csc|cc-exp*">` — payment card data
    ///
    /// **INVARIANT**: This is a security-critical check — see vault disclosure
    /// policies for how sensitive fields are treated during form autofill.
    pub fn is_sensitive_field(&self) -> bool {
        if self.tag_name != "input" {
            return false;
        }
        let input_type = self.get_attribute("type").unwrap_or("text");
        if matches!(input_type, "password" | "hidden") {
            return true;
        }
        self.get_attribute("autocomplete")
            .map(|ac| {
                ac.contains("password")
                    || ac.contains("cc-number")
                    || ac.contains("cc-csc")
                    || ac.contains("cc-exp")
            })
            .unwrap_or(false)
    }

    /// Return `true` if this is an email input field.
    pub fn is_email_field(&self) -> bool {
        if self.tag_name != "input" {
            return false;
        }
        let input_type = self.get_attribute("type").unwrap_or("text");
        if input_type == "email" {
            return true;
        }
        self.get_attribute("autocomplete")
            .map(|ac| ac == "email" || ac.contains("email"))
            .unwrap_or(false)
    }
}

/// The DOM tree — owns all nodes and provides traversal methods.
#[derive(Debug)]
pub struct DomTree {
    /// All nodes in the tree (densely packed — NodeId indexes into this vec).
    nodes: Vec<DomNode>,
    /// The root document node ID.
    root: Option<NodeId>,
    /// Next available node ID counter.
    next_id: u64,
}

impl DomTree {
    /// Create a new, empty DOM tree.
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            root: None,
            next_id: 0,
        }
    }

    /// Create the document root node. Should be called exactly once.
    pub fn create_document(&mut self) -> NodeId {
        let id = self.allocate_id();
        self.nodes.push(DomNode {
            id,
            node_type: NodeType::Document,
            children: Vec::new(),
            parent: None,
        });
        self.root = Some(id);
        id
    }

    /// Create an element node with no attributes.
    pub fn create_element(&mut self, tag_name: impl Into<String>) -> NodeId {
        let id = self.allocate_id();
        self.nodes.push(DomNode {
            id,
            node_type: NodeType::Element(ElementData::new(tag_name)),
            children: Vec::new(),
            parent: None,
        });
        id
    }

    /// Create an element node with pre-parsed attributes (used by the html5ever bridge).
    pub fn create_element_with_attrs(
        &mut self,
        tag_name: impl Into<String>,
        attributes: HashMap<String, String>,
    ) -> NodeId {
        let id = self.allocate_id();
        self.nodes.push(DomNode {
            id,
            node_type: NodeType::Element(ElementData::with_attributes(tag_name, attributes)),
            children: Vec::new(),
            parent: None,
        });
        id
    }

    /// Create a text node.
    pub fn create_text(&mut self, text: impl Into<String>) -> NodeId {
        let id = self.allocate_id();
        self.nodes.push(DomNode {
            id,
            node_type: NodeType::Text(text.into()),
            children: Vec::new(),
            parent: None,
        });
        id
    }

    /// Create a comment node.
    pub fn create_comment(&mut self, text: impl Into<String>) -> NodeId {
        let id = self.allocate_id();
        self.nodes.push(DomNode {
            id,
            node_type: NodeType::Comment(text.into()),
            children: Vec::new(),
            parent: None,
        });
        id
    }

    /// Append `child` as the last child of `parent`.
    pub fn append_child(&mut self, parent: NodeId, child: NodeId) {
        if let Some(child_node) = self.get_node_mut(child) {
            child_node.parent = Some(parent);
        }
        if let Some(parent_node) = self.get_node_mut(parent) {
            parent_node.children.push(child);
        }
    }

    /// Get a node by `NodeId` (immutable).
    pub fn get_node(&self, id: NodeId) -> Option<&DomNode> {
        self.nodes.get(id.0 as usize).filter(|n| n.id == id)
    }

    /// Get a node by `NodeId` (mutable).
    pub fn get_node_mut(&mut self, id: NodeId) -> Option<&mut DomNode> {
        self.nodes
            .get_mut(id.0 as usize)
            .filter(|n| n.id == id)
    }

    /// Get the root document node.
    pub fn root(&self) -> Option<&DomNode> {
        self.root.and_then(|id| self.get_node(id))
    }

    /// Get all nodes.
    pub fn nodes(&self) -> &[DomNode] {
        &self.nodes
    }

    /// Total number of nodes in the tree.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Find the first element with the given tag name (BFS).
    ///
    /// Returns `None` if no matching element exists.
    pub fn find_element(&self, tag_name: &str) -> Option<&ElementData> {
        let tag = tag_name.to_lowercase();
        self.nodes.iter().find_map(|n| {
            if let NodeType::Element(ref e) = n.node_type {
                if e.tag_name == tag {
                    return Some(e);
                }
            }
            None
        })
    }

    /// Find **all** elements with the given tag name.
    pub fn find_all_elements(&self, tag_name: &str) -> Vec<&ElementData> {
        let tag = tag_name.to_lowercase();
        self.nodes
            .iter()
            .filter_map(|n| {
                if let NodeType::Element(ref e) = n.node_type {
                    if e.tag_name == tag {
                        return Some(e);
                    }
                }
                None
            })
            .collect()
    }

    /// Set the text content of a node.
    pub fn set_text_content(&mut self, node_id: NodeId, text: &str) {
        if let Some(node) = self.get_node_mut(node_id) {
            node.children.clear();
            let text_node_id = self.create_text(text);
            self.append_child(node_id, text_node_id);
        }
    }

    /// Set the inner HTML of a node.
    pub fn set_inner_html(&mut self, node_id: NodeId, html: &str) {
        if let Some(node) = self.get_node_mut(node_id) {
            node.children.clear();
            // This is a simplified implementation that doesn't parse the HTML.
            // A full implementation would require a circular dependency on the parser.
            let text_node_id = self.create_text(html);
            self.append_child(node_id, text_node_id);
        }
    }

    /// Find an element by its `id` attribute.
    pub fn get_element_by_id(&self, id: &str) -> Option<NodeId> {
        self.nodes.iter().find_map(|n| {
            if let NodeType::Element(ref e) = n.node_type {
                if e.id() == Some(id) {
                    return Some(n.id);
                }
            }
            None
        })
    }

    /// Iterate over all non-empty text node contents in document order.
    pub fn text_nodes(&self) -> impl Iterator<Item = &str> {
        self.nodes.iter().filter_map(|n| {
            if let NodeType::Text(ref t) = n.node_type {
                if !t.trim().is_empty() {
                    return Some(t.as_str());
                }
            }
            None
        })
    }

    /// Find the first `<input>` with `type="password"` (convenience for agents).
    pub fn find_password_fields(&self) -> Vec<&ElementData> {
        self.nodes
            .iter()
            .filter_map(|n| {
                if let NodeType::Element(ref e) = n.node_type {
                    if e.is_sensitive_field() {
                        return Some(e);
                    }
                }
                None
            })
            .collect()
    }

    fn allocate_id(&mut self) -> NodeId {
        let id = NodeId(self.next_id);
        self.next_id += 1;
        id
    }
}

impl Default for DomTree {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dom_tree_basic() {
        let mut tree = DomTree::new();
        let root = tree.create_document();
        let html = tree.create_element("html");
        let body = tree.create_element("body");
        let text = tree.create_text("Hello, KitsuneEngine!");

        tree.append_child(root, html);
        tree.append_child(html, body);
        tree.append_child(body, text);

        assert_eq!(tree.node_count(), 4);
        assert!(tree.root().is_some());
    }

    #[test]
    fn test_sensitive_field_detection() {
        let mut elem = ElementData::new("input");
        elem.set_attribute("type", "password");
        assert!(elem.is_sensitive_field());

        let mut elem2 = ElementData::new("input");
        elem2.set_attribute("type", "text");
        assert!(!elem2.is_sensitive_field());

        let mut elem3 = ElementData::new("input");
        elem3.set_attribute("autocomplete", "cc-number");
        assert!(elem3.is_sensitive_field());
    }

    #[test]
    fn test_email_field_detection() {
        let mut elem = ElementData::new("input");
        elem.set_attribute("type", "email");
        assert!(elem.is_email_field());

        let mut elem2 = ElementData::new("input");
        elem2.set_attribute("type", "text");
        assert!(!elem2.is_email_field());
    }

    #[test]
    fn test_domain_pattern_exact() {
        let mut attrs = HashMap::new();
        attrs.insert("id".to_string(), "main-form".to_string());
        let elem = ElementData::with_attributes("form", attrs);
        assert_eq!(elem.id(), Some("main-form"));
    }

    #[test]
    fn test_classes_parsed() {
        let mut elem = ElementData::new("div");
        elem.set_attribute("class", "card primary active");
        let classes = elem.classes();
        assert!(classes.contains(&"card"));
        assert!(classes.contains(&"primary"));
        assert!(classes.contains(&"active"));
    }

    #[test]
    fn test_find_element() {
        let mut tree = DomTree::new();
        let root = tree.create_document();
        let body = tree.create_element("body");
        let div = tree.create_element("div");
        tree.append_child(root, body);
        tree.append_child(body, div);

        assert!(tree.find_element("div").is_some());
        assert!(tree.find_element("span").is_none());
    }

    #[test]
    fn test_find_all_elements() {
        let mut tree = DomTree::new();
        let root = tree.create_document();
        let body = tree.create_element("body");
        let p1 = tree.create_element("p");
        let p2 = tree.create_element("p");
        tree.append_child(root, body);
        tree.append_child(body, p1);
        tree.append_child(body, p2);

        assert_eq!(tree.find_all_elements("p").len(), 2);
    }

    #[test]
    fn test_text_nodes_iterator() {
        let mut tree = DomTree::new();
        let root = tree.create_document();
        let body = tree.create_element("body");
        let t1 = tree.create_text("Hello");
        let t2 = tree.create_text("World");
        tree.append_child(root, body);
        tree.append_child(body, t1);
        tree.append_child(body, t2);

        let texts: Vec<_> = tree.text_nodes().collect();
        assert_eq!(texts.len(), 2);
        assert!(texts.contains(&"Hello"));
        assert!(texts.contains(&"World"));
    }

    #[test]
    fn test_find_password_fields() {
        let mut tree = DomTree::new();
        let root = tree.create_document();
        let form = tree.create_element("form");
        let pwd = {
            let mut attrs = HashMap::new();
            attrs.insert("type".to_string(), "password".to_string());
            tree.create_element_with_attrs("input", attrs)
        };
        let text = {
            let mut attrs = HashMap::new();
            attrs.insert("type".to_string(), "text".to_string());
            tree.create_element_with_attrs("input", attrs)
        };
        tree.append_child(root, form);
        tree.append_child(form, pwd);
        tree.append_child(form, text);

        let sensitive = tree.find_password_fields();
        assert_eq!(sensitive.len(), 1);
        assert_eq!(sensitive[0].get_attribute("type"), Some("password"));
    }
}
