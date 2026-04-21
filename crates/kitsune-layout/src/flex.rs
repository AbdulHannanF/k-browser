use crate::engine::Viewport;
use crate::layout_tree::{LayoutNode, LayoutType};
use crate::box_model::BoxDimensions;
use crate::LayoutRect;
use kitsune_css::style_engine::StyledNode;
use kitsune_css::{CssUnit, CssValue, DisplayType};
use taffy::prelude::*;
use taffy::tree::NodeId as TaffyNodeId;

pub struct FlexEngine;

impl FlexEngine {
    pub fn layout(root: &StyledNode, viewport: Viewport) -> LayoutNode {
        let mut taffy: TaffyTree<crate::TextContext> = TaffyTree::new();

        // Recursively build taffy tree
        let root_node_id = Self::build_taffy_tree(&mut taffy, root, viewport);

        // Compute layout
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
                    _ => 800.0, // fallback
                };
                let chars_per_line = (avail_width / char_width).max(1.0);
                let num_lines = ((ctx.text.len() as f64) / chars_per_line).ceil().max(1.0);
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
        }).unwrap();

        // Convert back to LayoutNode tree
        Self::build_layout_tree(&taffy, root_node_id, root)
    }

    pub(crate) fn build_taffy_tree(
        taffy: &mut TaffyTree<crate::TextContext>,
        styled: &StyledNode,
        viewport: Viewport,
    ) -> TaffyNodeId {
        let mut style = Style::default();

        // Map display
        style.display = match styled.style.display {
            DisplayType::None => taffy::style::Display::None,
            DisplayType::Flex => taffy::style::Display::Flex,
            DisplayType::Grid => taffy::style::Display::Grid,
            DisplayType::Inline | DisplayType::InlineBlock | DisplayType::InlineFlex => taffy::style::Display::Flex, // Inline nodes need flex to wrap children if they have any text
            _ => taffy::style::Display::Block,
        };

        // Map position (Sticky and Static map to Relative in taffy)
        style.position = match styled.style.position {
            kitsune_css::PositionType::Absolute | 
            kitsune_css::PositionType::Fixed => taffy::style::Position::Absolute,
            _ => taffy::style::Position::Relative, // Fallback for Sticky
        };

        // Size handling
        let map_val = |val: &CssValue| -> Dimension {
            match val {
                CssValue::Length(v, CssUnit::Px) => Dimension::Length(*v as f32),
                CssValue::Percentage(v) => Dimension::Percent((*v / 100.0) as f32),
                CssValue::Length(v, CssUnit::Vw) => Dimension::Length((*v / 100.0 * viewport.width) as f32),
                CssValue::Length(v, CssUnit::Vh) => Dimension::Length((*v / 100.0 * viewport.height) as f32),
                _ => Dimension::Auto,
            }
        };

        if let Some(ref w) = styled.style.width { style.size.width = map_val(w); }
        if let Some(ref h) = styled.style.height { style.size.height = map_val(h); }
        if let Some(ref w) = styled.style.min_width { style.min_size.width = map_val(w); }
        if let Some(ref h) = styled.style.min_height { style.min_size.height = map_val(h); }
        if let Some(ref w) = styled.style.max_width { style.max_size.width = map_val(w); }
        if let Some(ref h) = styled.style.max_height { style.max_size.height = map_val(h); }

        // Flex alignment mapping
        style.flex_direction = match styled.style.flex_direction {
            kitsune_css::FlexDirection::Row => taffy::style::FlexDirection::Row,
            kitsune_css::FlexDirection::RowReverse => taffy::style::FlexDirection::RowReverse,
            kitsune_css::FlexDirection::Column => taffy::style::FlexDirection::Column,
            kitsune_css::FlexDirection::ColumnReverse => taffy::style::FlexDirection::ColumnReverse,
        };

        style.align_items = match styled.style.align_items {
            kitsune_css::AlignItems::Stretch => Some(taffy::style::AlignItems::Stretch),
            kitsune_css::AlignItems::FlexStart => Some(taffy::style::AlignItems::FlexStart),
            kitsune_css::AlignItems::FlexEnd => Some(taffy::style::AlignItems::FlexEnd),
            kitsune_css::AlignItems::Center => Some(taffy::style::AlignItems::Center),
            kitsune_css::AlignItems::Baseline => Some(taffy::style::AlignItems::Baseline),
        };

        style.justify_content = match styled.style.justify_content {
            kitsune_css::JustifyContent::FlexStart => Some(taffy::style::JustifyContent::FlexStart),
            kitsune_css::JustifyContent::FlexEnd => Some(taffy::style::JustifyContent::FlexEnd),
            kitsune_css::JustifyContent::Center => Some(taffy::style::JustifyContent::Center),
            kitsune_css::JustifyContent::SpaceBetween => Some(taffy::style::JustifyContent::SpaceBetween),
            kitsune_css::JustifyContent::SpaceAround => Some(taffy::style::JustifyContent::SpaceAround),
            kitsune_css::JustifyContent::SpaceEvenly => Some(taffy::style::JustifyContent::SpaceEvenly),
        };

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

        crate::grid::GridEngine::apply_grid_styles(styled, &mut style);

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
            text: styled.text.clone(),
            tag: styled.tag.clone(),
            attributes: styled.attributes.clone(),
            style: styled.style.clone(),
            dirty: true, // Brand new nodes are dirty by default until rendered
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
                    _ => 800.0, // fallback
                };
                let chars_per_line = (avail_width / char_width).max(1.0);
                let num_lines = ((ctx.text.len() as f64) / chars_per_line).ceil().max(1.0);
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
        }).unwrap();

        Self::apply_taffy_incremental(&taffy, root_node_id, root);
    }

    pub(crate) fn build_taffy_incremental(
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
            // Since it's clean, we drop any margin/padding inside taffy and simulate it strictly with its exact final size
            return taffy.new_leaf(style).unwrap();
        }

        let mut style = Style::default();

        style.display = match layout_node.style.display {
            DisplayType::None => taffy::style::Display::None,
            DisplayType::Flex => taffy::style::Display::Flex,
            DisplayType::Grid => taffy::style::Display::Grid,
            DisplayType::Inline | DisplayType::InlineBlock | DisplayType::InlineFlex => taffy::style::Display::Flex,
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
        if let Some(ref w) = layout_node.style.min_width { style.min_size.width = map_val(w); }
        if let Some(ref h) = layout_node.style.min_height { style.min_size.height = map_val(h); }
        if let Some(ref w) = layout_node.style.max_width { style.max_size.width = map_val(w); }
        if let Some(ref h) = layout_node.style.max_height { style.max_size.height = map_val(h); }

        style.flex_direction = match layout_node.style.flex_direction {
            kitsune_css::FlexDirection::Row => taffy::style::FlexDirection::Row,
            kitsune_css::FlexDirection::RowReverse => taffy::style::FlexDirection::RowReverse,
            kitsune_css::FlexDirection::Column => taffy::style::FlexDirection::Column,
            kitsune_css::FlexDirection::ColumnReverse => taffy::style::FlexDirection::ColumnReverse,
        };

        style.align_items = match layout_node.style.align_items {
            kitsune_css::AlignItems::Stretch => Some(taffy::style::AlignItems::Stretch),
            kitsune_css::AlignItems::FlexStart => Some(taffy::style::AlignItems::FlexStart),
            kitsune_css::AlignItems::FlexEnd => Some(taffy::style::AlignItems::FlexEnd),
            kitsune_css::AlignItems::Center => Some(taffy::style::AlignItems::Center),
            kitsune_css::AlignItems::Baseline => Some(taffy::style::AlignItems::Baseline),
        };

        style.justify_content = match layout_node.style.justify_content {
            kitsune_css::JustifyContent::FlexStart => Some(taffy::style::JustifyContent::FlexStart),
            kitsune_css::JustifyContent::FlexEnd => Some(taffy::style::JustifyContent::FlexEnd),
            kitsune_css::JustifyContent::Center => Some(taffy::style::JustifyContent::Center),
            kitsune_css::JustifyContent::SpaceBetween => Some(taffy::style::JustifyContent::SpaceBetween),
            kitsune_css::JustifyContent::SpaceAround => Some(taffy::style::JustifyContent::SpaceAround),
            kitsune_css::JustifyContent::SpaceEvenly => Some(taffy::style::JustifyContent::SpaceEvenly),
        };

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

        crate::grid::GridEngine::apply_grid_styles_layout(layout_node, &mut style);

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
