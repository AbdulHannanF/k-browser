/// CSS stylesheet representation.
use crate::CssValue;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stylesheet {
    pub rules: Vec<CssRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CssRule {
    pub selectors: Vec<Selector>,
    pub declarations: Vec<Declaration>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Selector {
    pub specificity: Specificity,
    pub components: Vec<SelectorComponent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SelectorComponent {
    Tag(String),
    Class(String),
    Id(String),
    Universal,
    Descendant,
    Child,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct Specificity {
    pub ids: u32,
    pub classes: u32,
    pub elements: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Declaration {
    pub property: String,
    pub value: CssValue,
    pub important: bool,
}
