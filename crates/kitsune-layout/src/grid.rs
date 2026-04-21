//! CSS Grid layout engine — maps kitsune-css GridStyle to Taffy grid layout.
//!
//! Converts a `StyledNode` with `display: grid` into a Taffy tree using the
//! CSS Grid algorithm, then reads back computed positions into `LayoutNode`.

use crate::box_model::BoxDimensions;
use crate::engine::Viewport;
use crate::layout_tree::{LayoutNode, LayoutType};
use crate::LayoutRect;
use kitsune_css::style_engine::StyledNode;
use kitsune_css::{CssUnit, CssValue, DisplayType, GridTrackDef, GridTrackSize};
use taffy::prelude::*;
use taffy::tree::NodeId as TaffyNodeId;

/// Grid layout engine backed by Taffy 0.5.
pub struct GridEngine;

impl GridEngine {
    /// Lay out a grid container and its children.
    pub fn layout(root: &StyledNode, viewport: Viewport) -> LayoutNode {
        let mut taffy: TaffyTree<crate::TextContext> = TaffyTree::new();

        let root_node_id = Self::build_taffy_tree(&mut taffy, root, viewport);

        let size = Size {
            width: AvailableSpace::Definite(viewport.width as f32),
            height: AvailableSpace::Definite(viewport.height as f32),
        };
        taffy
            .compute_layout_with_measure(
                root_node_id,
                size,
                |_known_dimensions, available_space, _node_id, context, _style| {
                    if let Some(ctx) = context {
                        if ctx.text.is_empty() {
                            return Size::ZERO;
                        }
                        let font_size = ctx.font_size as f64;
                        let line_height = 1.2;
                        let char_width = font_size * 0.6;
                        let avail_width = match available_space.width {
                            AvailableSpace::Definite(w) => w as f64,
                            _ => 800.0,
                        };
                        let chars_per_line = (avail_width / char_width).max(1.0);
                        let num_lines =
                            ((ctx.text.len() as f64) / chars_per_line).ceil().max(1.0);
                        let text_height = num_lines * font_size * line_height;
                        let text_width = if num_lines <= 1.0 {
                            ctx.text.len() as f64 * char_width
                        } else {
                            avail_width
                        };
                        Size {
                            width: text_width as f32,
                            height: text_height as f32,
                        }
                    } else {
                        Size::ZERO
                    }
                },
            )
            .unwrap();

        Self::build_layout_tree(&taffy, root_node_id, root)
    }

    /// Convert a kitsune-css `GridTrackDef` into a Taffy `TrackSizingFunction`.
    fn map_track_def(def: &GridTrackDef) -> TrackSizingFunction {
        match def {
            GridTrackDef::Single(size) => {
                TrackSizingFunction::Single(Self::map_track_size(size))
            }
            GridTrackDef::Repeat(count, size) => {
                let repetition = GridTrackRepetition::try_from(*count).unwrap();
                let track = Self::map_track_size(size);
                TrackSizingFunction::Repeat(repetition, vec![track])
            }
        }
    }

    /// Convert a kitsune-css `GridTrackSize` into a Taffy `NonRepeatedTrackSizingFunction`.
    fn map_track_size(size: &GridTrackSize) -> NonRepeatedTrackSizingFunction {
        match size {
            GridTrackSize::Px(px) => NonRepeatedTrackSizingFunction::from_length(*px),
            GridTrackSize::Fr(fr) => NonRepeatedTrackSizingFunction::from_flex(*fr),
            GridTrackSize::Auto => NonRepeatedTrackSizingFunction::AUTO,
        }
    }

