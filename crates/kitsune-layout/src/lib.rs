// ARCHITECTURE: kitsune-layout implements the box model, flexbox, and grid layout.
// It takes a styled DOM tree and produces a layout tree with computed positions
// and dimensions for each element.

pub mod box_model;
pub mod flex;
pub mod grid;
pub mod layout_tree;
pub mod engine;

pub use box_model::*;
pub use layout_tree::*;

use serde::{Deserialize, Serialize};

/// A rectangle in layout coordinates.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct LayoutRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl LayoutRect {
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self { x, y, width, height }
    }

    pub fn contains_point(&self, px: f64, py: f64) -> bool {
        px >= self.x && px <= self.x + self.width && py >= self.y && py <= self.y + self.height
    }

    pub fn intersects(&self, other: &LayoutRect) -> bool {
        self.x < other.x + other.width
            && self.x + self.width > other.x
            && self.y < other.y + other.height
            && self.y + self.height > other.y
    }
}

/// Context passed to Taffy's measurement closure to provide text and font information.
#[derive(Debug, Clone, Default)]
pub struct TextContext {
    pub text: String,
    pub font_size: f32,
}

