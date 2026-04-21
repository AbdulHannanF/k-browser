/// CSS cascade resolution.
use crate::ComputedStyle;

/// Resolve the cascade for a set of matching rules.
pub fn resolve_cascade(styles: &[&ComputedStyle]) -> ComputedStyle {
    let mut result = ComputedStyle::default();
    for style in styles {
        // Later styles override earlier ones (simplified cascade)
        result = (*style).clone();
    }
    result
}