    /// Recursively build the Taffy tree from a StyledNode.
    fn build_taffy_tree(
        taffy: &mut TaffyTree<crate::TextContext>,
        styled: &StyledNode,
        viewport: Viewport,
    ) -> TaffyNodeId {
        let mut style = Style::default();

        // Map display — the root of a grid subtree is Display::Grid
        style.display = match styled.style.display {
            DisplayType::None => taffy::style::Display::None,
            DisplayType::Flex => taffy::style::Display::Flex,
            DisplayType::Grid => taffy::style::Display::Grid,
            _ => taffy::style::Display::Block,
        };

        // Map position
        style.position = match styled.style.position {
            kitsune_css::PositionType::Absolute | kitsune_css::PositionType::Fixed => {
                taffy::style::Position::Absolute
            }
            _ => taffy::style::Position::Relative,
        };

        // Size handling
        let map_val = |val: &CssValue| -> Dimension {
            match val {
                CssValue::Length(v, CssUnit::Px) => Dimension::Length(*v as f32),
                CssValue::Percentage(v) => Dimension::Percent((*v / 100.0) as f32),
                CssValue::Length(v, CssUnit::Vw) => {
                    Dimension::Length((*v / 100.0 * viewport.width) as f32)
                }
                CssValue::Length(v, CssUnit::Vh) => {
                    Dimension::Length((*v / 100.0 * viewport.height) as f32)
                }
                _ => Dimension::Auto,
            }
        };

        if let Some(ref w) = styled.style.width {
            style.size.width = map_val(w);
        }
        if let Some(ref h) = styled.style.height {
            style.size.height = map_val(h);
        }

        style.margin = Rect {
            left: LengthPercentageAuto::Length(styled.style.margin.left as f32),
            right: LengthPercentageAuto::Length(styled.style.margin.right as f32),
            top: LengthPercentageAuto::Length(styled.style.margin.top as f32),
            bottom: LengthPercentageAuto::Length(styled.style.margin.bottom as f32),
        };

        style.padding = Rect {
            left: LengthPercentage::Length(styled.style.padding.left as f32),
            right: LengthPercentage::Length(styled.style.padding.right as f32),
            top: LengthPercentage::Length(styled.style.padding.top as f32),
            bottom: LengthPercentage::Length(styled.style.padding.bottom as f32),
        };

        Self::apply_grid_styles(styled, &mut style);

        let is_text = styled.text.is_some();

        if is_text {
            let text = styled.text.as_ref().unwrap().clone();
            let font_size = styled.style.font_size as f32;
            taffy.new_leaf_with_context(style, crate::TextContext { text, font_size }).unwrap()
        } else {
            let mut children = Vec::new();
            for child in &styled.children {
                children.push(Self::build_taffy_tree(taffy, child, viewport));
            }
            taffy.new_with_children(style, &children).unwrap()
        }
    }

    /// Extracted helper to apply grid-specific styling to the Taffy `Style` node.
    pub(crate) fn apply_grid_styles(styled: &StyledNode, style: &mut Style) {
        // Grid container properties
        let grid = &styled.style.grid;

        if !grid.template_columns.is_empty() {
            style.grid_template_columns = grid
                .template_columns
                .iter()
                .map(Self::map_track_def)
                .collect();
        }

        if !grid.template_rows.is_empty() {
            style.grid_template_rows = grid
                .template_rows
                .iter()
                .map(Self::map_track_def)
                .collect();
        }

        // Gap
        if grid.column_gap > 0.0 || grid.row_gap > 0.0 {
            style.gap = Size {
                width: LengthPercentage::Length(grid.column_gap),
                height: LengthPercentage::Length(grid.row_gap),
            };
        }

        // Grid child placement (grid-column / grid-row span)
        if let Some(ref col_placement) = styled.style.grid.column_placement {
            style.grid_column = Line {
                start: taffy::style::GridPlacement::Auto,
                end: taffy::style::GridPlacement::Span(col_placement.span),
            };
        }

        if let Some(ref row_placement) = styled.style.grid.row_placement {
            style.grid_row = Line {
                start: taffy::style::GridPlacement::Auto,
                end: taffy::style::GridPlacement::Span(row_placement.span),
            };
        }
    }

    /// Convert the Taffy layout results back into a LayoutNode tree.
    fn build_layout_tree(
        taffy: &TaffyTree<crate::TextContext>,
        node_id: TaffyNodeId,
        styled: &StyledNode,
    ) -> LayoutNode {
        let layout = taffy.layout(node_id).unwrap();

        let mut children = Vec::new();
        let child_ids = taffy.children(node_id).unwrap();

        for (i, child_styled) in styled.children.iter().enumerate() {
            if i < child_ids.len() {
                children.push(Self::build_layout_tree(taffy, child_ids[i], child_styled));
            }
        }

        let l_type = match styled.style.display {
            DisplayType::Block => LayoutType::Block,
            DisplayType::Flex | DisplayType::InlineFlex => LayoutType::Flex,
            DisplayType::Inline | DisplayType::InlineBlock => LayoutType::Inline,
            DisplayType::Grid => LayoutType::Grid,
            DisplayType::None => LayoutType::None,
        };

        LayoutNode {
            dom_node_id: styled.node_id,
            dimensions: BoxDimensions {
                content: LayoutRect {
                    x: layout.location.x as f64,
                    y: layout.location.y as f64,
                    width: layout.size.width as f64,
                    height: layout.size.height as f64,
                },
                padding: styled.style.padding,
                border: styled.style.border_width,
                margin: styled.style.margin,
            },
            children,
            layout_type: l_type,
            tag: styled.tag.clone(),
            attributes: styled.attributes.clone(),
            text: styled.text.clone(),
            style: styled.style.clone(),
            dirty: true,
            scroll: None,
            z_index: styled.style.z_index.unwrap_or(0),
            absolute_x: None,
            absolute_y: None,
            is_containing_block: false,
        }
    }

