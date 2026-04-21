//! Layout engine — converts a StyledTree into a tree of positioned LayoutNodes.
//!
//! Routes to the appropriate layout algorithm based on the root display type:
//! - `display: grid` → GridEngine (Taffy CSS Grid)
//! - everything else → FlexEngine (Taffy flexbox / block)

use crate::layout_tree::LayoutNode;
use kitsune_css::style_engine::StyledTree;
use kitsune_css::DisplayType;

/// Viewport dimensions.
#[derive(Debug, Clone, Copy)]
pub struct Viewport {
    pub width: f64,
    pub height: f64,
}

impl Viewport {
    pub fn new(width: f64, height: f64) -> Self {
        Self { width, height }
    }
}

/// The layout engine.
pub struct LayoutEngine;

impl LayoutEngine {
    /// Lay out a styled tree within the given viewport.
    ///
    /// Detects `display: grid` on the root styled node and delegates to
    /// [`GridEngine`] when appropriate; otherwise falls back to [`FlexEngine`].
    pub fn layout(styled: &StyledTree, viewport: Viewport) -> LayoutNode {
        let _span = tracing::info_span!("pipeline::layout").entered();
        let mut root = if styled.root.style.display == DisplayType::Grid {
            crate::grid::GridEngine::layout(&styled.root, viewport)
        } else {
            crate::flex::FlexEngine::layout(&styled.root, viewport)
        };

        // After main layout, resolve absolute positions and scroll containers
        root.resolve_absolute_positions(crate::LayoutRect::new(0.0, 0.0, viewport.width, viewport.height));
        root.compute_scroll_state();

        root.mark_clean();
        root
    }

    /// Perform an incremental layout, skipping subtrees where `dirty == false`.
    pub fn layout_incremental(root: &mut LayoutNode, viewport: Viewport) {
        let _span = tracing::info_span!("pipeline::layout_inc").entered();
        if root.style.display == DisplayType::Grid {
            crate::grid::GridEngine::layout_incremental(root, viewport);
        } else {
            crate::flex::FlexEngine::layout_incremental(root, viewport);
        }

        // After incremental layout, re-resolve absolute positions and scroll containers
        root.resolve_absolute_positions(crate::LayoutRect::new(0.0, 0.0, viewport.width, viewport.height));
        root.compute_scroll_state();

        root.mark_clean();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layout_simple_page() {
        let html = "<html><body><h1>Title</h1><p>Paragraph text here.</p></body></html>";
        let dom = kitsune_html::parser::parse_html(html).unwrap();
        let mut style_engine = kitsune_css::style_engine::StyleEngine::new();
        let styled = style_engine.compute_styles(&dom, vec![]);
        let layout = LayoutEngine::layout(&styled, Viewport::new(1280.0, 800.0));

        // Root should have children
        assert!(!layout.children.is_empty());
    }

    #[test]
    fn test_text_node_gets_dimensions() {
        let html = "<html><body><p>Hello World</p></body></html>";
        let dom = kitsune_html::parser::parse_html(html).unwrap();
        let mut style_engine = kitsune_css::style_engine::StyleEngine::new();
        let styled = style_engine.compute_styles(&dom, vec![]);
        let layout = LayoutEngine::layout(&styled, Viewport::new(800.0, 600.0));

        // Find a text node with non-zero dimensions
        fn find_text(node: &LayoutNode) -> bool {
            if node.text.is_some() && node.dimensions.content.height > 0.0 {
                return true;
            }
            node.children.iter().any(find_text)
        }
        assert!(find_text(&layout), "Should have at least one text node with dimensions");
    }

    #[test]
    fn test_mark_dirty_propagates_to_root() {
        let mut root = LayoutNode {
            dom_node_id: 1,
            dimensions: Default::default(),
            layout_type: crate::layout_tree::LayoutType::Block,
            tag: "div".to_string(),
            attributes: std::collections::HashMap::new(),
            text: None,
            style: Default::default(),
            dirty: false,
            children: vec![
                LayoutNode {
                    dom_node_id: 2,
                    dimensions: Default::default(),
                    layout_type: crate::layout_tree::LayoutType::Block,
                    tag: "div".to_string(),
                    attributes: std::collections::HashMap::new(),
                    text: None,
                    style: Default::default(),
                    dirty: false,
                    children: vec![
                        LayoutNode {
                            dom_node_id: 3,
                            dimensions: Default::default(),
                            layout_type: crate::layout_tree::LayoutType::Block,
                            tag: "div".to_string(),
                            attributes: std::collections::HashMap::new(),
                            text: None,
                            style: Default::default(),
                            dirty: false,
                            children: vec![],
                            scroll: None,
                            z_index: 0,
                            absolute_x: None,
                            absolute_y: None,
                            is_containing_block: false,
                        }
                    ],
                    scroll: None,
                    z_index: 0,
                    absolute_x: None,
                    absolute_y: None,
                    is_containing_block: false,
                }
            ],
            scroll: None,
            z_index: 0,
            absolute_x: None,
            absolute_y: None,
            is_containing_block: false,
        };

        // Mark node 3 as dirty
        let found = root.mark_dirty(3);
        assert!(found);

        // All ancestors should be dirty
        assert!(root.dirty);
        assert!(root.children[0].dirty);
        assert!(root.children[0].children[0].dirty);
    }

    #[test]
    fn test_mark_clean_clears_all() {
        let mut root = LayoutNode {
            dom_node_id: 1,
            dimensions: Default::default(),
            layout_type: crate::layout_tree::LayoutType::Block,
            tag: "div".to_string(),
            attributes: std::collections::HashMap::new(),
            text: None,
            style: Default::default(),
            dirty: true,
            children: vec![
                LayoutNode {
                    dom_node_id: 2,
                    dimensions: Default::default(),
                    layout_type: crate::layout_tree::LayoutType::Block,
                    tag: "div".to_string(),
                    attributes: std::collections::HashMap::new(),
                    text: None,
                    style: Default::default(),
                    dirty: true,
                    children: vec![],
                    scroll: None,
                    z_index: 0,
                    absolute_x: None,
                    absolute_y: None,
                    is_containing_block: false,
                }
            ],
            scroll: None,
            z_index: 0,
            absolute_x: None,
            absolute_y: None,
            is_containing_block: false,
        };

        root.mark_clean();

        assert!(!root.dirty);
        assert!(!root.children[0].dirty);
    }

    #[test]
    fn test_layout_incremental_clears_dirty() {
        let html = "<html><body><p>Hello</p></body></html>";
        let dom = kitsune_html::parser::parse_html(html).unwrap();
        let mut style_engine = kitsune_css::style_engine::StyleEngine::new();
        let styled = style_engine.compute_styles(&dom, vec![]);
        
        // Initial layout creates the tree and implicitly calls mark_clean()
        let mut layout = LayoutEngine::layout(&styled, Viewport::new(800.0, 600.0));
        assert!(!layout.dirty, "Should be clean after first layout");

        // Force a dirty state
        layout.mark_dirty(dom.root().unwrap().id.0);
        assert!(layout.dirty);

        // Re-layout incrementally
        LayoutEngine::layout_incremental(&mut layout, Viewport::new(800.0, 600.0));
        assert!(!layout.dirty, "Incremental layout should leave tree clean");
    }
}
