//! GPU image cache for kitsune-render.
//!
//! Decodes raw image bytes with the `image` crate, uploads them to wgpu textures,
//! and manages an LRU memory budget (default 256 MB). Handles all common formats
//! (PNG, JPEG, GIF, WebP) transparently.
//!
//! # Usage
//! ```ignore
//! cache.upload(id, png_bytes, device, queue)?;
//! cache.touch(id);
//! if let Some(bind_group) = cache.get_bind_group(id) {
//!     // draw using texture pipeline
//! }
//! ```

use std::collections::{HashMap, VecDeque};
use std::time::Instant;

/// Opaque image identifier.
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub struct ImageId(pub u64);

struct ImageEntry {
    texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub bind_group: wgpu::BindGroup,
    size_bytes: usize,
    last_used: Instant,
}

/// GPU image cache with LRU eviction and a configurable byte budget.
pub struct ImageCache {
    entries: HashMap<ImageId, ImageEntry>,
    /// Total GPU memory committed to image textures.
    total_bytes: usize,
    /// Maximum GPU memory before LRU eviction (default 256 MB).
    max_bytes: usize,
    /// LRU order — front = least recently used.
    lru_order: VecDeque<ImageId>,
    /// Bind group layout compatible with `TexturePipeline`.
    pub bind_group_layout: wgpu::BindGroupLayout,
}

impl ImageCache {
    /// Create a new `ImageCache` with the provided texture bind group layout.
    ///
    /// The layout must have binding 0 = `texture_2d<f32>` and binding 1 = `sampler`.
    pub fn new(device: &wgpu::Device) -> Self {
        let bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("image_cache_bgl"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        Self {
            entries: HashMap::new(),
            total_bytes: 0,
            max_bytes: 256 * 1024 * 1024,
            lru_order: VecDeque::new(),
            bind_group_layout,
        }
    }

    /// Decode `data` and upload to GPU. No-op if `id` is already cached.
    ///
    /// Returns an error if the byte slice cannot be decoded as an image.
    pub fn upload(
        &mut self,
        id: ImageId,
        data: &[u8],
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<(), image::ImageError> {
        if self.entries.contains_key(&id) {
            return Ok(());
        }

        let img = image::load_from_memory(data)?.to_rgba8();
        let (width, height) = img.dimensions();
        let size_bytes = (width * height * 4) as usize;

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("image_cache_tex"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            img.as_raw(),
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(4 * width),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("image_cache_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("image_cache_bg"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        self.total_bytes += size_bytes;
        self.entries.insert(
            id,
            ImageEntry {
                texture,
                view,
                bind_group,
                size_bytes,
                last_used: Instant::now(),
            },
        );
        self.lru_order.push_back(id);
        self.evict_if_over_limit();
        Ok(())
    }

    /// Return the `BindGroup` for rendering, or `None` if `id` is not cached.
    pub fn get_bind_group(&self, id: ImageId) -> Option<&wgpu::BindGroup> {
        self.entries.get(&id).map(|e| &e.bind_group)
    }

    /// Mark `id` as recently used (moves to back of LRU queue).
    pub fn touch(&mut self, id: ImageId) {
        if let Some(e) = self.entries.get_mut(&id) {
            e.last_used = Instant::now();
        }
        if let Some(pos) = self.lru_order.iter().position(|k| *k == id) {
            self.lru_order.remove(pos);
            self.lru_order.push_back(id);
        }
    }

    /// Evict a specific entry from the cache.
    pub fn evict(&mut self, id: ImageId) {
        if let Some(entry) = self.entries.remove(&id) {
            self.total_bytes = self.total_bytes.saturating_sub(entry.size_bytes);
        }
        self.lru_order.retain(|k| *k != id);
    }

    /// Total GPU bytes currently used.
    pub fn total_bytes(&self) -> usize {
        self.total_bytes
    }

    fn evict_if_over_limit(&mut self) {
        while self.total_bytes > self.max_bytes {
            if let Some(oldest) = self.lru_order.pop_front() {
                if let Some(entry) = self.entries.remove(&oldest) {
                    self.total_bytes = self.total_bytes.saturating_sub(entry.size_bytes);
                }
            } else {
                break;
            }
        }
    }
}

/// Construct a minimal 1×1 RGBA PNG for use in tests.
#[cfg(test)]
pub fn make_test_png(r: u8, g: u8, b: u8) -> Vec<u8> {
    // Minimal valid 1×1 4-channel PNG
    let img = image::RgbaImage::from_pixel(1, 1, image::Rgba([r, g, b, 255]));
    let mut buf: Vec<u8> = Vec::new();
    img.write_to(
        &mut std::io::Cursor::new(&mut buf),
        image::ImageFormat::Png,
    )
    .unwrap();
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn headless_device() -> (wgpu::Device, wgpu::Queue) {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::GL,
            ..Default::default()
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .unwrap();
        adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::downlevel_webgl2_defaults(),
                    memory_hints: wgpu::MemoryHints::default(),
                },
                None,
            )
            .await
            .unwrap()
    }

    /// Upload a 1×1 red PNG — entry must exist in cache.
    #[tokio::test]
    async fn test_upload_creates_entry() {
        let (device, queue) = headless_device().await;
        let mut cache = ImageCache::new(&device);
        let png = make_test_png(255, 0, 0);
        cache.upload(ImageId(1), &png, &device, &queue).unwrap();
        assert!(cache.get_bind_group(ImageId(1)).is_some());
    }

    /// Uploading the same id twice is a no-op.
    #[tokio::test]
    async fn test_upload_idempotent() {
        let (device, queue) = headless_device().await;
        let mut cache = ImageCache::new(&device);
        let png = make_test_png(0, 255, 0);
        cache.upload(ImageId(2), &png, &device, &queue).unwrap();
        let bytes_before = cache.total_bytes();
        cache.upload(ImageId(2), &png, &device, &queue).unwrap(); // second call
        assert_eq!(cache.total_bytes(), bytes_before, "second upload must be no-op");
    }

    /// `get_bind_group` returns None for unknown id.
    #[tokio::test]
    async fn test_get_unknown_returns_none() {
        let (device, _queue) = headless_device().await;
        let cache = ImageCache::new(&device);
        assert!(cache.get_bind_group(ImageId(99)).is_none());
    }

    /// After evict, the entry is gone.
    #[tokio::test]
    async fn test_evict_removes_entry() {
        let (device, queue) = headless_device().await;
        let mut cache = ImageCache::new(&device);
        let png = make_test_png(0, 0, 255);
        cache.upload(ImageId(3), &png, &device, &queue).unwrap();
        assert!(cache.get_bind_group(ImageId(3)).is_some());
        cache.evict(ImageId(3));
        assert!(cache.get_bind_group(ImageId(3)).is_none());
        assert_eq!(cache.total_bytes(), 0);
    }
}
