use crate::pipelines::{RectPipeline, TexturePipeline};
use crate::DisplayList;

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use image::GenericImageView;
use wgpu::util::DeviceExt;

pub struct ImageCache {
    pub textures: HashMap<u64, (wgpu::Texture, wgpu::BindGroup)>,
}

impl ImageCache {
    pub fn new() -> Self {
        Self { textures: HashMap::new() }
    }
}

pub struct WgpuBackend {
    device: wgpu::Device,
    queue: wgpu::Queue,
    rect_pipeline: RectPipeline,
    texture_pipeline: TexturePipeline,
    image_cache: ImageCache,
    page_texture: wgpu::Texture,
    page_texture_view: wgpu::TextureView,
    dirty: bool,
}

impl WgpuBackend {
    #[cfg(test)]
    pub async fn new_headless() -> Self {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::GL,
            ..Default::default()
        });

        let adapter = instance.request_adapter(
            &wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: None,
                force_fallback_adapter: false,
            },
        ).await.unwrap();

        let (device, queue) = adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_webgl2_defaults(),
                memory_hints: wgpu::MemoryHints::default(),
            },
            None,
        ).await.unwrap();

        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let rect_pipeline = RectPipeline::new(&device, format);
        let texture_pipeline = TexturePipeline::new(&device, format);

        let size = wgpu::Extent3d {
            width: 800,
            height: 600,
            depth_or_array_layers: 1,
        };
        let page_texture = device.create_texture(&wgpu::TextureDescriptor {
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            label: Some("page_texture"),
            view_formats: &[],
        });
        let page_texture_view = page_texture.create_view(&wgpu::TextureViewDescriptor::default());

        Self {
            device,
            queue,
            rect_pipeline,
            texture_pipeline,
            image_cache: ImageCache::new(),
            page_texture,
            page_texture_view,
            dirty: true,
        }
    }

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    pub fn render_frame(&mut self, display_list: &DisplayList, width: f32, height: f32) {
        if !self.dirty {
            // We still might want to re-render if the size changed, but for now we assume dirty handles it
            // return;
        }

        let to_ndc = |x: f32, y: f32| -> [f32; 2] {
            [
                (x / width) * 2.0 - 1.0,
                1.0 - (y / height) * 2.0,
            ]
        };

        // 1. Pre-process image commands to ensure all images are in cache
        for command in &display_list.commands {
            if let crate::RenderCommand::DrawImage { image_data, .. } = command {
                let mut s = std::collections::hash_map::DefaultHasher::new();
                image_data.hash(&mut s);
                let hash = s.finish();

                if !self.image_cache.textures.contains_key(&hash) {
                    if let Ok(img) = image::load_from_memory(image_data) {
                        let rgba = img.to_rgba8();
                        let (iw, ih) = img.dimensions();
                        let size = wgpu::Extent3d { width: iw, height: ih, depth_or_array_layers: 1 };
                        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                            label: None, size, mip_level_count: 1, sample_count: 1,
                            dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Rgba8UnormSrgb,
                            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                            view_formats: &[],
                        });
                        self.queue.write_texture(
                            wgpu::ImageCopyTexture { texture: &texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                            &rgba, wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(4 * iw), rows_per_image: Some(ih) },
                            size,
                        );
                        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                        let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor::default());
                        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                            layout: &self.texture_pipeline.bind_group_layout,
                            entries: &[
                                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
                            ],
                            label: None,
                        });
                        self.image_cache.textures.insert(hash, (texture, bind_group));
                    }
                }
            }
        }

        // 2. Now we can borrow fields disjointly for the rest of the function
        let WgpuBackend {
            ref device,
            ref queue,
            ref rect_pipeline,
            ref texture_pipeline,
            ref image_cache,
            ref page_texture_view,
            ..
        } = *self;

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render Encoder"),
        });

        let mut rect_vertices = Vec::new();
        struct ImageDraw {
            buffer: wgpu::Buffer,
            hash: u64,
        }
        let mut image_draws = Vec::new();

        for command in &display_list.commands {
            match command {
                crate::RenderCommand::FillRect { x, y, width: w, height: h, color } => {
                    let p1 = to_ndc(*x, *y);
                    let p2 = to_ndc(x + w, y + h);
                    rect_vertices.push(crate::pipelines::RectVertex { position: [p1[0], p1[1]], color: *color });
                    rect_vertices.push(crate::pipelines::RectVertex { position: [p2[0], p1[1]], color: *color });
                    rect_vertices.push(crate::pipelines::RectVertex { position: [p1[0], p2[1]], color: *color });
                    rect_vertices.push(crate::pipelines::RectVertex { position: [p2[0], p1[1]], color: *color });
                    rect_vertices.push(crate::pipelines::RectVertex { position: [p2[0], p2[1]], color: *color });
                    rect_vertices.push(crate::pipelines::RectVertex { position: [p1[0], p2[1]], color: *color });
                }
                crate::RenderCommand::DrawBorder { x, y, width: w, height: h, border_width: bw, color } => {
                    let mut draw_rect = |rx, ry, rw, rh| {
                        let p1 = to_ndc(rx, ry);
                        let p2 = to_ndc(rx + rw, ry + rh);
                        rect_vertices.push(crate::pipelines::RectVertex { position: [p1[0], p1[1]], color: *color });
                        rect_vertices.push(crate::pipelines::RectVertex { position: [p2[0], p1[1]], color: *color });
                        rect_vertices.push(crate::pipelines::RectVertex { position: [p1[0], p2[1]], color: *color });
                        rect_vertices.push(crate::pipelines::RectVertex { position: [p2[0], p1[1]], color: *color });
                        rect_vertices.push(crate::pipelines::RectVertex { position: [p2[0], p2[1]], color: *color });
                        rect_vertices.push(crate::pipelines::RectVertex { position: [p1[0], p2[1]], color: *color });
                    };
                    draw_rect(*x, *y, *w, *bw);
                    draw_rect(*x, y + h - bw, *w, *bw);
                    draw_rect(*x, *y, *bw, *h);
                    draw_rect(x + w - bw, *y, *bw, *h);
                }
                crate::RenderCommand::DrawImage { x, y, width: w, height: h, image_data } => {
                    let mut s = std::collections::hash_map::DefaultHasher::new();
                    image_data.hash(&mut s);
                    let hash = s.finish();

                    let p1 = to_ndc(*x, *y);
                    let p2 = to_ndc(x + w, y + h);
                    let vertices = vec![
                        crate::pipelines::TextureVertex { position: [p1[0], p1[1]], uv: [0.0, 0.0] },
                        crate::pipelines::TextureVertex { position: [p2[0], p1[1]], uv: [1.0, 0.0] },
                        crate::pipelines::TextureVertex { position: [p1[0], p2[1]], uv: [0.0, 1.0] },
                        crate::pipelines::TextureVertex { position: [p2[0], p1[1]], uv: [1.0, 0.0] },
                        crate::pipelines::TextureVertex { position: [p2[0], p2[1]], uv: [1.0, 1.0] },
                        crate::pipelines::TextureVertex { position: [p1[0], p2[1]], uv: [0.0, 1.0] },
                    ];
                    let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Temp Image Buffer"),
                        contents: bytemuck::cast_slice(&vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                    image_draws.push(ImageDraw { buffer, hash });
                }
                _ => {}
            }
        }

        let rect_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Rect Buffer"),
            contents: bytemuck::cast_slice(&rect_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: page_texture_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: 1.0, g: 1.0, b: 1.0, a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            if !rect_vertices.is_empty() {
                render_pass.set_pipeline(&rect_pipeline.pipeline);
                render_pass.set_vertex_buffer(0, rect_buffer.slice(..));
                render_pass.draw(0..rect_vertices.len() as u32, 0..1);
            }

            render_pass.set_pipeline(&texture_pipeline.pipeline);
            for draw in &image_draws {
                if let Some((_, bind_group)) = image_cache.textures.get(&draw.hash) {
                    render_pass.set_bind_group(0, bind_group, &[]);
                    render_pass.set_vertex_buffer(0, draw.buffer.slice(..));
                    render_pass.draw(0..6, 0..1);
                }
            }
        }

        queue.submit(std::iter::once(encoder.finish()));
        self.dirty = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_headless() {
        let mut backend = WgpuBackend::new_headless().await;
        backend.render_frame(&DisplayList { commands: vec![] }, 800.0, 600.0);
        assert!(!backend.dirty);
        backend.mark_dirty();
        assert!(backend.dirty);
    }
}
