use crate::{DisplayList, RenderCommand};
use kitsune_layout::LayoutNode;
use kitsune_css::DisplayType;
use std::time::Instant;
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;
use std::cell::RefCell;
use url::Url;

thread_local! {
    static LAST_FRAME: RefCell<Option<(u64, DisplayList)>> = RefCell::new(None);
}

fn hash_node(node: &LayoutNode, state: &mut DefaultHasher) {
    node.dom_node_id.hash(state);
    node.dimensions.content.x.to_bits().hash(state);
    node.dimensions.content.y.to_bits().hash(state);
    node.dimensions.content.width.to_bits().hash(state);
    node.dimensions.content.height.to_bits().hash(state);
    if let Some(ref t) = node.text {
        t.hash(state);
    }
    for c in &node.children {
        hash_node(c, state);
    }
}

/// Paint a layout tree into a display list.
pub fn paint(
    root: &LayoutNode,
    resource_map: &std::collections::HashMap<String, Vec<u8>>,
    base_url: &url::Url,
) -> DisplayList {
    let start = Instant::now();
    let _span = tracing::info_span!("pipeline::paint").entered();
    
    let mut hasher = DefaultHasher::new();
    hash_node(root, &mut hasher);
    let current_hash = hasher.finish();
    
    if let Some((old_hash, old_list)) = LAST_FRAME.with(|f| {
        let b = f.borrow();
        b.as_ref().map(|x| (x.0, DisplayList { commands: x.1.commands.clone() }))
    }) {
        if old_hash == current_hash {
            tracing::info!("Paint cache hit");
            return old_list;
        }
    }

    let mut list = DisplayList::new();
    
    // Inject viewport background (clear color) based on root/body style
    let clear_color = root.style.background_color;
    if clear_color.a > 0.0 {
        list.push(RenderCommand::FillRect {
            x: 0.0, y: 0.0, width: 4000.0, height: 4000.0, // Large enough to cover all viewports
            color: [clear_color.r as f32 / 255.0, clear_color.g as f32 / 255.0, clear_color.b as f32 / 255.0, clear_color.a],
        });
    }

    paint_node(root, &mut list, resource_map, base_url);
    tracing::info!(commands = list.len(), "Display list built");
    
    LAST_FRAME.with(|f| *f.borrow_mut() = Some((current_hash, DisplayList { commands: list.commands.clone() })));

    let elapsed = start.elapsed().as_millis();
    if elapsed > 16 {
        tracing::warn!("Slow frame: {}ms", elapsed);
    }
    
    list
}