    pub fn layout_incremental(root: &mut LayoutNode, viewport: Viewport) {
        let mut taffy: TaffyTree<crate::TextContext> = TaffyTree::new();

        let root_node_id = Self::build_taffy_incremental(&mut taffy, root, viewport);

        let size = Size {
            width: AvailableSpace::Definite(viewport.width as f32),
            height: AvailableSpace::Definite(viewport.height as f32),
        };
        taffy.compute_layout_with_measure(root_node_id, size, |_known_dimensions, available_space, _node_id, context, _style| {
            if let Some(ctx) = context {
                if ctx.text.is_empty() { return Size::ZERO; }
                let font_size = ctx.font_size as f64;
                let line_height = 1.2;
                let char_width = font_size * 0.6;
                let avail_width = match available_space.width {
                    AvailableSpace::Definite(w) => w as f64,
                    _ => 800.0,
                };
                let chars_per_line = (avail_width / char_width).max(1.0);
                let num_lines = ((ctx.text.len() as f64) / chars_per_line).ceil().max(1.0);
                let text_height = num_lines * font_size * line_height;
                let text_width = if num_lines <= 1.0 {
                    ctx.text.len() as f64 * char_width
                } else {
                    avail_width
                };
                Size { width: text_width as f32, height: text_height as f32 }
            } else {
                Size::ZERO
            }
        }).unwrap();

        Self::apply_taffy_incremental(&taffy, root_node_id, root);
    }

    fn build_taffy_incremental(
        taffy: &mut TaffyTree<crate::TextContext>,
        layout_node: &LayoutNode,
        viewport: Viewport,
    ) -> TaffyNodeId {
        if !layout_node.dirty {
            let mut style = Style::default();
            style.size = Size {
                width: Dimension::Length(layout_node.dimensions.content.width as f32),
                height: Dimension::Length(layout_node.dimensions.content.height as f32),
            };
            return taffy.new_leaf(style).unwrap();
        }

        let mut style = Style::default();

        style.display = match layout_node.style.display {
            DisplayType::None => taffy::style::Display::None,
            DisplayType::Flex => taffy::style::Display::Flex,
            DisplayType::Grid => taffy::style::Display::Grid,
            _ => taffy::style::Display::Block,
        };

        style.position = match layout_node.style.position {
            kitsune_css::PositionType::Absolute | 
            kitsune_css::PositionType::Fixed => taffy::style::Position::Absolute,
            _ => taffy::style::Position::Relative,
        };

        let map_val = |val: &CssValue| -> Dimension {
            match val {
                CssValue::Length(v, CssUnit::Px) => Dimension::Length(*v as f32),
                CssValue::Percentage(v) => Dimension::Percent((*v / 100.0) as f32),
                CssValue::Length(v, CssUnit::Vw) => Dimension::Length((*v / 100.0 * viewport.width) as f32),
                CssValue::Length(v, CssUnit::Vh) => Dimension::Length((*v / 100.0 * viewport.height) as f32),
                _ => Dimension::Auto,
            }
        };

        if let Some(ref w) = layout_node.style.width { style.size.width = map_val(w); }
        if let Some(ref h) = layout_node.style.height { style.size.height = map_val(h); }

        style.margin = Rect {
            left: LengthPercentageAuto::Length(layout_node.style.margin.left as f32),
            right: LengthPercentageAuto::Length(layout_node.style.margin.right as f32),
            top: LengthPercentageAuto::Length(layout_node.style.margin.top as f32),
            bottom: LengthPercentageAuto::Length(layout_node.style.margin.bottom as f32),
        };

        style.padding = Rect {
            left: LengthPercentage::Length(layout_node.style.padding.left as f32),
            right: LengthPercentage::Length(layout_node.style.padding.right as f32),
            top: LengthPercentage::Length(layout_node.style.padding.top as f32),
            bottom: LengthPercentage::Length(layout_node.style.padding.bottom as f32),
        };

        Self::apply_grid_styles_layout(layout_node, &mut style);

        let is_text = layout_node.text.is_some();

        if is_text {
            let text = layout_node.text.as_ref().unwrap().clone();
            let font_size = layout_node.style.font_size as f32;
            taffy.new_leaf_with_context(style, crate::TextContext { text, font_size }).unwrap()
        } else {
            let mut children = Vec::new();
            for child in &layout_node.children {
                children.push(Self::build_taffy_incremental(taffy, child, viewport));
            }
            taffy.new_with_children(style, &children).unwrap()
        }
    }

