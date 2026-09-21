use std::{
    fs::File,
    io::BufReader,
    path::Path,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use bytemuck::{Pod, Zeroable};
use image::{AnimationDecoder, ImageFormat};
use wgpu::util::DeviceExt;
use wgpu::{BindGroup, BindGroupLayout, Buffer, Device, Queue, RenderPass, Sampler, Texture, TextureFormat};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct BackgroundUniform {
    data: [f32; 4],
}

struct BackgroundFrame {
    rgba: Vec<u8>,
    delay: Duration,
}

pub struct BackgroundRenderer {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: BindGroupLayout,
    sampler: Sampler,
    uniform: Buffer,
    texture: Option<Texture>,
    bind_group: Option<BindGroup>,
    frames: Vec<BackgroundFrame>,
    frame_index: usize,
    last_frame_at: Instant,
    image_width: u32,
    image_height: u32,
    loaded_path: String,
}

impl BackgroundRenderer {
    pub fn new(device: &Device, format: TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Hafþi background shader"),
            source: wgpu::ShaderSource::Wgsl(r#"
struct Uniforms { data: vec4<f32>, };
@group(0) @binding(0) var bg_texture: texture_2d<f32>;
@group(0) @binding(1) var bg_sampler: sampler;
@group(0) @binding(2) var<uniform> uniforms: Uniforms;

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> VsOut {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 3.0, -1.0),
        vec2<f32>(-1.0,  3.0)
    );
    var uvs = array<vec2<f32>, 3>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(2.0, 1.0),
        vec2<f32>(0.0, -1.0)
    );
    var out: VsOut;
    out.position = vec4<f32>(positions[i], 0.0, 1.0);
    out.uv = uvs[i];
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let window_aspect = uniforms.data.x;
    let image_aspect = uniforms.data.y;
    let opacity = uniforms.data.z;
    var uv = in.uv;

    if (window_aspect > image_aspect) {
        let visible = image_aspect / window_aspect;
        uv.y = 0.5 + (uv.y - 0.5) * visible;
    } else {
        let visible = window_aspect / image_aspect;
        uv.x = 0.5 + (uv.x - 0.5) * visible;
    }

    let c = textureSample(bg_texture, bg_sampler, uv);
    return vec4<f32>(c.rgb, c.a * opacity);
}
"#.into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Hafþi background bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Hafþi background pipeline layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Hafþi background pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState { module: &shader, entry_point: "vs_main", buffers: &[] },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Hafþi background sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Hafþi background uniform"),
            contents: bytemuck::bytes_of(&BackgroundUniform { data: [1.0, 1.0, 0.35, 0.0] }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        Self {
            pipeline,
            bind_group_layout,
            sampler,
            uniform,
            texture: None,
            bind_group: None,
            frames: Vec::new(),
            frame_index: 0,
            last_frame_at: Instant::now(),
            image_width: 1,
            image_height: 1,
            loaded_path: String::new(),
        }
    }

    pub fn load(&mut self, device: &Device, queue: &Queue, path: &str) -> Result<()> {
        if path.is_empty() || path == "default" {
            self.clear();
            return Ok(());
        }
        if self.loaded_path == path && self.bind_group.is_some() {
            return Ok(());
        }

        let path_ref = Path::new(path);
        let format = image::ImageReader::open(path_ref)
            .with_context(|| format!("failed to open background image {}", path_ref.display()))?
            .with_guessed_format()
            .context("failed to detect background image format")?
            .format();

        let (frames, width, height) = if format == Some(ImageFormat::Gif) {
            let file = File::open(path_ref)?;
            let decoder = image::codecs::gif::GifDecoder::new(BufReader::new(file))?;
            let decoded = decoder.into_frames().collect_frames()?;
            let first = decoded.first().context("GIF contains no frame")?;
            let width = first.buffer().width();
            let height = first.buffer().height();
            let frames = decoded
                .into_iter()
                .map(|frame| {
                    let (numer, denom) = frame.delay().numer_denom_ms();
                    let delay = Duration::from_millis(
                        (numer as u64 / denom.max(1) as u64).max(10),
                    );
                    BackgroundFrame {
                        rgba: frame.into_buffer().into_raw(),
                        delay,
                    }
                })
                .collect();
            (frames, width, height)
        } else {
            let rgba = image::ImageReader::open(path_ref)?.decode()?.to_rgba8();
            let width = rgba.width();
            let height = rgba.height();
            (
                vec![BackgroundFrame {
                    rgba: rgba.into_raw(),
                    delay: Duration::from_secs(3600),
                }],
                width,
                height,
            )
        };

        if frames.is_empty() {
            anyhow::bail!("background image contains no frames");
        }
        self.image_width = width;
        self.image_height = height;

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Hafþi background texture"),
            size: wgpu::Extent3d {
                width: self.image_width,
                height: self.image_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        Self::upload(queue, &texture, self.image_width, self.image_height, &frames[0].rgba);

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Hafþi background bind group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.uniform.as_entire_binding(),
                },
            ],
        });

        self.texture = Some(texture);
        self.bind_group = Some(bind_group);
        self.frames = frames;
        self.frame_index = 0;
        self.last_frame_at = Instant::now();
        self.loaded_path = path.to_string();
        Ok(())
    }

    pub fn clear(&mut self) {
        self.texture = None;
        self.bind_group = None;
        self.frames.clear();
        self.loaded_path.clear();
    }

    fn upload(queue: &Queue, texture: &Texture, width: u32, height: u32, rgba: &[u8]) {
        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(4 * width),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
    }

    pub fn advance(&mut self, queue: &Queue, max_fps: u32) -> bool {
        if self.frames.len() < 2 {
            return false;
        }
        let min_delay = Duration::from_secs_f64(1.0 / max_fps.max(1) as f64);
        let delay = self.frames[self.frame_index].delay.max(min_delay);
        if self.last_frame_at.elapsed() < delay {
            return false;
        }

        self.frame_index = (self.frame_index + 1) % self.frames.len();
        self.last_frame_at = Instant::now();
        if let Some(texture) = self.texture.as_ref() {
            Self::upload(
                queue,
                texture,
                self.image_width,
                self.image_height,
                &self.frames[self.frame_index].rgba,
            );
        }
        true
    }

    pub fn next_deadline(&self, max_fps: u32) -> Option<Instant> {
        if self.frames.len() < 2 {
            return None;
        }
        let min_delay = Duration::from_secs_f64(1.0 / max_fps.max(1) as f64);
        Some(self.last_frame_at + self.frames[self.frame_index].delay.max(min_delay))
    }

    pub fn draw<'a>(
        &'a self,
        pass: &mut RenderPass<'a>,
        queue: &Queue,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        opacity: f32,
    ) {
        let Some(bind_group) = self.bind_group.as_ref() else {
            return;
        };
        let width = width.max(1.0);
        let height = height.max(1.0);
        let uniform = BackgroundUniform {
            data: [
                width / height,
                self.image_width.max(1) as f32 / self.image_height.max(1) as f32,
                opacity.clamp(0.0, 1.0),
                0.0,
            ],
        };
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&uniform));
        pass.set_viewport(x, y, width, height, 0.0, 1.0);
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        pass.draw(0..3, 0..1);
    }

}
