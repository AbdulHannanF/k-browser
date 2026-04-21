use cssparser::{CowRcStr, ToCss};
use tracing::debug;
use kitsune_html::dom::{DomNode, DomTree, NodeId, NodeType};
use precomputed_hash::PrecomputedHash;
use selectors::attr::{AttrSelectorOperation, CaseSensitivity, NamespaceConstraint};
use selectors::matching::{matches_selector, ElementSelectorFlags, MatchingContext};
use selectors::parser::{NonTSPseudoClass, PseudoElement, SelectorImpl, SelectorList, SelectorParseErrorKind};
use selectors::{Element, OpaqueElement};
use std::fmt;

#[derive(Debug, Clone)]
pub struct KitsuneSelectorImpl;

impl SelectorImpl for KitsuneSelectorImpl {
    type ExtraMatchingData<'a> = PseudoClassState;
    type AttrValue = KitsuneString;
    type Identifier = KitsuneString;
    type LocalName = KitsuneString;
    type NamespaceUrl = KitsuneString;
    type NamespacePrefix = KitsuneString;
    type BorrowedNamespaceUrl = str;
    type BorrowedLocalName = str;
    type NonTSPseudoClass = PseudoClass;
    type PseudoElement = KitsunePseudoElement;
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Hash)]
pub struct KitsuneString(pub String);

