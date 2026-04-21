//! CSS Style Engine — applies styles from UA stylesheet, author stylesheets,
//! and inline `style=""` attributes to a DOM tree.
//!
//! Produces a `StyledTree` which pairs each DOM node with its `ComputedStyle`.

use crate::{
    BoxEdges, ComputedStyle, CssColor, CssValue, DisplayType,
    GridPlacement, GridTrackDef, GridTrackSize,
};
use crate::selector::{DomElement, ParsedRule, SelectorEngine, SparseStyle};
use kitsune_html::dom::{DomTree, NodeId, NodeType};
use tracing::{debug, info};
use dashmap::DashMap;
use rayon::prelude::*;

// ─── User-Agent Stylesheet ───────────────────────────────────────────────────

/// Minimal UA stylesheet — same defaults every browser applies.
const UA_STYLESHEET: &str = r#"
html { display: block; }
body { display: block; margin: 8px; }
div, p, ul, ol, li, section, article, nav, header, footer, main, aside, h1, h2, h3, h4, h5, h6, details, summary, picture { display: block; }
span, a, em, strong, b, i, u, code, small, sub, sup { display: inline; }
source { display: none; }
h1 { font-size: 32px; font-weight: 700; margin-top: 21px; margin-bottom: 21px; }
h2 { font-size: 24px; font-weight: 700; margin-top: 19px; margin-bottom: 19px; }
h3 { font-size: 18px; font-weight: 700; margin-top: 18px; margin-bottom: 18px; }
h4 { font-size: 16px; font-weight: 700; margin-top: 21px; margin-bottom: 21px; }
h5 { font-size: 13px; font-weight: 700; margin-top: 22px; margin-bottom: 22px; }
h6 { font-size: 10px; font-weight: 700; margin-top: 25px; margin-bottom: 25px; }
p { margin-top: 16px; margin-bottom: 16px; }
a { color: #0000EE; text-decoration: underline; }
img { display: inline-block; vertical-align: middle; }
input, button, select, textarea { display: inline-block; border-width: 1px; border-style: solid; border-color: #767676; padding: 2px; background-color: white; color: black; font-family: sans-serif; font-size: 13px; }
button { padding: 4px 10px; background-color: #efefef; }
hr { display: block; height: 1px; border-width: 1px; border-style: solid; border-color: #cccccc; margin: 16px 0; }
"#;

// ─── StyledNode ──────────────────────────────────────────────────────────────

/// A node in the styled tree — pairs a DOM node with its computed style.
#[derive(Debug, Clone)]
pub struct StyledNode {
    /// The DOM node ID this style applies to.
    pub node_id: u64,
    /// Tag name (e.g. "div", "p", "h1") or "#text" for text nodes.
    pub tag: String,
    /// Text content for text nodes (None for element nodes).
    pub text: Option<String>,
    /// Element attributes (e.g. for images/scripts).
    pub attributes: std::collections::HashMap<String, String>,
    /// Computed style after cascade resolution.
    pub style: ComputedStyle,
    /// Child styled nodes.
    pub children: Vec<StyledNode>,
}

/// A complete styled tree produced by the style engine.
#[derive(Debug, Clone)]
pub struct StyledTree {
    pub root: StyledNode,
}

// ─── StyleEngine ─────────────────────────────────────────────────────────────

/// The CSS style engine. Applies UA rules, author rules, and inline styles.
pub struct StyleEngine {
    ua_rules: Vec<ParsedRule>,
    author_rules: Vec<ParsedRule>,
    style_cache: DashMap<(u64, u64), ComputedStyle>,
    current_stylesheet_hash: u64,
}

impl StyleEngine {
    /// Create a new style engine with the built-in UA stylesheet.
    pub fn new() -> Self {
        let ua_rules = SelectorEngine::parse_stylesheet(UA_STYLESHEET);
        info!(rules = ua_rules.len(), "Style engine initialized with UA stylesheet");
        Self {
            ua_rules,
            author_rules: Vec::new(),
            style_cache: DashMap::new(),
            current_stylesheet_hash: 1,
        }
    }

    /// Increment the stylesheet hash to effectively invalidate the cache.
    pub fn invalidate_cache(&mut self) {
        self.current_stylesheet_hash = self.current_stylesheet_hash.wrapping_add(1);
        self.style_cache.clear();
    }

    /// Compute styles for all nodes in a DOM tree.
    pub fn compute_styles(&mut self, dom: &DomTree, author_sheets: Vec<String>) -> StyledTree {
        self.author_rules.clear();
        for sheet in author_sheets {
            self.author_rules.extend(SelectorEngine::parse_stylesheet(&sheet));
        }

        let _span = tracing::info_span!("pipeline::style").entered();
        let root_node = dom.root().expect("DOM tree must have a root node");
        let root = self.style_dom_node(dom, root_node.id, &ComputedStyle::default());
        StyledTree { root }
    }

    /// Recursively style a DOM node and its children using the public API.
    fn style_dom_node(&self, dom: &DomTree, node_id: NodeId, parent_style: &ComputedStyle) -> StyledNode {
        let node = match dom.get_node(node_id) {
            Some(n) => n,
            None => return StyledNode {
                node_id: node_id.0,
                tag: "#missing".to_string(),
                text: None,
                attributes: std::collections::HashMap::new(),
                style: ComputedStyle::default(),
                children: vec![],
            },
        };

        match &node.node_type {
            NodeType::Document => {
                let style = ComputedStyle::default();
                let styled_children: Vec<StyledNode> = node.children
                    .par_iter()
                    .map(|&child_id| self.style_dom_node(dom, child_id, &style))
                    .collect();
                StyledNode {
                    node_id: node_id.0,
                    tag: "#document".to_string(),
                    text: None,
                    attributes: std::collections::HashMap::new(),
                    style,
                    children: styled_children,
                }
            }
            NodeType::Element(data) => {
                let tag = data.tag_name.to_lowercase();

                // Skip <head>, <script>, <style>, <meta>, <link>, <title> for rendering
                if matches!(tag.as_str(), "head" | "script" | "style" | "meta" | "link" | "title" | "noscript") {
                    return StyledNode {
                        node_id: node_id.0,
                        tag,
                        text: None,
                        attributes: data.attributes.clone(),
                        style: ComputedStyle { display: DisplayType::None, ..Default::default() },
                        children: vec![],
                    };
                }

                let cache_key = (node_id.0, self.current_stylesheet_hash);
                
                let mut style = if let Some(cached) = self.style_cache.get(&cache_key) {
                    cached.clone()
                } else {
                    let s = self.compute_element_style(dom, node_id, parent_style);
                    self.style_cache.insert(cache_key, s.clone());
                    s
                };

                // Parse inline style attribute
                if let Some(inline_style) = data.get_attribute("style") {
                    apply_inline_style(&mut style, inline_style);
                }

                let styled_children: Vec<StyledNode> = node.children
                    .par_iter()
                    .map(|&child_id| self.style_dom_node(dom, child_id, &style))
                    .collect();

                StyledNode {
                    node_id: node_id.0,
                    tag,
                    text: None,
                    attributes: data.attributes.clone(),
                    style,
                    children: styled_children,
                }
            }
            NodeType::Text(content) => {
                // Text nodes inherit parent style
                let mut style = parent_style.clone();
                style.display = DisplayType::Inline;

                StyledNode {
                    node_id: node_id.0,
                    tag: "#text".to_string(),
                    text: Some(content.clone()),
                    attributes: std::collections::HashMap::new(),
                    style,
                    children: vec![],
                }
            }
            NodeType::Comment(_) => {
                StyledNode {
                    node_id: node_id.0,
                    tag: "#comment".to_string(),
                    text: None,
                    attributes: std::collections::HashMap::new(),
                    style: ComputedStyle { display: DisplayType::None, ..Default::default() },
                    children: vec![],
                }
            }
        }
    }

    /// Compute the style for an element by applying UA rules + inheritance.
    fn compute_element_style(&self, dom: &DomTree, node_id: NodeId, parent_style: &ComputedStyle) -> ComputedStyle {
        let element = DomElement { tree: dom, id: node_id };

        let mut all_rules = self.ua_rules.clone();
        all_rules.extend(self.author_rules.clone());

        let matched = SelectorEngine::match_rules(&element, &all_rules);
        SelectorEngine::compute_style(&element, &matched, parent_style)
    }
}

impl Default for StyleEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Grid Track Parsing ──────────────────────────────────────────────────────

/// Parse a single grid track size token: `auto`, `Npx`, or `Nfr`.
fn parse_track_size(token: &str) -> Option<GridTrackSize> {
    let token = token.trim();
    if token == "auto" {
        return Some(GridTrackSize::Auto);
    }
    if let Some(num_str) = token.strip_suffix("fr") {
        if let Ok(v) = num_str.trim().parse::<f32>() {
            return Some(GridTrackSize::Fr(v));
        }
    }
    if let Some(num_str) = token.strip_suffix("px") {
        if let Ok(v) = num_str.trim().parse::<f32>() {
            return Some(GridTrackSize::Px(v));
        }
    }
    None
}

/// Parse a `grid-template-columns` / `grid-template-rows` value string.
///
/// Supports: `auto`, `Npx`, `Nfr`, `repeat(N, size)`, and space-separated lists.
pub fn parse_grid_template(value: &str) -> Vec<GridTrackDef> {
    let value = value.trim();
    let mut tracks = Vec::new();

    // Tokenise respecting parentheses so `repeat(3, 1fr)` is one token.
    let mut tokens: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    for ch in value.chars() {
        match ch {
            '(' => { depth += 1; current.push(ch); }
            ')' => {
                depth = depth.saturating_sub(1);
                current.push(ch);
            }
            ' ' | '\t' if depth == 0 => {
                let t = current.trim().to_string();
                if !t.is_empty() { tokens.push(t); }
                current.clear();
            }
            _ => { current.push(ch); }
        }
    }
    let t = current.trim().to_string();
    if !t.is_empty() { tokens.push(t); }

    for token in &tokens {
        let token = token.trim();
        // Handle repeat(N, size)
        if let Some(inner) = token.strip_prefix("repeat(").and_then(|s| s.strip_suffix(')')) {
            let parts: Vec<&str> = inner.splitn(2, ',').collect();
            if parts.len() == 2 {
                if let Ok(count) = parts[0].trim().parse::<u16>() {
                    if let Some(size) = parse_track_size(parts[1].trim()) {
                        tracks.push(GridTrackDef::Repeat(count, size));
                        continue;
                    }
                }
            }
        }
        if let Some(size) = parse_track_size(token) {
            tracks.push(GridTrackDef::Single(size));
        }
    }
    tracks
}

/// Parse a `grid-column` / `grid-row` value (e.g. `span 2`).
pub fn parse_grid_placement(value: &str) -> Option<GridPlacement> {
    let value = value.trim();
    if let Some(span_str) = value.strip_prefix("span ") {
        if let Ok(n) = span_str.trim().parse::<u16>() {
            return Some(GridPlacement { span: n });
        }
    }
    // Plain integer line number — treat as span 1 starting at that line.
    // For now we only support span syntax.
    None
}

// ─── Inline Style Parsing ────────────────────────────────────────────────────

/// Parse and apply inline `style=""` attribute values.
fn apply_inline_style(style: &mut ComputedStyle, css_text: &str) {
    for declaration in css_text.split(';') {
        let declaration = declaration.trim();
        if declaration.is_empty() {
            continue;
        }
        let parts: Vec<&str> = declaration.splitn(2, ':').collect();
        if parts.len() != 2 {
            continue;
        }
        let property = parts[0].trim().to_lowercase();
        let value = parts[1].trim();

        match property.as_str() {
            "color" => {
                if let Some(c) = parse_color(value) {
                    style.color = c;
                }
            }
            "background-color" | "background" => {
                if let Some(c) = parse_color(value) {
                    style.background_color = c;
                }
            }
            "font-size" => {
                if let Some(px) = parse_px(value) {
                    style.font_size = px;
                }
            }
            "font-weight" => {
                style.font_weight = match value {
                    "bold" | "700" => 700,
                    "normal" | "400" => 400,
                    _ => value.parse().unwrap_or(400),
                };
            }
            "margin" => {
                if let Some(px) = parse_px(value) {
                    style.margin = BoxEdges::uniform(px);
                }
            }
            "margin-top" => {
                if let Some(px) = parse_px(value) {
                    style.margin.top = px;
                }
            }
            "margin-bottom" => {
                if let Some(px) = parse_px(value) {
                    style.margin.bottom = px;
                }
            }
            "padding" => {
                if let Some(px) = parse_px(value) {
                    style.padding = BoxEdges::uniform(px);
                }
            }
            "display" => {
                style.display = match value {
                    "block" => DisplayType::Block,
                    "inline" => DisplayType::Inline,
                    "inline-block" => DisplayType::InlineBlock,
                    "flex" => DisplayType::Flex,
                    "grid" => DisplayType::Grid,
                    "none" => DisplayType::None,
                    _ => style.display,
                };
            }
            "width" => {
                if let Some(px) = parse_px(value) {
                    style.width = Some(CssValue::Length(px, crate::CssUnit::Px));
                }
            }
            "height" => {
                if let Some(px) = parse_px(value) {
                    style.height = Some(CssValue::Length(px, crate::CssUnit::Px));
                }
            }
            "grid-template-columns" => {
                style.grid.template_columns = parse_grid_template(value);
            }
            "grid-template-rows" => {
                style.grid.template_rows = parse_grid_template(value);
            }
            "column-gap" | "grid-column-gap" => {
                if let Some(px) = parse_px(value) {
                    style.grid.column_gap = px as f32;
                }
            }
            "row-gap" | "grid-row-gap" => {
                if let Some(px) = parse_px(value) {
                    style.grid.row_gap = px as f32;
                }
            }
            "gap" => {
                if let Some(px) = parse_px(value) {
                    style.grid.column_gap = px as f32;
                    style.grid.row_gap = px as f32;
                }
            }
            "grid-column" => {
                if let Some(placement) = parse_grid_placement(value) {
                    style.grid.column_placement = Some(placement);
                }
            }
            "grid-row" => {
                if let Some(placement) = parse_grid_placement(value) {
                    style.grid.row_placement = Some(placement);
                }
            }
            "background-image" => {
                if let Some(url) = parse_url(value) {
                    style.background_image = Some(url);
                }
            }
            "border-width" => {
                if let Some(px) = parse_px(value) {
                    style.border_width = BoxEdges::uniform(px);
                }
            }
            "border-color" => {
                if let Some(c) = parse_color(value) {
                    style.border_color = c;
                }
            }
            "border-style" => {
                style.border_style = parse_border_style(value).unwrap_or(style.border_style);
            }
            "border" => {
                // simple shorthand: 1px solid black
                for part in value.split_whitespace() {
                    if let Some(px) = parse_px(part) {
                        style.border_width = BoxEdges::uniform(px);
                    } else if let Some(c) = parse_color(part) {
                        style.border_color = c;
                    } else if let Some(s) = parse_border_style(part) {
                        style.border_style = s;
                    }
                }
            }
            "transition" => {
                style.transitions = parse_transition(value);
            }
            _ => {
                debug!(property = %property, "Ignoring unsupported inline style property");
            }
        }
    }
}

// ─── Color Parsing ───────────────────────────────────────────────────────────

/// Parse a CSS color value (hex, named).
pub fn parse_color(s: &str) -> Option<CssColor> {
    let s = s.trim();
    if s.starts_with('#') {
        return parse_hex_color(s);
    }
    match s.to_lowercase().as_str() {
        "black" => Some(CssColor::black()),
        "white" => Some(CssColor::white()),
        "red" => Some(CssColor::rgb(255, 0, 0)),
        "green" => Some(CssColor::rgb(0, 128, 0)),
        "blue" => Some(CssColor::rgb(0, 0, 255)),
        "gray" | "grey" => Some(CssColor::rgb(128, 128, 128)),
        "transparent" => Some(CssColor::transparent()),
        _ => None,
    }
}

fn parse_hex_color(s: &str) -> Option<CssColor> {
    let hex = s.trim_start_matches('#');
    match hex.len() {
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some(CssColor::rgb(r, g, b))
        }
        3 => {
            let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()?;
            let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()?;
            let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()?;
            Some(CssColor::rgb(r, g, b))
        }
        _ => None,
    }
}

/// Parse a CSS length value like "16px".
pub fn parse_px(s: &str) -> Option<f64> {
    let s = s.trim();
    if s == "0" {
        return Some(0.0);
    }
    if let Some(num_str) = s.strip_suffix("px") {
        return num_str.trim().parse().ok();
    }
    s.parse().ok()
}

pub fn parse_url(s: &str) -> Option<String> {
    let s = s.trim();
    if let Some(inner) = s.strip_prefix("url(").and_then(|s| s.strip_suffix(')')) {
        let inner = inner.trim();
        let inner = inner.strip_prefix('"').and_then(|s| s.strip_suffix('"')).unwrap_or(inner);
        let inner = inner.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')).unwrap_or(inner);
        return Some(inner.to_string());
    }
    None
}

pub fn parse_border_style(s: &str) -> Option<crate::BorderStyle> {
    match s.trim().to_lowercase().as_str() {
        "none" => Some(crate::BorderStyle::None),
        "solid" => Some(crate::BorderStyle::Solid),
        "dashed" => Some(crate::BorderStyle::Dashed),
        "dotted" => Some(crate::BorderStyle::Dotted),
        "double" => Some(crate::BorderStyle::Double),
        _ => None,
    }
}

pub fn parse_css_value(s: &str) -> Option<crate::CssValue> {
    let s = s.trim();
    if s == "0" {
        return Some(crate::CssValue::Length(0.0, crate::CssUnit::Px));
    }
    if let Some(num_str) = s.strip_suffix("px") {
        if let Ok(v) = num_str.trim().parse() {
            return Some(crate::CssValue::Length(v, crate::CssUnit::Px));
        }
    }
    if let Some(num_str) = s.strip_suffix('%') {
        if let Ok(v) = num_str.trim().parse() {
            return Some(crate::CssValue::Percentage(v));
        }
    }
    if let Some(num_str) = s.strip_suffix("vh") {
        if let Ok(v) = num_str.trim().parse() {
            return Some(crate::CssValue::Length(v, crate::CssUnit::Vh));
        }
    }
    if let Some(num_str) = s.strip_suffix("vw") {
        if let Ok(v) = num_str.trim().parse() {
            return Some(crate::CssValue::Length(v, crate::CssUnit::Vw));
        }
    }
    if let Some(num_str) = s.strip_suffix("em") {
        if let Ok(v) = num_str.trim().parse() {
            return Some(crate::CssValue::Length(v, crate::CssUnit::Em));
        }
    }
    if let Some(num_str) = s.strip_suffix("rem") {
        if let Ok(v) = num_str.trim().parse() {
            return Some(crate::CssValue::Length(v, crate::CssUnit::Rem));
        }
    }
    Some(crate::CssValue::Keyword(s.to_string()))
}

/// Parse a `transition` shorthand property like "opacity 0.5s ease-in-out".
pub fn parse_transition(value: &str) -> Vec<crate::TransitionSpec> {
    let mut specs = Vec::new();
    for part in value.split(',') {
        let mut tokens = part.split_whitespace();
        if let Some(prop_str) = tokens.next() {
            let property = match prop_str.to_lowercase().as_str() {
                "opacity" => crate::TransitionProperty::Opacity,
                "transform" => crate::TransitionProperty::Transform,
                "color" => crate::TransitionProperty::Color,
                "background-color" | "background" => crate::TransitionProperty::BackgroundColor,
                _ => continue,
            };
            
            let mut duration = 0.0;
            let mut easing = crate::EasingFunction::Ease;
            
            for token in tokens {
                if let Some(dur) = crate::values::parse_duration(token) {
                    duration = dur;
                } else if let Some(ez) = crate::values::parse_easing(token) {
                    easing = ez;
                }
            }
            specs.push(crate::TransitionSpec {
                property, duration_secs: duration, easing
            });
        }
    }
    specs
}

pub(crate) fn apply_property_to_sparse(style: &mut SparseStyle, prop: &str, value: &str) {
    let prop_lower = prop.to_lowercase();
    
    // Catch custom properties or values containing var() early
    if prop_lower.starts_with("--") {
        style.custom_properties.insert(prop.to_string(), value.to_string());
        return;
    }
    if value.contains("var(") {
        style.raw_properties.insert(prop_lower.clone(), value.to_string());
        // For inherited or critical properties like display, we might want a fallback,
        // but compute_style handles the resolution later.
    }

    match prop_lower.as_str() {
        "color" => {
            if let Some(c) = parse_color(value) {
                style.color = Some(c);
            }
        }
        "background-color" | "background" => {
            if let Some(c) = parse_color(value) {
                style.background_color = Some(c);
            } else if let Some(url) = parse_url(value) {
                style.background_image = Some(url);
            }
        }
        "font-size" => {
            if let Some(px) = parse_px(value) {
                style.font_size = Some(px);
            }
        }
        "font-weight" => {
            style.font_weight = Some(match value.to_lowercase().as_str() {
                "bold" | "700" => 700,
                "normal" | "400" => 400,
                "medium" | "500" => 500,
                _ => value.parse().unwrap_or(400),
            });
        }
        "font-family" => {
            style.font_family = Some(value.to_string());
        }
        "line-height" => {
            if let Ok(lh) = value.parse() {
                style.line_height = Some(lh);
            }
        }
        "margin" => {
            if let Some(px) = parse_px(value) {
                style.margin_top = Some(px);
                style.margin_bottom = Some(px);
                style.margin_left = Some(px);
                style.margin_right = Some(px);
            }
        }
        "margin-top" => {
            if let Some(px) = parse_px(value) {
                style.margin_top = Some(px);
            }
        }
        "margin-bottom" => {
            if let Some(px) = parse_px(value) {
                style.margin_bottom = Some(px);
            }
        }
        "padding" => {
            let parts: Vec<&str> = value.split_whitespace().collect();
            match parts.len() {
                1 => if let Some(px) = parse_px(parts[0]) {
                    style.padding_top = Some(px); style.padding_right = Some(px);
                    style.padding_bottom = Some(px); style.padding_left = Some(px);
                },
                2 => {
                    let v = parse_px(parts[0]); let h = parse_px(parts[1]);
                    if let (Some(v), Some(h)) = (v, h) {
                        style.padding_top = Some(v); style.padding_bottom = Some(v);
                        style.padding_left = Some(h); style.padding_right = Some(h);
                    }
                },
                4 => {
                    let t = parse_px(parts[0]); let r = parse_px(parts[1]);
                    let b = parse_px(parts[2]); let l = parse_px(parts[3]);
                    if let (Some(t), Some(r), Some(b), Some(l)) = (t, r, b, l) {
                        style.padding_top = Some(t); style.padding_right = Some(r);
                        style.padding_bottom = Some(b); style.padding_left = Some(l);
                    }
                }
                _ => {}
            }
        }
        "padding-top" => { if let Some(px) = parse_px(value) { style.padding_top = Some(px); } }
        "padding-bottom" => { if let Some(px) = parse_px(value) { style.padding_bottom = Some(px); } }
        "padding-left" => { if let Some(px) = parse_px(value) { style.padding_left = Some(px); } }
        "padding-right" => { if let Some(px) = parse_px(value) { style.padding_right = Some(px); } }
        "width" => { style.width = parse_css_value(value); }
        "height" => { style.height = parse_css_value(value); }
        "max-width" => { style.max_width = parse_css_value(value); }
        "max-height" => { style.max_height = parse_css_value(value); }
        "min-width" => { style.min_width = parse_css_value(value); }
        "min-height" => { style.min_height = parse_css_value(value); }
        "align-items" => {
            style.align_items = Some(match value.trim() {
                "stretch" => crate::AlignItems::Stretch,
                "flex-start" | "start" => crate::AlignItems::FlexStart,
                "flex-end" | "end" => crate::AlignItems::FlexEnd,
                "center" => crate::AlignItems::Center,
                "baseline" => crate::AlignItems::Baseline,
                _ => return,
            });
        }
        "justify-content" => {
            style.justify_content = Some(match value.trim() {
                "flex-start" | "start" => crate::JustifyContent::FlexStart,
                "flex-end" | "end" => crate::JustifyContent::FlexEnd,
                "center" => crate::JustifyContent::Center,
                "space-between" => crate::JustifyContent::SpaceBetween,
                "space-around" => crate::JustifyContent::SpaceAround,
                "space-evenly" => crate::JustifyContent::SpaceEvenly,
                _ => return,
            });
        }
        "flex-direction" => {
            style.flex_direction = Some(match value.trim() {
                "row" => crate::FlexDirection::Row,
                "row-reverse" => crate::FlexDirection::RowReverse,
                "column" => crate::FlexDirection::Column,
                "column-reverse" => crate::FlexDirection::ColumnReverse,
                _ => return,
            });
        }
        "display" => {
            style.display = Some(match value {
                "block" => DisplayType::Block,
                "inline" => DisplayType::Inline,
                "inline-block" => DisplayType::InlineBlock,
                "flex" => DisplayType::Flex,
                "inline-flex" => DisplayType::InlineFlex,
                "grid" => DisplayType::Grid,
                "none" => DisplayType::None,
                _ => return, // Don't override
            });
        }
        "position" => {
            style.position = Some(match value {
                "static" => crate::PositionType::Static,
                "relative" => crate::PositionType::Relative,
                "absolute" => crate::PositionType::Absolute,
                "fixed" => crate::PositionType::Fixed,
                "sticky" => crate::PositionType::Sticky,
                _ => return,
            });
        }
        "border-radius" => {
            if let Some(px) = parse_px(value) {
                style.border_radius_top_left = Some(px);
                style.border_radius_top_right = Some(px);
                style.border_radius_bottom_right = Some(px);
                style.border_radius_bottom_left = Some(px);
            }
        }
        "box-shadow" => {
            style.box_shadow = Some(value.to_string());
        }
        "grid-template-columns" => {
            style.grid_template_columns = Some(parse_grid_template(value));
        }
        "grid-template-rows" => {
            style.grid_template_rows = Some(parse_grid_template(value));
        }
        "column-gap" | "grid-column-gap" => {
            if let Some(px) = parse_px(value) {
                style.grid_column_gap = Some(px as f32);
            }
        }
        "row-gap" | "grid-row-gap" => {
            if let Some(px) = parse_px(value) {
                style.grid_row_gap = Some(px as f32);
            }
        }
        "gap" => {
            if let Some(px) = parse_px(value) {
                style.grid_column_gap = Some(px as f32);
                style.grid_row_gap = Some(px as f32);
            }
        }
        "grid-column" => {
            style.grid_column_placement = parse_grid_placement(value);
        }
        "grid-row" => {
            style.grid_row_placement = parse_grid_placement(value);
        }
        "transition" => {
            style.transitions = Some(parse_transition(value));
        }
        "background-image" => {
            if let Some(url) = parse_url(value) {
                style.background_image = Some(url);
            }
        }
        "border-width" => {
            if let Some(px) = parse_px(value) {
                style.border_top_width = Some(px);
                style.border_right_width = Some(px);
                style.border_bottom_width = Some(px);
                style.border_left_width = Some(px);
            }
        }
        "border-color" => {
            if let Some(c) = parse_color(value) {
                style.border_color = Some(c);
            }
        }
        "border-style" => {
            style.border_style = parse_border_style(value);
        }
        "border" => {
             for part in value.split_whitespace() {
                if let Some(px) = parse_px(part) {
                    style.border_top_width = Some(px);
                    style.border_right_width = Some(px);
                    style.border_bottom_width = Some(px);
                    style.border_left_width = Some(px);
                } else if let Some(c) = parse_color(part) {
                    style.border_color = Some(c);
                } else if let Some(s) = parse_border_style(part) {
                    style.border_style = Some(s);
                }
            }
        }
        _ => {
            if prop.starts_with("--") {
                style.custom_properties.insert(prop.to_string(), value.to_string());
            } else if value.contains("var(") {
                style.raw_properties.insert(prop.to_string(), value.to_string());
            }
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ua_stylesheet_parses() {
        let rules = crate::selector::SelectorEngine::parse_stylesheet(UA_STYLESHEET);
        assert!(!rules.is_empty(), "UA stylesheet should produce rules");
        // h1 should have font-size 32 (find the rule with font_size set)
        let h1_rule = rules.iter().find(|r| r.style.font_size.is_some());
        assert!(h1_rule.is_some(), "Should have a dedicated rule with font_size");
        assert_eq!(h1_rule.unwrap().style.font_size, Some(32.0));
    }

    #[test]
    fn test_parse_hex_color() {
        let c = parse_hex_color("#ff0000").unwrap();
        assert_eq!(c.r, 255);
        assert_eq!(c.g, 0);
        assert_eq!(c.b, 0);
    }

    #[test]
    fn test_parse_px() {
        assert_eq!(parse_px("16px"), Some(16.0));
        assert_eq!(parse_px("0"), Some(0.0));
        assert_eq!(parse_px("1.5"), Some(1.5));
    }

    #[test]
    fn test_inline_style_parsing() {
        let mut style = ComputedStyle::default();
        apply_inline_style(&mut style, "color: red; font-size: 24px; font-weight: bold");
        assert_eq!(style.color.r, 255);
        assert_eq!(style.font_size, 24.0);
        assert_eq!(style.font_weight, 700);
    }

    #[test]
    fn test_style_engine_produces_styled_tree() {
        let html = "<html><body><h1>Hello</h1><p>World</p></body></html>";
        let dom = kitsune_html::parser::parse_html(html).unwrap();
        let mut engine = StyleEngine::new();
        let styled = engine.compute_styles(&dom, vec![]);
        assert_eq!(styled.root.tag, "#document");
    }
}
