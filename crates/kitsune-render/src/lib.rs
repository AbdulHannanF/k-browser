// ARCHITECTURE: kitsune-render uses wgpu for GPU-accelerated rendering.
// It takes a layout tree from kitsune-layout and renders it to a surface.

pub mod backend;
pub mod images;
pub mod pipelines;
pub mod text;
pub mod pipeline;
pub mod painter;

/// A rendering command.
#[derive(Debug, Clone)]
pub enum RenderCommand {
    /// Fill a rectangle with a color.
    FillRect {
        x: f32, y: f32, width: f32, height: f32,
        color: [f32; 4],
    },
    /// Draw text.
    DrawText {
        x: f32, y: f32,
        text: String,
        font_size: f32,
        color: [f32; 4],
    },
    /// Draw a border.
    DrawBorder {
        x: f32, y: f32, width: f32, height: f32,
        border_width: f32,
        color: [f32; 4],
    },
    /// Draw an image.
    DrawImage {
        x: f32, y: f32, width: f32, height: f32,
        image_data: Vec<u8>,
    },
}

/// A display list — a flat list of rendering commands.
#[derive(Debug, Default)]
pub struct DisplayList {
    pub commands: Vec<RenderCommand>,
}

impl DisplayList {
    pub fn new() -> Self {
        Self { commands: Vec::new() }
    }

    pub fn push(&mut self, command: RenderCommand) {
        self.commands.push(command);
    }

    pub fn len(&self) -> usize {
        self.commands.len()
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}