impl<'a> From<&'a str> for KitsuneString {
    fn from(s: &'a str) -> Self { KitsuneString(s.to_string()) }
}

impl AsRef<str> for KitsuneString {
    fn as_ref(&self) -> &str { &self.0 }
}

impl std::borrow::Borrow<str> for KitsuneString {
    fn borrow(&self) -> &str { &self.0 }
}

impl ToCss for KitsuneString {
    fn to_css<W: fmt::Write>(&self, dest: &mut W) -> fmt::Result {
        cssparser::serialize_identifier(&self.0, dest)
    }
}

impl PrecomputedHash for KitsuneString {
    fn precomputed_hash(&self) -> u32 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.0.hash(&mut hasher);
        hasher.finish() as u32
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PseudoClass {
    Hover,
    Focus,
    Active,
    Any,
}

impl NonTSPseudoClass for PseudoClass {
    type Impl = KitsuneSelectorImpl;
    fn is_active_or_hover(&self) -> bool { false }
    fn is_user_action_state(&self) -> bool { false }
}

impl ToCss for PseudoClass {
    fn to_css<W: fmt::Write>(&self, dest: &mut W) -> fmt::Result {
        dest.write_str(match self {
            PseudoClass::Hover => ":hover",
            PseudoClass::Focus => ":focus",
            PseudoClass::Active => ":active",
            PseudoClass::Any => ":any",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PseudoClassState {
    pub hovered: Option<NodeId>,
    pub focused: Option<NodeId>,
    pub active: Option<NodeId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KitsunePseudoElement {}

impl PseudoElement for KitsunePseudoElement {
    type Impl = KitsuneSelectorImpl;
}

impl ToCss for KitsunePseudoElement {
    fn to_css<W: fmt::Write>(&self, _dest: &mut W) -> fmt::Result {
        Ok(())
    }
}

pub struct SelectorParser;

impl<'i> selectors::parser::Parser<'i> for SelectorParser {
    type Impl = KitsuneSelectorImpl;
    type Error = SelectorParseErrorKind<'i>;

    fn parse_non_ts_pseudo_class(
        &self,
        _location: cssparser::SourceLocation,
        name: CowRcStr<'i>,
    ) -> Result<PseudoClass, cssparser::ParseError<'i, Self::Error>> {
        match name.as_ref() {
            "hover" => Ok(PseudoClass::Hover),
            "focus" => Ok(PseudoClass::Focus),
            "active" => Ok(PseudoClass::Active),
            _ => Ok(PseudoClass::Any),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DomElement<'a> {
    pub tree: &'a DomTree,
    pub id: NodeId,
}

impl<'a> DomElement<'a> {
    pub fn node(&self) -> &'a DomNode {
        self.tree.get_node(self.id).unwrap()
    }
}

impl<'a> PartialEq for DomElement<'a> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<'a> Eq for DomElement<'a> {}

impl<'a> Element for DomElement<'a> {
    type Impl = KitsuneSelectorImpl;

    fn opaque(&self) -> OpaqueElement {
        OpaqueElement::new(self.node())
    }

    fn parent_element(&self) -> Option<Self> {
        let parent_id = self.node().parent?;
        let parent = self.tree.get_node(parent_id)?;
        if matches!(parent.node_type, NodeType::Element(_)) {
            Some(DomElement {
                tree: self.tree,
                id: parent_id,
            })
        } else {
            None
        }
    }

    fn parent_node_is_shadow_root(&self) -> bool {
        false
    }

    fn containing_shadow_host(&self) -> Option<Self> {
        None
    }

    fn is_pseudo_element(&self) -> bool {
        false
    }

    fn prev_sibling_element(&self) -> Option<Self> {
        let parent_id = self.node().parent?;
        let parent = self.tree.get_node(parent_id)?;
        let mut prev = None;
        for &child_id in &parent.children {
            if child_id == self.id {
                return prev;
            }
            if let Some(child) = self.tree.get_node(child_id) {
                if matches!(child.node_type, NodeType::Element(_)) {
                    prev = Some(DomElement {
                        tree: self.tree,
                        id: child_id,
                    });
                }
            }
        }
        None
    }

    fn next_sibling_element(&self) -> Option<Self> {
        let parent_id = self.node().parent?;
        let parent = self.tree.get_node(parent_id)?;
        let mut found_self = false;
        for &child_id in &parent.children {
            if found_self {
                if let Some(child) = self.tree.get_node(child_id) {
                    if matches!(child.node_type, NodeType::Element(_)) {
                        return Some(DomElement {
                            tree: self.tree,
                            id: child_id,
                        });
                    }
                }
            }
            if child_id == self.id {
                found_self = true;
            }
        }
        None
    }

    fn first_element_child(&self) -> Option<Self> {
        for &child_id in &self.node().children {
            if let Some(child) = self.tree.get_node(child_id) {
                if matches!(child.node_type, NodeType::Element(_)) {
                    return Some(DomElement {
                        tree: self.tree,
                        id: child_id,
                    });
                }
            }
        }
        None
    }

    fn is_html_element_in_html_document(&self) -> bool {
        true
    }

    fn has_local_name(&self, local_name: &<Self::Impl as SelectorImpl>::BorrowedLocalName) -> bool {
        if let NodeType::Element(data) = &self.node().node_type {
            data.tag_name == local_name
        } else {
            false
        }
    }

    fn has_namespace(&self, _ns: &<Self::Impl as SelectorImpl>::BorrowedNamespaceUrl) -> bool {
        true
    }

    fn is_same_type(&self, other: &Self) -> bool {
        if let (NodeType::Element(data1), NodeType::Element(data2)) = (&self.node().node_type, &other.node().node_type) {
            data1.tag_name == data2.tag_name
        } else {
            false
        }
    }

    fn match_non_ts_pseudo_class(
        &self,
        pc: &<Self::Impl as SelectorImpl>::NonTSPseudoClass,
        context: &mut MatchingContext<Self::Impl>,
    ) -> bool {
        let state = &context.extra_data;
        match pc {
            PseudoClass::Hover => state.hovered == Some(self.id),
            PseudoClass::Focus => state.focused == Some(self.id),
            PseudoClass::Active => state.active == Some(self.id),
            PseudoClass::Any => false,
        }
    }

    fn match_pseudo_element(
        &self,
        _pe: &<Self::Impl as SelectorImpl>::PseudoElement,
        _context: &mut MatchingContext<Self::Impl>,
    ) -> bool {
        false
    }

    fn apply_selector_flags(&self, _flags: ElementSelectorFlags) {}

    fn is_link(&self) -> bool {
        if let NodeType::Element(data) = &self.node().node_type {
            data.tag_name == "a" && data.get_attribute("href").is_some()
        } else {
            false
        }
    }

    fn is_html_slot_element(&self) -> bool {
        false
    }

    fn has_id(&self, id: &<Self::Impl as SelectorImpl>::Identifier, case_sensitivity: CaseSensitivity) -> bool {
        if let NodeType::Element(data) = &self.node().node_type {
            if let Some(elem_id) = data.id() {
                match case_sensitivity {
                    CaseSensitivity::CaseSensitive => elem_id == id.as_ref(),
                    CaseSensitivity::AsciiCaseInsensitive => elem_id.eq_ignore_ascii_case(id.as_ref()),
                }
            } else {
                false
            }
        } else {
            false
        }
    }

    fn has_class(&self, name: &<Self::Impl as SelectorImpl>::Identifier, case_sensitivity: CaseSensitivity) -> bool {
        if let NodeType::Element(data) = &self.node().node_type {
            data.classes().into_iter().any(|c| match case_sensitivity {
                CaseSensitivity::CaseSensitive => c == name.as_ref(),
                CaseSensitivity::AsciiCaseInsensitive => c.eq_ignore_ascii_case(name.as_ref()),
            })
        } else {
            false
        }
    }

    fn imported_part(&self, _name: &<Self::Impl as SelectorImpl>::Identifier) -> Option<<Self::Impl as SelectorImpl>::Identifier> {
        None
    }

    fn is_part(&self, _name: &<Self::Impl as SelectorImpl>::Identifier) -> bool {
        false
    }

    fn is_empty(&self) -> bool {
        !self.node().children.iter().any(|&c| {
            if let Some(child) = self.tree.get_node(c) {
                match child.node_type {
                    NodeType::Element(_) => true,
                    NodeType::Text(ref t) => !t.is_empty(),
                    _ => false,
                }
            } else {
                false
            }
        })
    }

    fn is_root(&self) -> bool {
        if let Some(parent_id) = self.node().parent {
            if let Some(parent) = self.tree.get_node(parent_id) {
                return matches!(parent.node_type, NodeType::Document);
            }
        }
        false
    }

    fn attr_matches(
        &self,
        _ns: &NamespaceConstraint<&<Self::Impl as SelectorImpl>::NamespaceUrl>,
        local_name: &<Self::Impl as SelectorImpl>::LocalName,
        operation: &AttrSelectorOperation<&<Self::Impl as SelectorImpl>::AttrValue>,
    ) -> bool {
        if let NodeType::Element(data) = &self.node().node_type {
            if let Some(val) = data.get_attribute(local_name.as_ref()) {
                return operation.eval_str(val);
            }
        }
        false
    }
    
    fn has_custom_state(&self, _name: &<Self::Impl as SelectorImpl>::Identifier) -> bool {
        false
    }
    
    fn add_element_unique_hashes(&self, _filter: &mut selectors::bloom::BloomFilter) -> bool {
        false
    }
}

pub use crate::{ComputedStyle, CssColor, DisplayType, GridStyle, GridTrackDef, GridTrackSize, GridPlacement};

#[derive(Default, Debug, Clone)]
pub struct SparseStyle {
    pub display: Option<DisplayType>,
    pub color: Option<CssColor>,
    pub background_color: Option<CssColor>,
    pub background_image: Option<String>,
    pub border_color: Option<CssColor>,
    pub border_style: Option<crate::BorderStyle>,
    pub font_size: Option<f64>,
    pub font_weight: Option<u16>,
    pub font_family: Option<String>,
    pub line_height: Option<f64>,
    pub margin_top: Option<f64>,
    pub margin_bottom: Option<f64>,
    pub margin_left: Option<f64>,
    pub margin_right: Option<f64>,
    pub padding_top: Option<f64>,
    pub padding_bottom: Option<f64>,
    pub padding_left: Option<f64>,
    pub padding_right: Option<f64>,
    pub width: Option<crate::CssValue>,
    pub height: Option<crate::CssValue>,
    pub min_width: Option<crate::CssValue>,
    pub min_height: Option<crate::CssValue>,
    pub max_width: Option<crate::CssValue>,
    pub max_height: Option<crate::CssValue>,
    pub border_radius_top_left: Option<f64>,
    pub border_radius_top_right: Option<f64>,
    pub border_radius_bottom_right: Option<f64>,
    pub border_radius_bottom_left: Option<f64>,
    pub border_top_width: Option<f64>,
    pub border_right_width: Option<f64>,
    pub border_bottom_width: Option<f64>,
    pub border_left_width: Option<f64>,
    pub box_shadow: Option<String>,
    pub position: Option<crate::PositionType>,
    pub custom_properties: std::collections::HashMap<String, String>,
    pub raw_properties: std::collections::HashMap<String, String>,
    /// `grid-template-columns` parsed track list.
    pub grid_template_columns: Option<Vec<GridTrackDef>>,
    /// `grid-template-rows` parsed track list.
    pub grid_template_rows: Option<Vec<GridTrackDef>>,
    /// `column-gap` / `grid-column-gap` in pixels.
    pub grid_column_gap: Option<f32>,
    /// `row-gap` / `grid-row-gap` in pixels.
    pub grid_row_gap: Option<f32>,
    /// `grid-column` span placement.
    pub grid_column_placement: Option<GridPlacement>,
    /// `grid-row` span placement.
    pub grid_row_placement: Option<GridPlacement>,
    /// Transition specifications.
    pub transitions: Option<Vec<crate::TransitionSpec>>,
    /// Flex properties
    pub flex_direction: Option<crate::FlexDirection>,
    pub align_items: Option<crate::AlignItems>,
    pub justify_content: Option<crate::JustifyContent>,
}

#[derive(Debug, Clone)]
pub struct ParsedRule {
    pub selectors: SelectorList<KitsuneSelectorImpl>,
    pub style: SparseStyle,
}

pub struct MatchedRule<'a> {
    pub specificity: u32,
    pub rule: &'a ParsedRule,
}

pub struct SelectorEngine;

impl SelectorEngine {
    pub fn parse_stylesheet(css: &str) -> Vec<ParsedRule> {
        let mut rules = Vec::new();
        
        // 1. Strip comments
        let mut clean_css = String::new();
        let mut in_comment = false;
        let mut chars = css.chars().peekable();
        while let Some(ch) = chars.next() {
            if in_comment {
                if ch == '*' && chars.peek() == Some(&'/') {
                    chars.next();
                    in_comment = false;
                }
            } else if ch == '/' && chars.peek() == Some(&'*') {
                chars.next();
                in_comment = true;
            } else {
                clean_css.push(ch);
            }
        }

        // 2. Split by '}' but handle nested blocks (naive brace counting)
        let mut current_block = String::new();
        let mut brace_depth = 0;
        
        for ch in clean_css.chars() {
            current_block.push(ch);
            if ch == '{' {
                brace_depth += 1;
            } else if ch == '}' {
                brace_depth -= 1;
                if brace_depth == 0 {
                    let block_trimmed = current_block.trim().to_string();
                    current_block.clear();
                    
                    if block_trimmed.is_empty() { continue; }
                    
                    // Skip at-rules for now (they contain nested blocks we don't support well yet)
                    if block_trimmed.starts_with('@') { continue; }
                    
                    if let Some((selectors_str, declarations_str)) = block_trimmed.split_once('{') {
                        let selectors_str = selectors_str.trim();
                        let declarations_str = declarations_str.trim_end_matches('}');
                        
                        let mut parser_input = cssparser::ParserInput::new(selectors_str);
                        let mut parser = cssparser::Parser::new(&mut parser_input);
                        
                        let selector_list = match SelectorList::parse(&SelectorParser, &mut parser, selectors::parser::ParseRelative::No) {
                            Ok(list) => list,
                            Err(_) => continue,
                        };

                        let mut style = SparseStyle::default();
                        for decl in declarations_str.split(';') {
                            let mut parts = decl.splitn(2, ':');
                            let prop = parts.next().unwrap_or("").trim();
                            let val = parts.next().unwrap_or("").trim();
                            if prop.is_empty() || val.is_empty() {
                                continue;
                            }
                            crate::style_engine::apply_property_to_sparse(&mut style, prop, val);
                        }

                        rules.push(ParsedRule {
                            selectors: selector_list,
                            style,
                        });
                    }
                }
            }
        }
        rules
    }

    pub fn match_rules<'a>(node: &DomElement<'a>, stylesheet: &'a [ParsedRule]) -> Vec<MatchedRule<'a>> {
        let mut matched = Vec::new();
        // Set up context with dummy caches to satisfy selectors crate
        let mut nth_cache = Default::default();
        let mut context = MatchingContext::new(
            selectors::matching::MatchingMode::Normal,
            None,
            &mut nth_cache,
            selectors::context::QuirksMode::NoQuirks,
            selectors::matching::NeedsSelectorFlags::No,
            selectors::matching::MatchingForInvalidation::No,
        );
        context.extra_data = PseudoClassState::default();
        for rule in stylesheet {
            for selector in rule.selectors.slice() {
                if matches_selector(
                    selector,
                    0,
                    None,
                    node,
                    &mut context,
                ) {
                    debug!(selector = ?selector, "Selector matched");
                    matched.push(MatchedRule {
                        specificity: selector.specificity(),
                        rule,
                    });
                    break;
                }
            }
        }
        matched.sort_by_key(|m| m.specificity);
        matched
    }

    pub fn compute_style(
        _node: &DomElement<'_>,
        matched_rules: &[MatchedRule<'_>],
        parent_style: &ComputedStyle,
    ) -> ComputedStyle {
        let mut computed = ComputedStyle {
            color: parent_style.color,
            font_size: parent_style.font_size,
            font_family: parent_style.font_family.clone(),
            font_weight: parent_style.font_weight,
            line_height: parent_style.line_height,
            visibility: parent_style.visibility,
            custom_properties: parent_style.custom_properties.clone(),
            ..ComputedStyle::default()
        };

        // First pass: accumulate custom properties
        for matched in matched_rules {
            for (k, v) in &matched.rule.style.custom_properties {
                computed.custom_properties.insert(k.clone(), v.clone());
            }
        }

        // Second pass: apply normal and resolve CSS variables
        for matched in matched_rules {
            let style = &matched.rule.style;
            if let Some(ref val) = style.display { computed.display = *val; }
            if let Some(ref val) = style.color { computed.color = *val; }
            if let Some(ref val) = style.background_color { computed.background_color = *val; }
            if let Some(ref val) = style.background_image { computed.background_image = Some(val.clone()); }
            if let Some(ref val) = style.border_color { computed.border_color = *val; }
            if let Some(ref val) = style.border_style { computed.border_style = *val; }
            if let Some(ref val) = style.font_size { computed.font_size = *val; }
            if let Some(ref val) = style.font_weight { computed.font_weight = *val; }
            if let Some(ref val) = style.font_family { computed.font_family = val.clone(); }
            if let Some(ref val) = style.line_height { computed.line_height = *val; }
            if let Some(ref val) = style.margin_top { computed.margin.top = *val; }
            if let Some(ref val) = style.margin_bottom { computed.margin.bottom = *val; }
            if let Some(ref val) = style.margin_left { computed.margin.left = *val; }
            if let Some(ref val) = style.margin_right { computed.margin.right = *val; }
            if let Some(ref val) = style.padding_top { computed.padding.top = *val; }
            if let Some(ref val) = style.padding_bottom { computed.padding.bottom = *val; }
            if let Some(ref val) = style.padding_left { computed.padding.left = *val; }
            if let Some(ref val) = style.padding_right { computed.padding.right = *val; }
            if let Some(ref val) = style.width { computed.width = Some(val.clone()); }
            if let Some(ref val) = style.height { computed.height = Some(val.clone()); }
            if let Some(ref val) = style.min_width { computed.min_width = Some(val.clone()); }
            if let Some(ref val) = style.min_height { computed.min_height = Some(val.clone()); }
            if let Some(ref val) = style.max_width { computed.max_width = Some(val.clone()); }
            if let Some(ref val) = style.max_height { computed.max_height = Some(val.clone()); }
            if let Some(ref val) = style.position { computed.position = *val; }
            if let Some(ref val) = style.border_radius_top_left { computed.border_radius.top = *val; }
            if let Some(ref val) = style.border_radius_top_right { computed.border_radius.right = *val; }
            if let Some(ref val) = style.border_radius_bottom_right { computed.border_radius.bottom = *val; }
            if let Some(ref val) = style.border_radius_bottom_left { computed.border_radius.left = *val; }
            if let Some(ref val) = style.border_top_width { computed.border_width.top = *val; }
            if let Some(ref val) = style.border_right_width { computed.border_width.right = *val; }
            if let Some(ref val) = style.border_bottom_width { computed.border_width.bottom = *val; }
            if let Some(ref val) = style.border_left_width { computed.border_width.left = *val; }
            if let Some(ref val) = style.box_shadow { computed.box_shadow = Some(val.clone()); }
            
            // Flex properties
            if let Some(ref val) = style.flex_direction { computed.flex_direction = *val; }
            if let Some(ref val) = style.align_items { computed.align_items = *val; }
            if let Some(ref val) = style.justify_content { computed.justify_content = *val; }

            // Grid properties
            if let Some(ref val) = style.grid_template_columns { computed.grid.template_columns = val.clone(); }
            if let Some(ref val) = style.grid_template_rows { computed.grid.template_rows = val.clone(); }
            if let Some(val) = style.grid_column_gap { computed.grid.column_gap = val; }
            if let Some(val) = style.grid_row_gap { computed.grid.row_gap = val; }
            if let Some(ref val) = style.grid_column_placement { computed.grid.column_placement = Some(val.clone()); }
            if let Some(ref val) = style.grid_row_placement { computed.grid.row_placement = Some(val.clone()); }
            if let Some(ref val) = style.transitions { computed.transitions = val.clone(); }

            for (k, v) in &matched.rule.style.raw_properties {
                let mut resolved_val = v.clone();
                // Simple repeated replacement for var(--name)
                for (var_k, var_v) in &computed.custom_properties {
                    let pattern = format!("var({})", var_k);
                    resolved_val = resolved_val.replace(&pattern, var_v);
                }
                
                // If it STILL contains var(), fallback to transparent to avoid parse panic
                if resolved_val.contains("var(") {
                    resolved_val = "transparent".to_string();
                }

                // Apply the resolved value via a dummy SparseStyle
                let mut dummy = SparseStyle::default();
                crate::style_engine::apply_property_to_sparse(&mut dummy, k, &resolved_val);
                
                // Map dummy results back to computed
                if let Some(ref val) = dummy.display { computed.display = *val; }
                if let Some(ref val) = dummy.color { computed.color = *val; }
                if let Some(ref val) = dummy.background_color { computed.background_color = *val; }
                if let Some(ref val) = dummy.background_image { computed.background_image = Some(val.clone()); }
                if let Some(ref val) = dummy.border_color { computed.border_color = *val; }
                if let Some(ref val) = dummy.border_style { computed.border_style = *val; }
                if let Some(ref val) = dummy.font_size { computed.font_size = *val; }
                if let Some(ref val) = dummy.font_weight { computed.font_weight = *val; }
                if let Some(ref val) = dummy.font_family { computed.font_family = val.clone(); }
                if let Some(ref val) = dummy.line_height { computed.line_height = *val; }
                if let Some(ref val) = dummy.margin_top { computed.margin.top = *val; }
                if let Some(ref val) = dummy.margin_bottom { computed.margin.bottom = *val; }
                if let Some(ref val) = dummy.margin_left { computed.margin.left = *val; }
                if let Some(ref val) = dummy.margin_right { computed.margin.right = *val; }
                if let Some(ref val) = dummy.padding_top { computed.padding.top = *val; }
                if let Some(ref val) = dummy.padding_bottom { computed.padding.bottom = *val; }
                if let Some(ref val) = dummy.padding_left { computed.padding.left = *val; }
                if let Some(ref val) = dummy.padding_right { computed.padding.right = *val; }
                if let Some(ref val) = dummy.width { computed.width = Some(val.clone()); }
                if let Some(ref val) = dummy.height { computed.height = Some(val.clone()); }
                if let Some(ref val) = dummy.min_width { computed.min_width = Some(val.clone()); }
                if let Some(ref val) = dummy.max_width { computed.max_width = Some(val.clone()); }
                if let Some(ref val) = dummy.position { computed.position = *val; }
                if let Some(ref val) = dummy.border_radius_top_left { computed.border_radius.top = *val; }
                if let Some(ref val) = dummy.border_top_width { computed.border_width.top = *val; }
                if let Some(ref val) = dummy.border_right_width { computed.border_width.right = *val; }
                if let Some(ref val) = dummy.border_bottom_width { computed.border_width.bottom = *val; }
                if let Some(ref val) = dummy.border_left_width { computed.border_width.left = *val; }
                if let Some(ref val) = dummy.box_shadow { computed.box_shadow = Some(val.clone()); }
                if let Some(ref val) = dummy.flex_direction { computed.flex_direction = *val; }
                if let Some(ref val) = dummy.align_items { computed.align_items = *val; }
                if let Some(ref val) = dummy.justify_content { computed.justify_content = *val; }
            }
        }

        computed
    }
}

pub fn query_all(selector: &str, root: NodeId, tree: &DomTree, state: &PseudoClassState) -> Vec<NodeId> {
    let mut parser_input = cssparser::ParserInput::new(selector);
    let mut parser = cssparser::Parser::new(&mut parser_input);
    let selector_list = match SelectorList::parse(&SelectorParser, &mut parser, selectors::parser::ParseRelative::No) {
        Ok(list) => list,
        Err(_) => return vec![],
    };

    let mut nth_cache = Default::default();
    let mut context = MatchingContext::new(
        selectors::matching::MatchingMode::Normal,
        None,
        &mut nth_cache,
        selectors::context::QuirksMode::NoQuirks,
        selectors::matching::NeedsSelectorFlags::No,
        selectors::matching::MatchingForInvalidation::No,
    );
    context.extra_data = state.clone();

    let mut results = Vec::new();
    
    // We just check every node in the DOM to see if it matches the selector
    // and if it is a descendant (or self) of `root`.
    // For a real browser we'd walk from `root` down but a full tree scan is fine for MVP.
    fn is_descendant(tree: &DomTree, mut node: NodeId, parent: NodeId) -> bool {
        if node == parent { return true; }
        while let Some(n) = tree.get_node(node) {
            if let Some(p) = n.parent {
                if p == parent { return true; }
                node = p;
            } else {
                break;
            }
        }
        false
    }

    for node in tree.nodes() {
        if matches!(node.node_type, NodeType::Element(_)) {
            if !is_descendant(tree, node.id, root) {
                continue;
            }
            let el = DomElement { tree, id: node.id };
            for sel in selector_list.slice() {
                if matches_selector(sel, 0, None, &el, &mut context) {
                    debug!(selector = ?sel, "Selector matched");
                    results.push(node.id);
                    break;
                }
            }
        }
    }
    
    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use kitsune_html::dom::{DomTree, NodeType, ElementData, NodeId};
    use std::collections::HashMap;

    #[test]
    fn test_direct_child_selector() {
        let mut tree = DomTree::new();
        let root = tree.create_document();
        let div = tree.create_element("div");
        let p1 = tree.create_element("p"); // child of div
        let p2 = tree.create_element("p"); // child of body (not div)
        let body = tree.create_element("body");
        
        tree.append_child(root, body);
        tree.append_child(body, div);
        tree.append_child(div, p1);
        tree.append_child(body, p2);

        let state = PseudoClassState { hovered: None, focused: None, active: None };
        let results = query_all("div > p", root, &tree, &state);
        // Only p1 should match
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], p1);
    }

    #[test]
    fn test_pseudo_class_hover() {
        let mut tree = DomTree::new();
        let root = tree.create_document();
        let a = tree.create_element("a");
        tree.append_child(root, a);

        let sel = "a:hover";
        let state_unhovered = PseudoClassState { hovered: None, focused: None, active: None };
        let results = query_all(sel, root, &tree, &state_unhovered);
        assert!(results.is_empty());

        let state_hovered = PseudoClassState { hovered: Some(a), focused: None, active: None };
        let results2 = query_all(sel, root, &tree, &state_hovered);
        assert_eq!(results2.len(), 1);
        assert_eq!(results2[0], a);
    }

    #[test]
    fn test_attribute_selector() {
        let mut tree = DomTree::new();
        let root = tree.create_document();
        let mut attrs = HashMap::new();
        attrs.insert("data-type".to_string(), "buy".to_string());
        let el = tree.create_element_with_attrs("button", attrs);
        tree.append_child(root, el);

        let mut attrs2 = HashMap::new();
        attrs2.insert("data-type".to_string(), "sell".to_string());
        let el2 = tree.create_element_with_attrs("button", attrs2);
        tree.append_child(root, el2);

        let state = PseudoClassState { hovered: None, focused: None, active: None };
        let results = query_all("[data-type=\"buy\"]", root, &tree, &state);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], el);
    }

    #[test]
    fn test_specificity() {
        // Build a rule set and match it against an element with id, class, and tag
        let css = r#"
            div { color: red; }
            .my-class { color: green; }
            #my-id { color: blue; }
        "#;
        let rules = SelectorEngine::parse_stylesheet(css);

        let mut tree = DomTree::new();
        let mut attrs = HashMap::new();
        attrs.insert("id".to_string(), "my-id".to_string());
        attrs.insert("class".to_string(), "my-class".to_string());
        let el_node = tree.create_element_with_attrs("div", attrs);
        let doc = tree.create_document();
        tree.append_child(doc, el_node);

        let el = DomElement { tree: &tree, id: el_node };
        let matched = SelectorEngine::match_rules(&el, &rules);

        // specificity output requires #my-id matches with highest specificity
        assert_eq!(matched.len(), 3);
        // Sorted ascending by specificity. Last one should be blue (id)
        assert_eq!(matched[0].rule.style.color, Some(CssColor::rgb(255, 0, 0))); // div (red)
        assert_eq!(matched[1].rule.style.color, Some(CssColor::rgb(0, 128, 0))); // .my-class (green)
        assert_eq!(matched[2].rule.style.color, Some(CssColor::rgb(0, 0, 255))); // #my-id (blue)
    }
}
