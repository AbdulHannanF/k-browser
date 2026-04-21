// ARCHITECTURE: kitsune-css provides CSS parsing and cascade resolution.
// It parses CSS into a structured representation and resolves the cascade
// to compute final styles for each DOM element.

pub mod values;
pub mod stylesheet;
pub mod cascade;
pub mod style_engine;
pub mod selector;

use serde::{Deserialize, Serialize};

// ─── Grid Types ──────────────────────────────────────────────────────────────

/// A single track sizing value for CSS Grid.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GridTrackSize {
    /// A fixed pixel size (e.g. `200px`).
    Px(f32),
    /// A fractional unit (e.g. `1fr`).
    Fr(f32),
    /// `auto` sizing.
    Auto,
}

/// A grid template track definition — either a single track or a repeat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GridTrackDef {
    /// A single track with the given size.
    Single(GridTrackSize),
    /// `repeat(N, size)` — expands to N copies of the given size.
    Repeat(u16, GridTrackSize),
}

/// Grid placement for a child element (`grid-column` / `grid-row`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GridPlacement {
    /// `span N` — how many tracks this item spans.
    pub span: u16,
}

impl Default for GridPlacement {
    fn default() -> Self {
        Self { span: 1 }
    }
}

/// All grid-related style properties collected in one place.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct GridStyle {
    /// `grid-template-columns` track list.
    pub template_columns: Vec<GridTrackDef>,
    /// `grid-template-rows` track list.
    pub template_rows: Vec<GridTrackDef>,
    /// `column-gap` / `grid-column-gap` in pixels.
    pub column_gap: f32,
    /// `row-gap` / `grid-row-gap` in pixels.
    pub row_gap: f32,
    /// `grid-column` placement for child elements.
    pub column_placement: Option<GridPlacement>,
    /// `grid-row` placement for child elements.
    pub row_placement: Option<GridPlacement>,
}

/// A parsed CSS property value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CssValue {
    /// A keyword (e.g., "auto", "none", "block").
    Keyword(String),
    /// A length value (e.g., "16px", "1.5em").
    Length(f64, CssUnit),
    /// A percentage (e.g., "50%").
    Percentage(f64),
    /// A color.
    Color(CssColor),
    /// A number (e.g., line-height: 1.5).
    Number(f64),
    /// A string value.
    String(String),
}

/// CSS length units.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum CssUnit {
    Px,
    Em,
    Rem,
    Vh,
    Vw,
    Percent,
}

/// A CSS color.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CssColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: f32,
}

impl CssColor {
    pub fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    pub fn rgba(r: u8, g: u8, b: u8, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub fn transparent() -> Self {
        Self { r: 0, g: 0, b: 0, a: 0.0 }
    }

    pub fn black() -> Self {
        Self::rgb(0, 0, 0)
    }

    pub fn white() -> Self {
        Self::rgb(255, 255, 255)
    }
}

/// Computed style for a DOM element.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComputedStyle {
    pub display: DisplayType,
    pub position: PositionType,
    pub width: Option<CssValue>,
    pub height: Option<CssValue>,
    pub min_width: Option<CssValue>,
    pub min_height: Option<CssValue>,
    pub max_width: Option<CssValue>,
    pub max_height: Option<CssValue>,
    pub margin: BoxEdges,
    pub padding: BoxEdges,
    pub border_width: BoxEdges,
    pub border_radius: BoxEdges,
    pub box_shadow: Option<String>,
    pub color: CssColor,
    pub background_color: CssColor,
    pub background_image: Option<String>,
    pub border_color: CssColor,
    pub border_style: BorderStyle,
    pub font_size: f64,
    pub font_family: String,
    pub font_weight: u16,
    pub line_height: f64,
    pub overflow: Overflow,
    pub opacity: f32,
    pub visibility: Visibility,
    pub custom_properties: std::collections::HashMap<String, String>,
    /// CSS Grid layout properties (populated when `display: grid`).
    pub grid: GridStyle,
    /// CSS transitions active on this element.
    pub transitions: Vec<TransitionSpec>,
    /// Inset (top/right/bottom/left) for positioned elements.
    pub inset_top: Option<CssValue>,
    pub inset_right: Option<CssValue>,
    pub inset_bottom: Option<CssValue>,
    pub inset_left: Option<CssValue>,
    /// Z-index for stacking context.
    pub z_index: Option<i32>,
    /// Flexbox layout properties.
    pub flex_direction: FlexDirection,
    pub align_items: AlignItems,
    pub justify_content: JustifyContent,
}

impl Default for ComputedStyle {
    fn default() -> Self {
        Self {
            display: DisplayType::Block,
            position: PositionType::Static,
            width: None,
            height: None,
            min_width: None,
            min_height: None,
            max_width: None,
            max_height: None,
            margin: BoxEdges::zero(),
            padding: BoxEdges::zero(),
            border_width: BoxEdges::zero(),
            border_radius: BoxEdges::zero(),
            box_shadow: None,
            color: CssColor::black(),
            background_color: CssColor::transparent(),
            background_image: None,
            border_color: CssColor::black(),
            border_style: BorderStyle::None,
            font_size: 16.0,
            font_family: "sans-serif".to_string(),
            font_weight: 400,
            line_height: 1.2,
            overflow: Overflow::Visible,
            opacity: 1.0,
            visibility: Visibility::Visible,
            custom_properties: std::collections::HashMap::new(),
            grid: GridStyle::default(),
            transitions: Vec::new(),
            inset_top: None,
            inset_right: None,
            inset_bottom: None,
            inset_left: None,
            z_index: None,
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Stretch,
            justify_content: JustifyContent::FlexStart,
        }
    }
}

// ─── Transitions ─────────────────────────────────────────────────────────────

/// Supported properties that can be transitioned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TransitionProperty {
    Opacity,
    Transform,
    Color,
    BackgroundColor,
}

/// Easing functions for animations/transitions.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum EasingFunction {
    Linear,
    Ease,
    EaseIn,
    EaseOut,
}

impl Default for EasingFunction {
    fn default() -> Self {
        Self::Ease
    }
}

/// A parsed CSS transition specification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransitionSpec {
    pub property: TransitionProperty,
    pub duration_secs: f32,
    pub easing: EasingFunction,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum DisplayType {
    Block,
    Inline,
    InlineBlock,
    Flex,
    InlineFlex,
    Grid,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PositionType {
    Static,
    Relative,
    Absolute,
    Fixed,
    Sticky,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Overflow {
    Visible,
    Hidden,
    Scroll,
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Visibility {
    Visible,
    Hidden,
    Collapse,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum BorderStyle {
    None,
    Solid,
    Dashed,
    Dotted,
    Double,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum FlexDirection {
    Row,
    RowReverse,
    Column,
    ColumnReverse,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum AlignItems {
    Stretch,
    FlexStart,
    FlexEnd,
    Center,
    Baseline,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum JustifyContent {
    FlexStart,
    FlexEnd,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

impl Default for BorderStyle {
    fn default() -> Self {
        Self::None
    }
}

/// Edge values for margin, padding, border.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BoxEdges {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}

impl BoxEdges {
    pub fn zero() -> Self {
        Self { top: 0.0, right: 0.0, bottom: 0.0, left: 0.0 }
    }

    pub fn uniform(value: f64) -> Self {
        Self { top: value, right: value, bottom: value, left: value }
    }
}

impl Default for BoxEdges {
    fn default() -> Self {
        Self::zero()
    }
}