fn paint_node(
    node: &LayoutNode,
    list: &mut DisplayList,
    resource_map: &std::collections::HashMap<String, Vec<u8>>,
    base_url: &url::Url,
) {
    // Skip display: none
    if matches!(node.style.display, DisplayType::None) {
        return;
    }

    let dim = &node.dimensions;
    let content = &dim.content;

    // Calculate absolute position
    let x = node.absolute_x.unwrap_or(content.x);
    let y = node.absolute_y.unwrap_or(content.y);

    // Skip nodes with zero area except if they have children or text
    if content.width <= 0.0 || content.height <= 0.0 {
        if node.text.is_some() && !node.text.as_ref().unwrap().is_empty() {
            // we have text, we should probably paint it even if width/height are 0
        } else {
            // Still process children (they might be positioned outside)
            for child in &node.children {
                paint_node(child, list, resource_map, base_url);
            }
            return;
        }
    }

    // 1. Paint background color
    let bg = &node.style.background_color;
    if bg.a > 0.0 {
        list.push(RenderCommand::FillRect {
            x: x as f32,
            y: y as f32,
            width: content.width as f32,
            height: content.height as f32,
            color: [bg.r as f32 / 255.0, bg.g as f32 / 255.0, bg.b as f32 / 255.0, bg.a],
        });
    }

    // 2. Paint background image (CSS)
    if let Some(ref bg_url) = node.style.background_image {
        if let Ok(u) = base_url.join(bg_url) {
            if let Some(data) = resource_map.get(u.as_str()) {
                list.push(RenderCommand::DrawImage {
                    x: x as f32,
                    y: y as f32,
                    width: content.width as f32,
                    height: content.height as f32,
                    image_data: data.clone(),
                });
            } else {
                tracing::debug!(url = %u, "Background image resource not found in map");
            }
        } else {
            tracing::warn!(url = %bg_url, "Failed to join background image URL");
        }
    }

    // 3. Paint <img> data
    if node.tag == "img" {
        if let Some(src) = node.attributes.get("src") {
            if let Ok(u) = base_url.join(src) {
                if let Some(data) = resource_map.get(u.as_str()) {
                    list.push(RenderCommand::DrawImage {
                        x: x as f32,
                        y: y as f32,
                        width: content.width as f32,
                        height: content.height as f32,
                        image_data: data.clone(),
                    });
                } else {
                    tracing::debug!(url = %u, "Image resource not found in map");
                }
            } else {
                tracing::warn!(url = %src, "Failed to join image URL");
            }
        }
    }

    // 4. Paint border
    if node.style.border_style != kitsune_css::BorderStyle::None {
        let bw = node.style.border_width.top as f32; // simplify for now
        let bc = &node.style.border_color;
        if bw > 0.0 {
            list.push(RenderCommand::DrawBorder {
                x: x as f32,
                y: y as f32,
                width: content.width as f32,
                height: content.height as f32,
                border_width: bw,
                color: [bc.r as f32 / 255.0, bc.g as f32 / 255.0, bc.b as f32 / 255.0, bc.a],
            });
        }
    }

    // Paint text
    if let Some(ref text) = node.text {
        if !text.is_empty() {
            let fg = &node.style.color;
            list.push(RenderCommand::DrawText {
                x: x as f32,
                y: y as f32,
                text: text.clone(),
                font_size: node.style.font_size as f32,
                color: [fg.r as f32 / 255.0, fg.g as f32 / 255.0, fg.b as f32 / 255.0, fg.a],
            });
        }
    }

    // Paint children
    for child in &node.children {
        paint_node(child, list, resource_map, base_url);
    }
}