    pub(crate) fn apply_grid_styles_layout(layout_node: &LayoutNode, style: &mut Style) {
        let grid = &layout_node.style.grid;

        if !grid.template_columns.is_empty() {
            style.grid_template_columns = grid
                .template_columns
                .iter()
                .map(Self::map_track_def)
                .collect();
        }

        if !grid.template_rows.is_empty() {
            style.grid_template_rows = grid
                .template_rows
                .iter()
                .map(Self::map_track_def)
                .collect();
        }

        if grid.column_gap > 0.0 || grid.row_gap > 0.0 {
            style.gap = Size {
                width: LengthPercentage::Length(grid.column_gap),
                height: LengthPercentage::Length(grid.row_gap),
            };
        }

        if let Some(ref col_placement) = layout_node.style.grid.column_placement {
            style.grid_column = Line {
                start: taffy::style::GridPlacement::Auto,
                end: taffy::style::GridPlacement::Span(col_placement.span),
            };
        }

        if let Some(ref row_placement) = layout_node.style.grid.row_placement {
            style.grid_row = Line {
                start: taffy::style::GridPlacement::Auto,
                end: taffy::style::GridPlacement::Span(row_placement.span),
            };
        }
    }

    fn apply_taffy_incremental(
        taffy: &TaffyTree<crate::TextContext>,
        node_id: TaffyNodeId,
        layout_node: &mut LayoutNode,
    ) {
        if !layout_node.dirty {
            let layout = taffy.layout(node_id).unwrap();
            let dx = layout.location.x as f64 - layout_node.dimensions.content.x;
            let dy = layout.location.y as f64 - layout_node.dimensions.content.y;
            if dx != 0.0 || dy != 0.0 {
                Self::shift_positions(layout_node, dx, dy);
            }
            return;
        }

        let layout = taffy.layout(node_id).unwrap();
        layout_node.dimensions.content.x = layout.location.x as f64;
        layout_node.dimensions.content.y = layout.location.y as f64;
        layout_node.dimensions.content.width = layout.size.width as f64;
        layout_node.dimensions.content.height = layout.size.height as f64;

        let child_ids = taffy.children(node_id).unwrap();
        for (i, child) in layout_node.children.iter_mut().enumerate() {
            if i < child_ids.len() {
                Self::apply_taffy_incremental(taffy, child_ids[i], child);
            }
        }
    }

