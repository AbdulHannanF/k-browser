/// Box model implementation.
use crate::LayoutRect;
use kitsune_css::BoxEdges;
use serde::{Deserialize, Serialize};

/// The CSS box model dimensions for a layout box.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct BoxDimensions {
    pub content: LayoutRect,
    pub padding: BoxEdges,
    pub border: BoxEdges,
    pub margin: BoxEdges,
}

impl BoxDimensions {
    /// Get the padding box (content + padding).
    pub fn padding_box(&self) -> LayoutRect {
        LayoutRect {
            x: self.content.x - self.padding.left,
            y: self.content.y - self.padding.top,
            width: self.content.width + self.padding.left + self.padding.right,
            height: self.content.height + self.padding.top + self.padding.bottom,
        }
    }

    /// Get the border box (content + padding + border).
    pub fn border_box(&self) -> LayoutRect {
        let padding = self.padding_box();
        LayoutRect {
            x: padding.x - self.border.left,
            y: padding.y - self.border.top,
            width: padding.width + self.border.left + self.border.right,
            height: padding.height + self.border.top + self.border.bottom,
        }
    }

    /// Get the margin box (content + padding + border + margin).
    pub fn margin_box(&self) -> LayoutRect {
        let border = self.border_box();
        LayoutRect {
            x: border.x - self.margin.left,
            y: border.y - self.margin.top,
            width: border.width + self.margin.left + self.margin.right,
            height: border.height + self.margin.top + self.margin.bottom,
        }
    }
}