pub fn paint_highlights(highlights: &mut Vec<kitsune_ipc::message::DomHighlight>, list: &mut DisplayList) {
    let now = Instant::now();
    
    highlights.retain_mut(|h| {
        let start = h.phase_start.unwrap_or(now);
        let elapsed = now.duration_since(start).as_secs_f32();
        
        let mut alpha: f32 = 0.0;
        
        match h.phase {
            kitsune_ipc::message::HighlightPhase::FadingIn => {
                alpha = (elapsed / 0.2).clamp(0.0, 1.0);
                if elapsed >= 0.2 {
                    if h.style == kitsune_ipc::message::HighlightStyle::Reading {
                        h.phase = kitsune_ipc::message::HighlightPhase::Pulsing;
                    } else {
                        h.phase = kitsune_ipc::message::HighlightPhase::Active;
                    }
                    h.phase_start = Some(now);
                }
            }
            kitsune_ipc::message::HighlightPhase::Active => {
                alpha = 1.0;
            }
            kitsune_ipc::message::HighlightPhase::Pulsing => {
                // Pulse: amplitude 0.15 + 0.05 * sin(time * 3.14)
                let pulse = 0.15 + 0.05 * (elapsed * 3.14).sin();
                // Map the base alpha down a bit when pulsing so the pulse is visible
                alpha = 1.0 - pulse;
            }
            kitsune_ipc::message::HighlightPhase::FadingOut => {
                alpha = (1.0 - (elapsed / 0.2)).clamp(0.0, 1.0);
                if elapsed >= 0.2 {
                    return false; // Remove completed fade out
                }
            }
        }
        
        let base_alpha = alpha;
        
        match h.style {
            kitsune_ipc::message::HighlightStyle::Reading => {
                // yellow rgba(251,191,36,0.15) fill + 2px dashed border
                list.push(RenderCommand::FillRect {
                    x: h.rect.x,
                    y: h.rect.y,
                    width: h.rect.width,
                    height: h.rect.height,
                    color: [251.0/255.0, 191.0/255.0, 36.0/255.0, 0.15 * base_alpha],
                });
                list.push(RenderCommand::DrawBorder {
                    x: h.rect.x,
                    y: h.rect.y,
                    width: h.rect.width,
                    height: h.rect.height,
                    border_width: 2.0,
                    color: [251.0/255.0, 191.0/255.0, 36.0/255.0, 1.0 * base_alpha],
                });
            }
            kitsune_ipc::message::HighlightStyle::Acting => {
                // blue rgba(96,165,250,0.2) fill + 2px solid border
                list.push(RenderCommand::FillRect {
                    x: h.rect.x,
                    y: h.rect.y,
                    width: h.rect.width,
                    height: h.rect.height,
                    color: [96.0/255.0, 165.0/255.0, 250.0/255.0, 0.2 * base_alpha],
                });
                list.push(RenderCommand::DrawBorder {
                    x: h.rect.x,
                    y: h.rect.y,
                    width: h.rect.width,
                    height: h.rect.height,
                    border_width: 2.0,
                    color: [96.0/255.0, 165.0/255.0, 250.0/255.0, 1.0 * base_alpha],
                });
            }
            kitsune_ipc::message::HighlightStyle::Done => {
                // green rgba(74,222,128,0.15) fill + 2px solid border
                list.push(RenderCommand::FillRect {
                    x: h.rect.x,
                    y: h.rect.y,
                    width: h.rect.width,
                    height: h.rect.height,
                    color: [74.0/255.0, 222.0/255.0, 128.0/255.0, 0.15 * base_alpha],
                });
                list.push(RenderCommand::DrawBorder {
                    x: h.rect.x,
                    y: h.rect.y,
                    width: h.rect.width,
                    height: h.rect.height,
                    border_width: 2.0,
                    color: [74.0/255.0, 222.0/255.0, 128.0/255.0, 1.0 * base_alpha],
                });
            }
        }
        
        true
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use kitsune_ipc::message::{DomHighlight, HighlightRect, HighlightStyle, HighlightPhase, IpcMessage, IpcPayload, ProcessId};
    use std::time::Duration;

    fn mock_highlight(style: HighlightStyle, phase: HighlightPhase, age_ms: u64) -> DomHighlight {
        DomHighlight {
            element_id: "test".into(),
            rect: HighlightRect { x: 0.0, y: 0.0, width: 100.0, height: 100.0 },
            style,
            phase,
            phase_start: Some(Instant::now() - Duration::from_millis(age_ms)),
        }
    }

    #[test]
    fn test_highlight_fade_in_alpha() {
        let mut highlights = vec![mock_highlight(HighlightStyle::Reading, HighlightPhase::FadingIn, 100)];
        let mut list = DisplayList::new();
        paint_highlights(&mut highlights, &mut list);
        
        let mut found_alpha = 0.0;
        for c in &list.commands {
            if let RenderCommand::FillRect { color, .. } = c {
                found_alpha = color[3]; // The alpha channel is multiplied
                // Reading base alpha is 0.15 * math alpha
                // Math alpha at 100ms / 200ms = 0.5
                break;
            }
        }
        
        assert!((found_alpha - (0.15 * 0.5)).abs() < 0.05);
    }

    #[test]
    fn test_highlight_done_auto_removes() {
        // FadingOut at >200ms
        let mut highlights = vec![mock_highlight(HighlightStyle::Done, HighlightPhase::FadingOut, 250)];
        let mut list = DisplayList::new();
        paint_highlights(&mut highlights, &mut list);
        
        assert!(highlights.is_empty(), "Done highlight should be removed after fading out");
    }
}
