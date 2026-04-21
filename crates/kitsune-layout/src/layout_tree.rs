/// Layout tree — the computed layout for a page.
use crate::box_model::BoxDimensions;
use crate::LayoutRect;
use serde::{Deserialize, Serialize};

/// Scroll state for a scrollable container.
#[derive(Debug, Clone)]
pub struct ScrollState {
    pub offset_x: f64,
    pub offset_y: f64,
    pub content_width: f64,
    pub content_height: f64,
    pub viewport_width: f64,
    pub viewport_height: f64,
}

impl ScrollState {
    pub fn new(content_width: f64, content_height: f64, viewport_width: f64, viewport_height: f64) -> Self {
        Self {
            offset_x: 0.0,
            offset_y: 0.0,
            content_width,
            content_height,
            viewport_width,
            viewport_height,
        }
    }

    pub fn handle_scroll(&mut self, delta_x: f64, delta_y: f64) {
        self.offset_x = (self.offset_x + delta_x).clamp(0.0, (self.content_width - self.viewport_width).max(0.0));
        self.offset_y = (self.offset_y + delta_y).clamp(0.0, (self.content_height - self.viewport_height).max(0.0));
    }
}

/// A node in the layout tree.
#[derive(Debug, Clone)]
pub struct LayoutNode {
    /// The DOM node this layout node corresponds to.
    pub dom_node_id: u64,
    /// Computed box dimensions.
    pub dimensions: BoxDimensions,
    /// Child layout nodes.
    pub children: Vec<LayoutNode>,
    /// Tag name (e.g. "div", "img", "#text").
    pub tag: String,
    /// Element attributes (for <img> src etc).
    pub attributes: std::collections::HashMap<String, String>,
    /// Layout type.
    pub layout_type: LayoutType,
    /// Text content for text nodes.
    pub text: Option<String>,
    /// The computed style (for painting).
    pub style: kitsune_css::ComputedStyle,
    /// Has this node or its children changed?
    pub dirty: bool,
    /// Scroll state (for overflow: scroll/auto containers).
    pub scroll: Option<ScrollState>,
    /// Z-index for stacking context (computed).
    pub z_index: i32,
    /// Absolute position (computed after layout).
    pub absolute_x: Option<f64>,
    pub absolute_y: Option<f64>,
    /// Whether this node is a positioning container (position != static or overflow != visible).
    pub is_containing_block: bool,
}

impl LayoutNode {
    /// Mark node as dirty if style or text changes. Recursively marks ancestors.
    pub fn update_incremental(old: &mut LayoutNode, new: &kitsune_css::style_engine::StyledNode) -> bool {
        let mut is_dirty = false;
        if old.style != new.style || old.text != new.text || old.tag != new.tag {
            old.dirty = true;
            is_dirty = true;
            old.style = new.style.clone();
            old.text = new.text.clone();
            old.tag = new.tag.clone();
        }

        for (i, old_child) in old.children.iter_mut().enumerate() {
            if i < new.children.len() {
                if Self::update_incremental(old_child, &new.children[i]) {
                    old.dirty = true;
                    is_dirty = true;
                }
            }
        }
        is_dirty
    }

    /// Recursively search for `dom_node_id`. If found, mark it and all its ancestors dirty.
    pub fn mark_dirty(&mut self, dom_node_id: u64) -> bool {
        if self.dom_node_id == dom_node_id {
            self.dirty = true;
            return true;
        }
        let mut found = false;
        for child in &mut self.children {
            if child.mark_dirty(dom_node_id) {
                found = true;
            }
        }
        if found {
            self.dirty = true;
        }
        found
    }

    /// Recursively mark this node and all descendants as clean.
    pub fn mark_clean(&mut self) {
        self.dirty = false;
        for child in &mut self.children {
            child.mark_clean();
        }
    }

    /// Find a node by its DOM node ID.
    pub fn find_by_dom_id(&self, dom_node_id: u64) -> Option<&LayoutNode> {
        if self.dom_node_id == dom_node_id {
            return Some(self);
        }
        for child in &self.children {
            if let Some(found) = child.find_by_dom_id(dom_node_id) {
                return Some(found);
            }
        }
        None
    }

    /// Find a node by its DOM node ID (mutable).
    pub fn find_by_dom_id_mut(&mut self, dom_node_id: u64) -> Option<&mut LayoutNode> {
        if self.dom_node_id == dom_node_id {
            return Some(self);
        }
        for child in &mut self.children {
            if let Some(found) = child.find_by_dom_id_mut(dom_node_id) {
                return Some(found);
            }
        }
        None
    }