    fn shift_positions(node: &mut LayoutNode, dx: f64, dy: f64) {
        node.dimensions.content.x += dx;
        node.dimensions.content.y += dy;
        for child in &mut node.children {
            Self::shift_positions(child, dx, dy);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kitsune_css::style_engine::StyleEngine;
    use kitsune_html::parser::parse_html;

    fn get_layout(html: &str, width: f64, height: f64) -> LayoutNode {
        let dom = parse_html(html).unwrap();
        let mut engine = StyleEngine::new();
        let styled = engine.compute_styles(&dom, vec![]);
        crate::engine::LayoutEngine::layout(&styled, Viewport::new(width, height))
    }

    fn find_grid_node(node: &LayoutNode) -> Option<&LayoutNode> {
        if node.style.display == kitsune_css::DisplayType::Grid {
            return Some(node);
        }
        for child in &node.children {
            if let Some(found) = find_grid_node(child) {
                return Some(found);
            }
        }
        None
    }

    #[test]
    fn test_grid_two_columns_equal_fr() {
        // container width 800px. Two columns at 1fr each = 400px each.
        let html = r#"<html><body><div style="display: grid; grid-template-columns: 1fr 1fr; width: 800px; height: 100px;"><div></div><div></div></div></body></html>"#;
        let layout = get_layout(html, 1000.0, 1000.0);
        
        let grid = find_grid_node(&layout).expect("Expected to find a grid node");

        assert_eq!(grid.children.len(), 2);
        
        let child1 = &grid.children[0];
        let child2 = &grid.children[1];

        // Each child should be 400px wide
        assert_eq!(child1.dimensions.content.width, 400.0);
        assert_eq!(child2.dimensions.content.width, 400.0);
        
        // Child 2 should be positioned after Child 1
        assert_eq!(child1.dimensions.content.x, grid.dimensions.content.x);
        assert_eq!(child2.dimensions.content.x, grid.dimensions.content.x + 400.0);
    }

    #[test]
    fn test_grid_three_columns_mixed() {
        // container width 800px. 200px + 1fr + 200px = 200px + 400px + 200px
        let html = r#"<html><body><div style="display: grid; grid-template-columns: 200px 1fr 200px; width: 800px; height: 100px;"><div></div><div></div><div></div></div></body></html>"#;
        let layout = get_layout(html, 1000.0, 1000.0);
        
        let grid = find_grid_node(&layout).expect("Expected to find a grid node");

        assert_eq!(grid.children.len(), 3);
        
        let child1 = &grid.children[0];
        let child2 = &grid.children[1];
        let child3 = &grid.children[2];

        assert_eq!(child1.dimensions.content.width, 200.0);
        assert_eq!(child2.dimensions.content.width, 400.0);
        assert_eq!(child3.dimensions.content.width, 200.0);
        
        assert_eq!(child1.dimensions.content.x, grid.dimensions.content.x);
        assert_eq!(child2.dimensions.content.x, grid.dimensions.content.x + 200.0);
        assert_eq!(child3.dimensions.content.x, grid.dimensions.content.x + 600.0);
    }

    #[test]
    fn test_grid_repeat() {
        // repeat(3, 1fr) inside 600px = three 200px columns
        let html = r#"<html><body><div style="display: grid; grid-template-columns: repeat(3, 1fr); width: 600px; height: 100px;"><div></div><div></div><div></div></div></body></html>"#;
        let layout = get_layout(html, 1000.0, 1000.0);
        
        let grid = find_grid_node(&layout).expect("Expected to find a grid node");

        assert_eq!(grid.children.len(), 3);
        
        for i in 0..3 {
            let child = &grid.children[i];
            assert_eq!(child.dimensions.content.width, 200.0);
            assert_eq!(child.dimensions.content.x, grid.dimensions.content.x + (i as f64 * 200.0));
        }
    }

    #[test]
    fn test_grid_gap() {
        // Two columns, gap 16px. Total width 416px.
        // Formula for fr: remaining space = 416 - 16 (gap) = 400. Each 1fr = 200px.
        let html = r#"<html><body><div style="display: grid; grid-template-columns: 1fr 1fr; gap: 16px; width: 416px; height: 100px;"><div></div><div></div></div></body></html>"#;
        let layout = get_layout(html, 1000.0, 1000.0);
        
        let grid = find_grid_node(&layout).expect("Expected to find a grid node");

        assert_eq!(grid.children.len(), 2);
        
        let child1 = &grid.children[0];
        let child2 = &grid.children[1];

        assert_eq!(child1.dimensions.content.width, 200.0);
        assert_eq!(child2.dimensions.content.width, 200.0);
        
        // Gap of 16px means child2 starts at x + 200 + 16
        assert_eq!(child2.dimensions.content.x, grid.dimensions.content.x + 216.0);
    }

    #[test]
    fn test_grid_child_span() {
        // Three 1fr columns in 600px = 200px each
        // Child 1 spans 2 = 400px
        // Child 2 gets remaining 1 = 200px
        let html = r#"<html><body><div style="display: grid; grid-template-columns: repeat(3, 1fr); width: 600px; height: 100px;"><div style="grid-column: span 2;"></div><div></div></div></body></html>"#;
        let layout = get_layout(html, 1000.0, 1000.0);
        
        let grid = find_grid_node(&layout).expect("Expected to find a grid node");

        assert_eq!(grid.children.len(), 2);
        
        let child1 = &grid.children[0];
        let child2 = &grid.children[1];

        assert_eq!(child1.dimensions.content.width, 400.0);
        assert_eq!(child2.dimensions.content.width, 200.0);
        
        assert_eq!(child1.dimensions.content.x, grid.dimensions.content.x);
        assert_eq!(child2.dimensions.content.x, grid.dimensions.content.x + 400.0);
    }
}

