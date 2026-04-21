/// CSS value parsing and manipulation.

use crate::{CssColor, CssUnit, CssValue, EasingFunction};

/// Parse a CSS value string into a CssValue.
pub fn parse_value(input: &str) -> Option<CssValue> {
    let trimmed = input.trim();

    // Try color
    if let Some(color) = parse_color(trimmed) {
        return Some(CssValue::Color(color));
    }

    // Try length
    if let Some((num, unit)) = parse_length(trimmed) {
        return Some(CssValue::Length(num, unit));
    }

    // Try percentage
    if trimmed.ends_with('%') {
        if let Ok(num) = trimmed.trim_end_matches('%').parse::<f64>() {
            return Some(CssValue::Percentage(num));
        }
    }

    // Try number
    if let Ok(num) = trimmed.parse::<f64>() {
        return Some(CssValue::Number(num));
    }

    // Keyword
    Some(CssValue::Keyword(trimmed.to_string()))
}

fn parse_length(input: &str) -> Option<(f64, CssUnit)> {
    let units = [
        ("px", CssUnit::Px),
        ("em", CssUnit::Em),
        ("rem", CssUnit::Rem),
        ("vh", CssUnit::Vh),
        ("vw", CssUnit::Vw),
    ];

    for (suffix, unit) in &units {
        if let Some(num_str) = input.strip_suffix(suffix) {
            if let Ok(num) = num_str.parse::<f64>() {
                return Some((num, *unit));
            }
        }
    }
    None
}

fn parse_color(input: &str) -> Option<CssColor> {
    if let Some(hex) = input.strip_prefix('#') {
        match hex.len() {
            3 => {
                let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
                let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
                let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
                Some(CssColor::rgb(r, g, b))
            }
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                Some(CssColor::rgb(r, g, b))
            }
            _ => None,
        }
    } else {
        match input {
            "black" => Some(CssColor::black()),
            "white" => Some(CssColor::white()),
            "transparent" => Some(CssColor::transparent()),
            _ => None,
        }
    }
}

/// Parse a duration string like "0.5s" or "500ms" into seconds.
pub fn parse_duration(input: &str) -> Option<f32> {
    if let Some(sec_str) = input.strip_suffix("ms") {
        return sec_str.parse::<f32>().ok().map(|ms| ms / 1000.0);
    }
    if let Some(sec_str) = input.strip_suffix('s') {
        return sec_str.parse::<f32>().ok();
    }
    None
}

/// Parse an easing function keyword.
pub fn parse_easing(input: &str) -> Option<EasingFunction> {
    match input.trim() {
        "linear" => Some(EasingFunction::Linear),
        "ease" => Some(EasingFunction::Ease),
        "ease-in" => Some(EasingFunction::EaseIn),
        "ease-out" => Some(EasingFunction::EaseOut),
        _ => None,
    }
}

/// Parse a URL from url() function.
pub fn parse_url(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if let Some(rest) = trimmed.strip_prefix("url(") {
        if let Some(inner) = rest.strip_suffix(')') {
            let inner_trimmed = inner.trim();
            // Handle quotes
            if inner_trimmed.starts_with('"') && inner_trimmed.ends_with('"') {
                return Some(inner_trimmed[1..inner_trimmed.len() - 1].to_string());
            }
            if inner_trimmed.starts_with('\'') && inner_trimmed.ends_with('\'') {
                return Some(inner_trimmed[1..inner_trimmed.len() - 1].to_string());
            }
            return Some(inner_trimmed.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_length() {
        let val = parse_value("16px").unwrap();
        assert!(matches!(val, CssValue::Length(16.0, CssUnit::Px)));
    }

    #[test]
    fn test_parse_color() {
        let val = parse_value("#ff0000").unwrap();
        if let CssValue::Color(c) = val {
            assert_eq!(c.r, 255);
            assert_eq!(c.g, 0);
            assert_eq!(c.b, 0);
        }
    }

    #[test]
    fn test_parse_keyword() {
        let val = parse_value("auto").unwrap();
        assert!(matches!(val, CssValue::Keyword(k) if k == "auto"));
    }
}
