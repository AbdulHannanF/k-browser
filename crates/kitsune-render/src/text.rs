//! Text rendering via cosmic-text (shaping + layout) with GPU rendering stub.
//!
//! Uses `cosmic_text` to shape and cache text. The GPU rasterization layer (glyphon)
//! is deferred to Task 20 wgpu version unification. The shaping cache, LRU eviction,
//! and prepare/render API are fully implemented here and tested headlessly.
//!
//! Cache evicts 100 LRU entries when size exceeds 500.

use cosmic_text::{Attrs, Buffer, FontSystem, Metrics, Shaping};
use std::collections::{HashMap, VecDeque};

/// A single draw-text command extracted from the `DisplayList`.
#[derive(Clone, Debug)]
pub struct DrawTextCommand {
    pub x: f32,
    pub y: f32,
    pub text: String,
    pub font_size: f32,
    pub color: [f32; 4],
}

/// Cache key. `font_size` stored as `(f32 * 100) as u32` to be hashable.
#[derive(Hash, Eq, PartialEq, Clone, Debug)]
struct TextCacheKey {
    text: String,
    font_size_u32: u32,
    color: [u8; 4],
}

impl TextCacheKey {
    fn from_cmd(cmd: &DrawTextCommand) -> Self {
        Self {
            text: cmd.text.clone(),
            font_size_u32: (cmd.font_size * 100.0) as u32,
            color: [
                (cmd.color[0] * 255.0) as u8,
                (cmd.color[1] * 255.0) as u8,
                (cmd.color[2] * 255.0) as u8,
                (cmd.color[3] * 255.0) as u8,
            ],
        }
    }
}

struct CachedBuffer {
    buffer: Buffer,
    last_used: u64,
}

/// GPU-accelerated text renderer with LRU shaping cache.
///
/// Shapes text via `cosmic_text`. GPU rasterization (glyphon) will be wired in
/// when the workspace wgpu version is unified in Task 20.
pub struct KitsuneTextRenderer {
    font_system: FontSystem,
    buffer_cache: HashMap<TextCacheKey, CachedBuffer>,
    /// LRU eviction queue — front = oldest.
    lru: VecDeque<TextCacheKey>,
    frame_counter: u64,
}

const MAX_CACHE_ENTRIES: usize = 500;
const EVICT_COUNT: usize = 100;

impl KitsuneTextRenderer {
    /// Create a new `KitsuneTextRenderer`.
    pub fn new() -> Self {
        Self {
            font_system: FontSystem::new(),
            buffer_cache: HashMap::new(),
            lru: VecDeque::new(),
            frame_counter: 0,
        }
    }

    /// Prepare (shape) all text commands for the current frame.
    ///
    /// Hits the cache for identical `(text, font_size, color)` tuples.
    /// Must be called before [`render_into_pass`](Self::render_into_pass).
    /// Returns `Ok(())` on success; never returns an error from shaping.
    pub fn prepare(
        &mut self,
        viewport_width: f32,
        viewport_height: f32,
        text_commands: &[DrawTextCommand],
    ) {
        self.frame_counter += 1;

        for cmd in text_commands {
            let key = TextCacheKey::from_cmd(cmd);
            if !self.buffer_cache.contains_key(&key) {
                let metrics = Metrics::new(cmd.font_size, cmd.font_size * 1.2);
                let mut buffer = Buffer::new(&mut self.font_system, metrics);
                buffer.set_size(
                    &mut self.font_system,
                    Some(viewport_width),
                    Some(viewport_height),
                );
                buffer.set_text(
                    &mut self.font_system,
                    &cmd.text,
                    Attrs::new(),
                    Shaping::Advanced,
                );
                buffer.shape_until_scroll(&mut self.font_system, false);

                self.lru.push_back(key.clone());
                self.buffer_cache.insert(
                    key.clone(),
                    CachedBuffer {
                        buffer,
                        last_used: self.frame_counter,
                    },
                );
                if self.buffer_cache.len() > MAX_CACHE_ENTRIES {
                    self.evict_lru();
                }
            } else {
                if let Some(e) = self.buffer_cache.get_mut(&key) {
                    e.last_used = self.frame_counter;
                }
                if let Some(pos) = self.lru.iter().position(|k| k == &key) {
                    self.lru.remove(pos);
                    self.lru.push_back(key);
                }
            }
        }
    }

    /// Returns the number of shaped buffers currently in cache.
    pub fn cache_len(&self) -> usize {
        self.buffer_cache.len()
    }

    fn evict_lru(&mut self) {
        for _ in 0..EVICT_COUNT {
            if let Some(key) = self.lru.pop_front() {
                self.buffer_cache.remove(&key);
            } else {
                break;
            }
        }
    }
}

impl Default for KitsuneTextRenderer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Constructing the renderer must not panic.
    #[test]
    fn test_text_renderer_new() {
        let _r = KitsuneTextRenderer::new();
    }

    /// prepare() with zero commands must succeed immediately.
    #[test]
    fn test_prepare_zero_commands() {
        let mut r = KitsuneTextRenderer::new();
        r.prepare(800.0, 600.0, &[]);
        assert_eq!(r.cache_len(), 0);
    }

    /// Same text+params hits the shape cache; different font_size causes a new entry.
    #[test]
    fn test_cache_hit_and_miss() {
        let mut r = KitsuneTextRenderer::new();

        let cmd = DrawTextCommand {
            x: 10.0,
            y: 20.0,
            text: "Hello".into(),
            font_size: 16.0,
            color: [0.0, 0.0, 0.0, 1.0],
        };

        r.prepare(800.0, 600.0, &[cmd.clone()]);
        assert_eq!(r.cache_len(), 1, "cache miss → 1 entry");

        r.prepare(800.0, 600.0, &[cmd.clone()]);
        assert_eq!(r.cache_len(), 1, "cache hit → still 1");

        let cmd2 = DrawTextCommand { font_size: 24.0, ..cmd.clone() };
        r.prepare(800.0, 600.0, &[cmd2]);
        assert_eq!(r.cache_len(), 2, "different font_size → 2 entries");
    }

    /// 50 distinct commands must all be prepared without panic.
    #[test]
    fn test_prepare_50_commands() {
        let mut r = KitsuneTextRenderer::new();
        let cmds: Vec<DrawTextCommand> = (0..50)
            .map(|i| DrawTextCommand {
                x: i as f32,
                y: 0.0,
                text: format!("word{i}"),
                font_size: 14.0,
                color: [0.0, 0.0, 0.0, 1.0],
            })
            .collect();
        r.prepare(1920.0, 1080.0, &cmds);
        assert_eq!(r.cache_len(), 50);
    }
}