    /// Handle scroll event on a specific node.
    pub fn handle_scroll(&mut self, dom_node_id: u64, delta_x: f64, delta_y: f64) -> bool {
        if let Some(node) = self.find_by_dom_id_mut(dom_node_id) {
            if let Some(ref mut scroll) = node.scroll {
                scroll.handle_scroll(delta_x, delta_y);
                node.dirty = true;
                return true;
            }
        }
        false
    }


    /// Compute absolute positions for all nodes in the tree.
    /// This should be called after the initial flex/grid layout pass.
    pub fn resolve_absolute_positions(&mut self, viewport: LayoutRect) {
        // The root's absolute position is its relative position (from taffy), which should be (0,0).
        // The initial containing block for the root is the viewport.
        self.resolve_absolute_positions_impl(self.dimensions.content.x, self.dimensions.content.y, viewport, viewport);
    }

    /// Recursive helper for absolute positioning.
    /// `parent_abs_x/y`: The absolute coordinates of the parent node's content area.
    /// `nearest_cb_rect`: The absolute layout rect of the nearest containing block.
    fn resolve_absolute_positions_impl(
        &mut self,
        parent_abs_x: f64,
        parent_abs_y: f64,
        nearest_cb_rect: LayoutRect,
        viewport: LayoutRect
    ) {
        use kitsune_css::{CssValue, PositionType};

        let is_positioned = self.style.position == PositionType::Absolute || self.style.position == PositionType::Fixed;
        let is_cb = self.style.position != PositionType::Static;

        if is_positioned {
            let containing_rect = if self.style.position == PositionType::Fixed {
                viewport
            } else {
                nearest_cb_rect
            };

            let mut abs_x = containing_rect.x;
            let mut abs_y = containing_rect.y;

            if let Some(CssValue::Length(val, _)) = self.style.inset_top { abs_y += val; }
            if let Some(CssValue::Length(val, _)) = self.style.inset_left { abs_x += val; }

            self.absolute_x = Some(abs_x);
            self.absolute_y = Some(abs_y);

        } else {
            self.absolute_x = Some(parent_abs_x + self.dimensions.content.x);
            self.absolute_y = Some(parent_abs_y + self.dimensions.content.y);
        }

        let my_abs_x = self.absolute_x.unwrap();
        let my_abs_y = self.absolute_y.unwrap();

        let child_cb_rect = if is_cb {
             LayoutRect::new(
                my_abs_x,
                my_abs_y,
                self.dimensions.content.width,
                self.dimensions.content.height,
            )
        } else {
            nearest_cb_rect
        };

        for child in &mut self.children {
            child.resolve_absolute_positions_impl(my_abs_x, my_abs_y, child_cb_rect, viewport);
        }
    }

    /// Update scroll state after layout.
    pub fn compute_scroll_state(&mut self) {
        use kitsune_css::{Overflow, PositionType};

        for child in &mut self.children {
            child.compute_scroll_state();
        }

        let is_scrollable = self.style.overflow == Overflow::Scroll || self.style.overflow == Overflow::Auto;

        if is_scrollable {
            let mut content_width = 0.0;
            let mut content_height = 0.0;

            for child in &self.children {
                // Absolutely positioned children do not contribute to the scrollable area of their parent.
                if child.style.position == PositionType::Absolute || child.style.position == PositionType::Fixed {
                    continue;
                }

                let child_x = child.dimensions.content.x;
                let child_y = child.dimensions.content.y;
                let child_right = child_x + child.dimensions.content.width;
                let child_bottom = child_y + child.dimensions.content.height;

                if child_right > content_width { content_width = child_right; }
                if child_bottom > content_height { content_height = child_bottom; }
            }

            let viewport_width = self.dimensions.content.width;
            let viewport_height = self.dimensions.content.height;

            if content_width > viewport_width || content_height > viewport_height {
                if let Some(scroll) = self.scroll.as_mut() {
                    scroll.content_width = content_width;
                    scroll.content_height = content_height;
                    scroll.viewport_width = viewport_width;
                    scroll.viewport_height = viewport_height;
                } else {
                    self.scroll = Some(ScrollState::new(content_width, content_height, viewport_width, viewport_height));
                }
            } else {
                self.scroll = None;
            }
        }
    }
}

/// How this node is laid out.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum LayoutType {
    Block,
    Inline,
    Flex,
    Grid,
    None,
}
