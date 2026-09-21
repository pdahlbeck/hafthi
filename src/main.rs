#[cfg(not(target_os = "linux"))]
compile_error!("Hafþi currently supports Linux/Wayland only.");

mod background;
mod menu;
mod preferences;
mod pty;
mod settings;
mod terminal;
mod ui_theme;
mod wayland_effect;

use std::{sync::Arc, time::Instant};

use anyhow::{Context, Result};
use arboard::Clipboard;
use bytemuck::{Pod, Zeroable};
use glyphon::{
    Attrs, Buffer, Color, Family, FontSystem, Metrics, Resolution, Shaping, SwashCache, TextArea,
    TextAtlas, TextBounds, TextRenderer, Weight,
};
use background::BackgroundRenderer;
use menu::{ContextMenu, MenuAction};
use preferences::{PrefAction, PreferencesPanel};
use pty::{AppEvent, PtySession};
use settings::{config_path, Settings};
use terminal::TerminalGrid;
use ui_theme as ui;
use wgpu::{
    Backends, CommandEncoderDescriptor, CompositeAlphaMode, Device, DeviceDescriptor, Features,
    Instance, InstanceDescriptor, Limits, LoadOp, MultisampleState, Operations, PowerPreference,
    PresentMode, Queue, RenderPassColorAttachment, RenderPassDescriptor, RequestAdapterOptions,
    Surface, SurfaceConfiguration, TextureUsages, TextureViewDescriptor,
    VertexAttribute, VertexBufferLayout, VertexFormat, VertexStepMode,
};
use wgpu::util::DeviceExt;
use winit::platform::wayland::WindowBuilderExtWayland;

use winit::{
    dpi::{LogicalSize, PhysicalSize},
    event::{ElementState, Event, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder},
    keyboard::{Key, ModifiersState, NamedKey},
    window::{Window, WindowBuilder},
};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct RectVertex {
    position: [f32; 2],
    color: [f32; 4],
}

struct RectRenderer {
    pipeline: wgpu::RenderPipeline,
}

impl RectRenderer {
    fn new(device: &Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Hafþi rectangle shader"),
            source: wgpu::ShaderSource::Wgsl(
                r#"
                struct VsOut {
                    @builtin(position) position: vec4<f32>,
                    @location(0) color: vec4<f32>,
                };

                @vertex
                fn vs_main(
                    @location(0) position: vec2<f32>,
                    @location(1) color: vec4<f32>
                ) -> VsOut {
                    var out: VsOut;
                    out.position = vec4<f32>(position, 0.0, 1.0);
                    out.color = color;
                    return out;
                }

                @fragment
                fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
                    return in.color;
                }
                "#.into(),
            ),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Hafþi rectangle pipeline layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });

        let attrs = [
            VertexAttribute {
                offset: 0,
                shader_location: 0,
                format: VertexFormat::Float32x2,
            },
            VertexAttribute {
                offset: std::mem::size_of::<[f32; 2]>() as u64,
                shader_location: 1,
                format: VertexFormat::Float32x4,
            },
        ];

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Hafþi rectangle pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[VertexBufferLayout {
                    array_stride: std::mem::size_of::<RectVertex>() as u64,
                    step_mode: VertexStepMode::Vertex,
                    attributes: &attrs,
                }],
            },
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
            multisample: MultisampleState::default(),
            multiview: None,
        });

        Self { pipeline }
    }

    fn push_rect(
        vertices: &mut Vec<RectVertex>,
        width: u32,
        height: u32,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: [f32; 4],
    ) {
        let sx = |px: f32| (px / width.max(1) as f32) * 2.0 - 1.0;
        let sy = |py: f32| 1.0 - (py / height.max(1) as f32) * 2.0;

        let x1 = sx(x);
        let x2 = sx(x + w);
        let y1 = sy(y);
        let y2 = sy(y + h);

        let v = |position| RectVertex { position, color };

        vertices.extend_from_slice(&[
            v([x1, y1]), v([x2, y1]), v([x2, y2]),
            v([x1, y1]), v([x2, y2]), v([x1, y2]),
        ]);
    }

    fn push_rounded_rect(
        vertices: &mut Vec<RectVertex>,
        width: u32,
        height: u32,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
        color: [f32; 4],
    ) {
        let radius = radius.min(w * 0.5).min(h * 0.5).max(0.0);
        if radius <= 0.5 {
            Self::push_rect(vertices, width, height, x, y, w, h, color);
            return;
        }

        let sx = |px: f32| (px / width.max(1) as f32) * 2.0 - 1.0;
        let sy = |py: f32| 1.0 - (py / height.max(1) as f32) * 2.0;
        let center = [sx(x + w * 0.5), sy(y + h * 0.5)];
        let v = |px: f32, py: f32| RectVertex {
            position: [sx(px), sy(py)],
            color,
        };
        let vc = RectVertex { position: center, color };

        let corners = [
            (x + radius, y + radius, std::f32::consts::PI, std::f32::consts::PI * 1.5),
            (x + w - radius, y + radius, std::f32::consts::PI * 1.5, std::f32::consts::PI * 2.0),
            (x + w - radius, y + h - radius, 0.0, std::f32::consts::PI * 0.5),
            (x + radius, y + h - radius, std::f32::consts::PI * 0.5, std::f32::consts::PI),
        ];

        let mut outline = Vec::new();
        const STEPS: usize = 6;
        for (cx, cy, start, end) in corners {
            for step in 0..=STEPS {
                let t = step as f32 / STEPS as f32;
                let angle = start + (end - start) * t;
                outline.push((cx + radius * angle.cos(), cy + radius * angle.sin()));
            }
        }

        for i in 0..outline.len() {
            let (x1, y1) = outline[i];
            let (x2, y2) = outline[(i + 1) % outline.len()];
            vertices.extend_from_slice(&[vc, v(x1, y1), v(x2, y2)]);
        }
    }

    fn create_buffer(&self, device: &Device, vertices: &[RectVertex]) -> Option<wgpu::Buffer> {
        if vertices.is_empty() {
            return None;
        }

        Some(device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Hafþi rectangle vertices"),
            contents: bytemuck::cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX,
        }))
    }

    fn draw<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        buffer: &'a wgpu::Buffer,
        vertex_count: u32,
    ) {
        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, buffer.slice(..));
        pass.draw(0..vertex_count, 0..1);
    }
}

struct GpuState {
    surface: Surface<'static>,
    device: Device,
    queue: Queue,
    config: SurfaceConfiguration,
    size: PhysicalSize<u32>,
    alpha_mode: CompositeAlphaMode,
    settings: Settings,

    font_system: FontSystem,
    swash_cache: SwashCache,
    atlas: TextAtlas,
    text_renderer: TextRenderer,
    text_buffer: Buffer,
    menu_buffer: Buffer,
    menu_icon_buffer: Buffer,
    menu_shortcut_buffer: Buffer,
    rect_renderer: RectRenderer,
    background_renderer: BackgroundRenderer,
}

impl GpuState {
    async fn new(window: Arc<Window>, settings: Settings) -> Result<Self> {
        let size = window.inner_size();

        let instance = Instance::new(InstanceDescriptor {
            backends: Backends::PRIMARY,
            ..Default::default()
        });

        let surface = instance
            .create_surface(window.clone())
            .context("failed to create wgpu surface")?;

        let adapter = instance
            .request_adapter(&RequestAdapterOptions {
                power_preference: PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .context("no suitable GPU adapter found")?;

        let info = adapter.get_info();
        eprintln!(
            "Hafþi: {} ({:?}, {:?})",
            info.name, info.backend, info.device_type
        );

        let (device, queue) = adapter
            .request_device(
                &DeviceDescriptor {
                    label: Some("Hafþi device"),
                    required_features: Features::empty(),
                    required_limits: Limits::downlevel_defaults(),
                },
                None,
            )
            .await
            .context("failed to create GPU device")?;

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|format| format.is_srgb())
            .unwrap_or(caps.formats[0]);

        let present_mode = if caps.present_modes.contains(&PresentMode::Mailbox) {
            PresentMode::Mailbox
        } else {
            PresentMode::Fifo
        };

        let alpha_mode = if caps.alpha_modes.contains(&CompositeAlphaMode::PreMultiplied) {
            CompositeAlphaMode::PreMultiplied
        } else if caps.alpha_modes.contains(&CompositeAlphaMode::PostMultiplied) {
            CompositeAlphaMode::PostMultiplied
        } else {
            caps.alpha_modes[0]
        };

        eprintln!("Hafþi alpha mode: {:?}", alpha_mode);

        let config = SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let mut font_system = FontSystem::new();
        let swash_cache = SwashCache::new();
        let mut atlas = TextAtlas::new(&device, &queue, format);
        let text_renderer =
            TextRenderer::new(&mut atlas, &device, MultisampleState::default(), None);
        let rect_renderer = RectRenderer::new(&device, format);
        let mut background_renderer = BackgroundRenderer::new(&device, format);
        if settings.branding_enabled {
            if let Err(err) =
                background_renderer.load(&device, &queue, &settings.branding_image)
            {
                eprintln!("Hafþi background: {err:#}");
            }
        }

        let mut text_buffer = Buffer::new(
            &mut font_system,
            Metrics::new(settings.font_size, settings.line_height),
        );
        text_buffer.set_size(&mut font_system, config.width as f32, config.height as f32);

        let menu_font_size = settings.font_size * 0.88;
        let menu_line_height = settings.line_height * 1.18;
        let mut menu_buffer = Buffer::new(
            &mut font_system,
            Metrics::new(menu_font_size, menu_line_height),
        );
        menu_buffer.set_size(&mut font_system, config.width as f32, config.height as f32);
        let mut menu_icon_buffer = Buffer::new(
            &mut font_system,
            Metrics::new(menu_font_size * 0.92, menu_line_height),
        );
        menu_icon_buffer.set_size(&mut font_system, config.width as f32, config.height as f32);
        let mut menu_shortcut_buffer = Buffer::new(
            &mut font_system,
            Metrics::new(menu_font_size * 0.82, menu_line_height),
        );
        menu_shortcut_buffer.set_size(&mut font_system, config.width as f32, config.height as f32);

        Ok(Self {
            surface,
            device,
            queue,
            config,
            size,
            alpha_mode,
            settings,
            font_system,
            swash_cache,
            atlas,
            text_renderer,
            text_buffer,
            menu_buffer,
            menu_icon_buffer,
            menu_shortcut_buffer,
            rect_renderer,
            background_renderer,
        })
    }

    fn resize(&mut self, size: PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            return;
        }

        self.size = size;
        self.config.width = size.width;
        self.config.height = size.height;
        self.surface.configure(&self.device, &self.config);
        self.text_buffer
            .set_size(&mut self.font_system, size.width as f32, size.height as f32);
        self.menu_buffer
            .set_size(&mut self.font_system, size.width as f32, size.height as f32);
        self.menu_icon_buffer
            .set_size(&mut self.font_system, size.width as f32, size.height as f32);
        self.menu_shortcut_buffer
            .set_size(&mut self.font_system, size.width as f32, size.height as f32);
    }

    fn menu_line_height(&self) -> f32 {
        self.settings.line_height * 1.18
    }

    fn apply_settings(&mut self, settings: Settings) {
        self.settings = settings;

        if self.settings.branding_enabled {
            if let Err(err) = self.background_renderer.load(
                &self.device,
                &self.queue,
                &self.settings.branding_image,
            ) {
                eprintln!("Hafþi background: {err:#}");
            }
        }

        let font_size = self.settings.font_size;
        let line_height = self.settings.line_height;
        let menu_font_size = font_size * 0.88;
        let menu_line_height = line_height * 1.18;

        self.text_buffer.set_metrics(
            &mut self.font_system,
            Metrics::new(font_size, line_height),
        );
        self.menu_buffer.set_metrics(
            &mut self.font_system,
            Metrics::new(menu_font_size, menu_line_height),
        );
        self.menu_icon_buffer.set_metrics(
            &mut self.font_system,
            Metrics::new(menu_font_size * 0.92, menu_line_height),
        );
        self.menu_shortcut_buffer.set_metrics(
            &mut self.font_system,
            Metrics::new(menu_font_size * 0.82, menu_line_height),
        );
    }

    fn background_deadline(&self) -> Option<Instant> {
        if self.settings.branding_enabled {
            self.background_renderer
                .next_deadline(self.settings.branding_max_fps)
        } else {
            None
        }
    }

    fn advance_background(&mut self) {
        if self.settings.branding_enabled {
            self.background_renderer
                .advance(&self.queue, self.settings.branding_max_fps);
        }
    }

    fn update_terminal_text(&mut self, terminal: &TerminalGrid) {
        let runs = terminal.styled_runs();

        self.text_buffer.set_rich_text(
            &mut self.font_system,
            runs.iter().map(|(text, style)| {
                let attrs = Attrs::new()
                    .family(Family::Name(&self.settings.font_family))
                    .color(Color::rgb(style.fg.r, style.fg.g, style.fg.b))
                    .weight(if style.bold {
                        Weight::BOLD
                    } else {
                        Weight::NORMAL
                    });
                (text.as_str(), attrs)
            }),
            Shaping::Advanced,
        );
        self.text_buffer.shape_until_scroll(&mut self.font_system);
    }

    fn render(
        &mut self,
        terminal: &TerminalGrid,
        selection: Option<((usize, usize), (usize, usize))>,
        menu: &ContextMenu,
        prefs: &PreferencesPanel,
    ) -> Result<()> {
        self.advance_background();

        if menu.visible {
            let ui_attrs = Attrs::new()
                .family(Family::SansSerif)
                .color(Color::rgb(240, 241, 243));

            self.menu_buffer.set_text(
                &mut self.font_system,
                &menu.text(),
                ui_attrs,
                Shaping::Advanced,
            );
            self.menu_buffer.shape_until_scroll(&mut self.font_system);

            self.menu_icon_buffer.set_text(
                &mut self.font_system,
                &menu.icons(),
                Attrs::new()
                    .family(Family::SansSerif)
                    .color(Color::rgb(213, 216, 221)),
                Shaping::Advanced,
            );
            self.menu_icon_buffer.shape_until_scroll(&mut self.font_system);

            self.menu_shortcut_buffer.set_text(
                &mut self.font_system,
                &menu.shortcuts(),
                Attrs::new()
                    .family(Family::SansSerif)
                    .color(Color::rgb(157, 161, 169)),
                Shaping::Advanced,
            );
            self.menu_shortcut_buffer.shape_until_scroll(&mut self.font_system);
        }

        let terminal_area = TextArea {
            buffer: &self.text_buffer,
            left: self.settings.padding,
            top: terminal_top(&self.settings),
            scale: 1.0,
            bounds: TextBounds {
                left: 0,
                top: 0,
                right: self.config.width as i32,
                bottom: self.config.height as i32,
            },
            default_color: Color::rgb(
                self.settings.foreground.r,
                self.settings.foreground.g,
                self.settings.foreground.b,
            ),
        };

        if prefs.visible {
            let primary = Color::rgb(240, 241, 243);
            let muted = Color::rgb(157, 161, 169);
            let mut labels: Vec<(Buffer, f32, f32, TextBounds)> = Vec::new();
            let mut add = |text: &str, x: f32, y: f32, width: f32, size: f32, color: Color| {
                let (px, py) = prefs.pos(x, y);
                let mut buffer = Buffer::new(
                    &mut self.font_system,
                    Metrics::new(size * prefs.scale, size * 1.35 * prefs.scale),
                );
                buffer.set_size(&mut self.font_system, width * prefs.scale, 38.0 * prefs.scale);
                buffer.set_text(
                    &mut self.font_system, text,
                    Attrs::new().family(Family::SansSerif).color(color),
                    Shaping::Advanced,
                );
                buffer.shape_until_scroll(&mut self.font_system);
                labels.push((buffer, px, py, TextBounds {
                    left: px as i32, top: py as i32,
                    right: (px + width * prefs.scale) as i32,
                    bottom: (py + 38.0 * prefs.scale) as i32,
                }));
            };

            add("Settings", 24.0, 19.0, 400.0, 21.0, primary);
            add("Appearance and background", 24.0, 52.0, 400.0, 12.0, muted);
            for (title, y) in [("APPEARANCE", 86.0), ("TERMINAL", 253.0), ("BACKGROUND", 332.0)] {
                add(title, 24.0, y, 250.0, 11.0, muted);
            }
            for (name, y) in [
                ("Font size", 119.0), ("Transparency", 163.0),
                ("Padding", 207.0), ("Scrollback", 286.0),
                ("Image display", 365.0), ("Image / GIF", 422.0),
                ("GIF max FPS", 493.0),
            ] {
                add(name, 24.0, y, 180.0, 15.0, primary);
            }
            for (value, y) in [
                (format!("{:.1} px", self.settings.font_size), 119.0),
                (format!("{}%", (self.settings.opacity * 100.0).round() as u32), 163.0),
                (format!("{:.0} px", self.settings.logical_padding()), 207.0),
                (self.settings.scrollback.to_string(), 286.0),
                (self.settings.branding_max_fps.to_string(), 493.0),
            ] {
                add(&value, 420.0, y, 125.0, 14.0, muted);
            }
            let image = &self.settings.branding_image;
            let filename = std::path::Path::new(image).file_name()
                .and_then(|name| name.to_str()).unwrap_or(image);
            let filename = truncate_label(filename, 30);
            let path = if image == "default" || image.is_empty() {
                "No image selected".to_string()
            } else {
                truncate_label(image, 44)
            };
            add(&filename, 205.0, 410.0, 275.0, 14.0, primary);
            add(&path, 205.0, 442.0, 275.0, 11.0, muted);
            for button in prefs.button_rects() {
                let caption = match button.action {
                    PrefAction::FontDown
                    | PrefAction::OpacityDown
                    | PrefAction::PaddingDown
                    | PrefAction::ScrollbackDown
                    | PrefAction::GifFpsDown => "−",
                    PrefAction::FontUp
                    | PrefAction::OpacityUp
                    | PrefAction::PaddingUp
                    | PrefAction::ScrollbackUp
                    | PrefAction::GifFpsUp => "+",
                    PrefAction::ImageOff => "Off",
                    PrefAction::ImageBanner => "Banner",
                    PrefAction::ImageFull => "Full",
                    PrefAction::ChooseImage => "Choose…",
                    PrefAction::ClearImage => "Clear",
                    PrefAction::Cancel => "Cancel",
                    PrefAction::Save => "Save",
                };
                let font_size = if caption == "+" || caption == "−" { 18.0 } else { 13.0 };
                let text_width = caption.chars().count() as f32 * font_size * 0.54 * prefs.scale;
                let x = (button.x + (button.w - text_width) / 2.0 - prefs.x) / prefs.scale;
                let y = (button.y - prefs.y) / prefs.scale + if font_size > 13.0 { 3.0 } else { 8.0 };
                let color = if button.action == PrefAction::Save {
                    Color::rgb(24, 26, 29)
                } else { primary };
                add(caption, x, y, button.w / prefs.scale, font_size, color);
            }
            drop(add);
            let mut areas = vec![terminal_area];
            areas.extend(labels.iter().map(|(buffer, x, y, bounds)| TextArea {
                buffer, left: *x, top: *y, scale: 1.0,
                bounds: *bounds, default_color: primary,
            }));
            self.text_renderer
                .prepare(
                    &self.device,
                    &self.queue,
                    &mut self.font_system,
                    &mut self.atlas,
                    Resolution {
                        width: self.config.width,
                        height: self.config.height,
                    },
                    areas,
                    &mut self.swash_cache,
                )
                .context("failed to prepare GPU text")?;
        } else if menu.visible {
            let scale = self.settings.scale_factor.max(1.0);
            let icon_area = TextArea {
                buffer: &self.menu_icon_buffer,
                left: menu.x + 20.0 * scale,
                top: menu.y,
                scale: 1.0,
                bounds: TextBounds {
                    left: menu.x as i32,
                    top: menu.y as i32,
                    right: (menu.x + 54.0 * scale) as i32,
                    bottom: (menu.y + menu.height()) as i32,
                },
                default_color: Color::rgb(213, 216, 221),
            };
            let menu_area = TextArea {
                buffer: &self.menu_buffer,
                left: menu.x + 58.0 * scale,
                top: menu.y,
                scale: 1.0,
                bounds: TextBounds {
                    left: (menu.x + 54.0 * scale) as i32,
                    top: menu.y as i32,
                    right: (menu.x + menu.width - 175.0 * scale) as i32,
                    bottom: (menu.y + menu.height()) as i32,
                },
                default_color: Color::rgb(240, 241, 243),
            };
            let shortcut_area = TextArea {
                buffer: &self.menu_shortcut_buffer,
                left: menu.x + menu.width - 168.0 * scale,
                top: menu.y,
                scale: 1.0,
                bounds: TextBounds {
                    left: (menu.x + menu.width - 174.0 * scale) as i32,
                    top: menu.y as i32,
                    right: (menu.x + menu.width - 20.0 * scale) as i32,
                    bottom: (menu.y + menu.height()) as i32,
                },
                default_color: Color::rgb(157, 161, 169),
            };

            self.text_renderer
                .prepare(
                    &self.device,
                    &self.queue,
                    &mut self.font_system,
                    &mut self.atlas,
                    Resolution {
                        width: self.config.width,
                        height: self.config.height,
                    },
                    [terminal_area, icon_area, menu_area, shortcut_area],
                    &mut self.swash_cache,
                )
                .context("failed to prepare GPU text")?;
        } else {
            self.text_renderer
                .prepare(
                    &self.device,
                    &self.queue,
                    &mut self.font_system,
                    &mut self.atlas,
                    Resolution {
                        width: self.config.width,
                        height: self.config.height,
                    },
                    [terminal_area],
                    &mut self.swash_cache,
                )
                .context("failed to prepare GPU text")?;
        }

        let mut rect_vertices = Vec::new();

        if let Some((start, end)) = selection {
            let (a, b) = if (start.1, start.0) <= (end.1, end.0) {
                (start, end)
            } else {
                (end, start)
            };

            for row in a.1..=b.1 {
                let start_col = if row == a.1 { a.0 } else { 0 };
                let end_col = if row == b.1 { b.0 } else { terminal.dimensions().0.saturating_sub(1) };

                let x = self.settings.padding + start_col as f32 * self.settings.cell_width;
                let y = terminal_top(&self.settings) + row as f32 * self.settings.line_height;
                let width =
                    (end_col.saturating_sub(start_col) + 1) as f32 * self.settings.cell_width;

                RectRenderer::push_rect(
                    &mut rect_vertices,
                    self.config.width,
                    self.config.height,
                    x,
                    y,
                    width,
                    self.settings.line_height,
                    Settings::rgba_f32(self.settings.selection_background, 1.0),
                );
            }
        }

        if terminal.cursor_visible() {
            let (cx, cy) = terminal.cursor();

            // Use glyphon's actual shaped glyph geometry for the cursor instead
            // of estimating X from font_size * a constant. This keeps the block
            // exactly on the terminal cell after the final rendered character,
            // regardless of DPI, font metrics or glyphon shaping.
            let shaped_cursor = self
                .text_buffer
                .layout_runs()
                .find(|run| run.line_i == cy)
                .and_then(|run| {
                    if let Some(glyph) = run.glyphs.get(cx) {
                        Some((glyph.x, glyph.w))
                    } else {
                        run.glyphs
                            .last()
                            .map(|glyph| (glyph.x + glyph.w, glyph.w))
                    }
                });

            let (cursor_x, cursor_w) = shaped_cursor.unwrap_or((
                cx as f32 * self.settings.cell_width,
                self.settings.cell_width,
            ));

            RectRenderer::push_rect(
                &mut rect_vertices,
                self.config.width,
                self.config.height,
                self.settings.padding + cursor_x,
                terminal_top(&self.settings) + cy as f32 * self.settings.line_height,
                cursor_w.max(1.0),
                self.settings.line_height,
                Settings::rgba_f32(self.settings.cursor, 0.55),
            );
        }

        if prefs.visible {
            let scale = prefs.scale;
            RectRenderer::push_rounded_rect(
                &mut rect_vertices,
                self.config.width,
                self.config.height,
                prefs.x - scale, prefs.y - scale,
                prefs.width + 2.0 * scale, prefs.height() + 2.0 * scale,
                (ui::PANEL_RADIUS + 1.0) * scale, ui::PANEL_BORDER,
            );
            RectRenderer::push_rounded_rect(
                &mut rect_vertices,
                self.config.width,
                self.config.height,
                prefs.x, prefs.y, prefs.width, prefs.height(),
                ui::PANEL_RADIUS * scale, ui::PANEL_SURFACE,
            );
            if let Some(row) = prefs.hovered_row {
                let (x, y, w, h) = prefs.row_rect(row);
                RectRenderer::push_rounded_rect(
                    &mut rect_vertices, self.config.width, self.config.height,
                    x, y, w, h, ui::CONTROL_RADIUS * scale, ui::HOVER,
                );
            }
            for y in [77.0, 244.0, 323.0, 534.0] {
                let (x, y) = prefs.pos(24.0, y);
                RectRenderer::push_rect(
                    &mut rect_vertices, self.config.width, self.config.height,
                    x, y, prefs.width - 48.0 * scale, scale.max(1.0), ui::DIVIDER,
                );
            }
            let selected = if !self.settings.branding_enabled {
                PrefAction::ImageOff
            } else if self.settings.branding_mode == "banner" {
                PrefAction::ImageBanner
            } else {
                PrefAction::ImageFull
            };
            for button in prefs.button_rects() {
                let color = if button.action == PrefAction::Save {
                    if prefs.hovered == Some(button.action) {
                        [1.0, 1.0, 1.0, 1.0]
                    } else {
                        [0.91, 0.92, 0.93, 1.0]
                    }
                } else if prefs.hovered == Some(button.action) {
                    ui::HOVER
                } else if button.action == selected {
                    ui::ACTIVE
                } else {
                    ui::CONTROL
                };
                RectRenderer::push_rounded_rect(
                    &mut rect_vertices, self.config.width, self.config.height,
                    button.x, button.y, button.w, button.h,
                    ui::CONTROL_RADIUS * scale, color,
                );
            }
        }

        if menu.visible {
            let scale = self.settings.scale_factor.max(1.0);
            let radius = ui::PANEL_RADIUS * scale;

            // A subtle edge plus a charcoal surface gives the menu the same
            // modern visual weight as contemporary KDE/Wayland menus.
            RectRenderer::push_rounded_rect(
                &mut rect_vertices,
                self.config.width,
                self.config.height,
                menu.x - 1.0 * scale,
                menu.y - 1.0 * scale,
                menu.width + 2.0 * scale,
                menu.height() + 2.0 * scale,
                radius + 1.0 * scale,
                ui::PANEL_BORDER,
            );
            RectRenderer::push_rounded_rect(
                &mut rect_vertices,
                self.config.width,
                self.config.height,
                menu.x,
                menu.y,
                menu.width,
                menu.height(),
                radius,
                ui::PANEL_SURFACE,
            );

            if let Some(index) = menu.hovered {
                RectRenderer::push_rounded_rect(
                    &mut rect_vertices,
                    self.config.width,
                    self.config.height,
                    menu.x + 7.0 * scale,
                    menu.y + index as f32 * menu.row_height + 4.0 * scale,
                    menu.width - 14.0 * scale,
                    menu.row_height - 8.0 * scale,
                    ui::CONTROL_RADIUS * scale,
                    ui::HOVER,
                );
            }

            for (index, entry) in menu.entries.iter().enumerate() {
                if entry.separator_after && index + 1 < menu.entries.len() {
                    let y = menu.y + (index + 1) as f32 * menu.row_height;
                    RectRenderer::push_rect(
                        &mut rect_vertices,
                        self.config.width,
                        self.config.height,
                        menu.x + 16.0 * scale,
                        y - 0.5 * scale,
                        menu.width - 32.0 * scale,
                        1.0 * scale,
                        ui::DIVIDER,
                    );
                }
            }
        }

        let frame = self
            .surface
            .get_current_texture()
            .context("failed to acquire surface texture")?;
        let view = frame
            .texture
            .create_view(&TextureViewDescriptor::default());

        let rect_buffer = self
            .rect_renderer
            .create_buffer(&self.device, &rect_vertices);

        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("Hafþi frame encoder"),
            });

        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("Hafþi terminal pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(match self.alpha_mode {
                            CompositeAlphaMode::PreMultiplied => {
                                Settings::rgba_premultiplied(
                                    self.settings.background,
                                    self.settings.opacity,
                                )
                            }
                            CompositeAlphaMode::PostMultiplied => {
                                Settings::rgba(
                                    self.settings.background,
                                    self.settings.opacity,
                                )
                            }
                            CompositeAlphaMode::Opaque => {
                                Settings::rgba(self.settings.background, 1.0)
                            }
                            _ => Settings::rgba(
                                self.settings.background,
                                self.settings.opacity,
                            ),
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            if self.settings.branding_enabled {
                if self.settings.branding_mode == "banner" {
                    let x = self.settings.padding;
                    let y = self.settings.padding;
                    let width = (520.0 * self.settings.scale_factor.max(1.0))
                        .min((self.config.width as f32 - self.settings.padding * 2.0).max(1.0));
                    let height = banner_height(&self.settings)
                        .min((self.config.height as f32 - self.settings.padding * 3.0).max(1.0));
                    self.background_renderer.draw(
                        &mut pass,
                        &self.queue,
                        x,
                        y,
                        width,
                        height,
                        1.0,
                    );
                } else {
                    self.background_renderer.draw(
                        &mut pass,
                        &self.queue,
                        0.0,
                        0.0,
                        self.config.width as f32,
                        self.config.height as f32,
                        self.settings.opacity as f32,
                    );
                }
                pass.set_viewport(
                    0.0,
                    0.0,
                    self.config.width as f32,
                    self.config.height as f32,
                    0.0,
                    1.0,
                );
            }

            if let Some(buffer) = rect_buffer.as_ref() {
                self.rect_renderer
                    .draw(&mut pass, buffer, rect_vertices.len() as u32);
            }

            self.text_renderer
                .render(&self.atlas, &mut pass)
                .context("failed to render GPU text")?;
        }

        self.queue.submit(Some(encoder.finish()));
        frame.present();
        self.atlas.trim();
        Ok(())
    }
}

fn banner_height(settings: &Settings) -> f32 {
    180.0 * settings.scale_factor.max(1.0)
}

fn truncate_label(value: &str, max_chars: usize) -> String {
    let chars: Vec<char> = value.chars().collect();
    if chars.len() <= max_chars {
        return value.to_string();
    }
    format!("…{}", chars[chars.len() - (max_chars - 1)..].iter().collect::<String>())
}

fn terminal_top(settings: &Settings) -> f32 {
    if settings.branding_enabled
        && settings.branding_mode == "banner"
        && !settings.branding_image.is_empty()
        && settings.branding_image != "default"
    {
        settings.padding * 2.0 + banner_height(settings)
    } else {
        settings.padding
    }
}

fn grid_size(size: PhysicalSize<u32>, settings: &Settings) -> (u16, u16) {
    let usable_w = (size.width as f32 - settings.padding * 2.0).max(settings.cell_width);
    let usable_h = (size.height as f32 - terminal_top(settings) - settings.padding).max(settings.line_height);
    let cols = (usable_w / settings.cell_width).floor().max(1.0) as u16;
    let rows = (usable_h / settings.line_height).floor().max(1.0) as u16;
    (cols, rows)
}

fn mouse_to_cell(
    position: winit::dpi::PhysicalPosition<f64>,
    terminal: &TerminalGrid,
    settings: &Settings,
) -> (usize, usize) {
    let (cols, rows) = terminal.dimensions();
    let x =
        (((position.x as f32) - settings.padding).max(0.0) / settings.cell_width).floor() as usize;
    let y =
        (((position.y as f32) - terminal_top(settings)).max(0.0) / settings.line_height).floor() as usize;
    (
        x.min(cols.saturating_sub(1)),
        y.min(rows.saturating_sub(1)),
    )
}

fn send_key(pty: &PtySession, key: &Key, text: Option<&str>, modifiers: ModifiersState) {
    if modifiers.control_key() {
        if let Key::Character(ch) = key {
            let lower = ch.to_lowercase();
            let mut chars = lower.chars();
            if let Some(c) = chars.next() {
                if c.is_ascii_lowercase() {
                    let code = (c as u8) - b'a' + 1;
                    pty.write(&[code]);
                    return;
                }
            }
        }
    }

    match key {
        Key::Named(NamedKey::Enter) => pty.write(b"\r"),
        Key::Named(NamedKey::Backspace) => pty.write(&[0x7f]),
        Key::Named(NamedKey::Tab) => pty.write(b"\t"),
        Key::Named(NamedKey::Escape) => pty.write(&[0x1b]),
        Key::Named(NamedKey::ArrowUp) => pty.write(b"\x1b[A"),
        Key::Named(NamedKey::ArrowDown) => pty.write(b"\x1b[B"),
        Key::Named(NamedKey::ArrowRight) => pty.write(b"\x1b[C"),
        Key::Named(NamedKey::ArrowLeft) => pty.write(b"\x1b[D"),
        Key::Named(NamedKey::Home) => pty.write(b"\x1b[H"),
        Key::Named(NamedKey::End) => pty.write(b"\x1b[F"),
        Key::Named(NamedKey::Delete) => pty.write(b"\x1b[3~"),
        Key::Named(NamedKey::PageUp) => pty.write(b"\x1b[5~"),
        Key::Named(NamedKey::PageDown) => pty.write(b"\x1b[6~"),
        _ => {
            if let Some(text) = text {
                if !text.is_empty() && !modifiers.control_key() {
                    pty.write(text.as_bytes());
                }
            }
        }
    }
}

fn request_hyprland_no_blur() {
    if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_none() {
        return;
    }

    let pid_target = format!("pid:{}", std::process::id());

    std::thread::spawn(move || {
        // Wait until Hyprland has mapped the Wayland window, then target the
        // exact Hafþi process instead of relying on class/title regexes.
        for delay_ms in [100_u64, 250, 500, 900] {
            std::thread::sleep(std::time::Duration::from_millis(delay_ms));

            for prop in ["no_blur", "noblur"] {
                let status = std::process::Command::new("hyprctl")
                    .args(["setprop", &pid_target, prop, "1", "lock"])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();

                if status.is_ok_and(|status| status.success()) {
                    return;
                }
            }
        }
    });
}


fn main() -> Result<()> {
    let event_loop = EventLoopBuilder::<AppEvent>::with_user_event().build()?;
    let proxy = event_loop.create_proxy();

    let mut settings = Settings::load();

    let builder = WindowBuilder::new()
        .with_title("Hafþi")
        .with_transparent(true)
        .with_inner_size(LogicalSize::new(
            settings.window_width as f64,
            settings.window_height as f64,
        ));

    let builder = builder.with_name("se.dahlbeck.Hafthi", "se.dahlbeck.Hafthi");

    let window = Arc::new(builder.build(&event_loop)?);

    settings.apply_scale_factor(window.scale_factor());
    eprintln!(
        "Hafþi scale: {:.2}x, font: {:.1}px",
        window.scale_factor(),
        settings.font_size
    );

    // Attach ext-background-effect-v1 to winit's own wl_surface with an
    // explicitly empty blur region. Hyprland treats that as application-level
    // no-blur and gives it precedence over the compositor's normal blur policy.
    let mut wayland_no_blur = wayland_effect::NoBlur::attach(&window);

    let mut gpu = pollster::block_on(GpuState::new(window.clone(), settings.clone()))?;
    window.set_title("Hafþi");
    if wayland_no_blur.is_none() {
        eprintln!("Hafþi: ext-background-effect-v1 unavailable, using Hyprland fallback");
        request_hyprland_no_blur();
    }

    let (cols, rows) = grid_size(window.inner_size(), &settings);
    let mut terminal = TerminalGrid::new_with_theme(
        cols as usize,
        rows as usize,
        settings.foreground,
        settings.ansi,
        settings.scrollback,
    );
    let pty = PtySession::spawn(cols, rows, proxy.clone())?;

    let mut selection: Option<((usize, usize), (usize, usize))> = None;
    let mut selecting = false;
    let mut mouse_pos = winit::dpi::PhysicalPosition::new(0.0, 0.0);
    let mut clipboard = Clipboard::new().ok();
    let mut context_menu = ContextMenu::new();
    let mut preferences = PreferencesPanel::new();
    let mut preferences_backup: Option<Settings> = None;

    gpu.update_terminal_text(&terminal);

    let mut dirty = true;
    let mut modifiers = ModifiersState::empty();

    event_loop.run(move |event, elwt| {
        match event {
            Event::UserEvent(AppEvent::PtyOutput(bytes)) => {
                terminal.feed(&bytes);
                gpu.update_terminal_text(&terminal);
                dirty = true;
                window.request_redraw();
            }
            Event::UserEvent(AppEvent::PtyExited) => {
                elwt.exit();
            }
            Event::UserEvent(AppEvent::ImageChosen(path)) => {
                if let Some(path) = path {
                    settings.branding_image = path;
                    if !settings.branding_enabled {
                        settings.branding_enabled = true;
                        settings.branding_mode = "banner".into();
                    }
                    gpu.apply_settings(settings.clone());

                    let size = window.inner_size();
                    let (cols, rows) = grid_size(size, &settings);
                    terminal.resize(cols as usize, rows as usize);
                    terminal.set_scrollback_limit(settings.scrollback);
                    pty.resize(
                        cols,
                        rows,
                        size.width.min(u16::MAX as u32) as u16,
                        size.height.min(u16::MAX as u32) as u16,
                    );
                    gpu.update_terminal_text(&terminal);
                    dirty = true;
                    window.request_redraw();
                }
            }
            Event::AboutToWait => {
                if let Some(no_blur) = wayland_no_blur.as_mut() {
                    no_blur.dispatch_pending();
                }

                if let Some(deadline) = gpu.background_deadline() {
                    if deadline <= Instant::now() {
                        dirty = true;
                        window.request_redraw();
                    } else {
                        elwt.set_control_flow(ControlFlow::WaitUntil(deadline));
                    }
                } else {
                    elwt.set_control_flow(ControlFlow::Wait);
                }
            }
            Event::WindowEvent { window_id, event } if window_id == window.id() => match event {
                WindowEvent::CloseRequested => elwt.exit(),
                WindowEvent::ModifiersChanged(new_modifiers) => {
                    modifiers = new_modifiers.state();
                }
                WindowEvent::KeyboardInput { event, .. } => {
                    if event.state == ElementState::Pressed {
                        if preferences.visible {
                            if event.logical_key == Key::Named(NamedKey::Escape) {
                                if let Some(original) = preferences_backup.take() {
                                    settings = original;
                                    gpu.apply_settings(settings.clone());

                                    let size = window.inner_size();
                                    let (cols, rows) = grid_size(size, &settings);
                                    terminal.resize(cols as usize, rows as usize);
                                    terminal.set_scrollback_limit(settings.scrollback);
                                    pty.resize(
                                        cols,
                                        rows,
                                        size.width.min(u16::MAX as u32) as u16,
                                        size.height.min(u16::MAX as u32) as u16,
                                    );
                                    gpu.update_terminal_text(&terminal);
                                }
                                preferences.close();
                                dirty = true;
                                window.request_redraw();
                            }
                            return;
                        }

                        if context_menu.visible {
                            context_menu.close();
                            dirty = true;
                            window.request_redraw();

                            if event.logical_key == Key::Named(NamedKey::Escape) {
                                return;
                            }
                        }

                        let ctrl_shift = modifiers.control_key() && modifiers.shift_key();

                        if ctrl_shift {
                            match &event.logical_key {
                                Key::Character(ch) if ch.eq_ignore_ascii_case("c") => {
                                    if let (Some(selection), Some(clipboard)) =
                                        (selection, clipboard.as_mut())
                                    {
                                        let text =
                                            terminal.selected_text(selection.0, selection.1);
                                        if !text.is_empty() {
                                            let _ = clipboard.set_text(text);
                                        }
                                    }
                                    return;
                                }
                                Key::Character(ch) if ch.eq_ignore_ascii_case("v") => {
                                    if let Some(clipboard) = clipboard.as_mut() {
                                        if let Ok(text) = clipboard.get_text() {
                                            terminal.scroll_to_bottom();
                                            pty.write(text.as_bytes());
                                            selection = None;
                                            gpu.update_terminal_text(&terminal);
                                            dirty = true;
                                            window.request_redraw();
                                        }
                                    }
                                    return;
                                }
                                _ => {}
                            }
                        }

                        selection = None;
                        terminal.scroll_to_bottom();
                        send_key(
                            &pty,
                            &event.logical_key,
                            event.text.as_deref(),
                            modifiers,
                        );
                    }
                }
                WindowEvent::CursorMoved { position, .. } => {
                    mouse_pos = position;

                    if preferences.visible {
                        if preferences.update_hover(position.x as f32, position.y as f32) {
                            dirty = true;
                            window.request_redraw();
                        }
                    } else if context_menu.visible {
                        if context_menu.update_hover(position.x as f32, position.y as f32) {
                            dirty = true;
                            window.request_redraw();
                        }
                    } else if selecting {
                        if let Some((start, _)) = selection {
                            selection = Some((start, mouse_to_cell(position, &terminal, &settings)));
                            dirty = true;
                            window.request_redraw();
                        }
                    }
                }
                WindowEvent::MouseInput {
                    state: ElementState::Pressed,
                    button: MouseButton::Right,
                    ..
                } => {
                    if preferences.visible {
                        return;
                    }
                    selecting = false;
                    context_menu.open(
                        mouse_pos.x as f32,
                        mouse_pos.y as f32,
                        gpu.config.width,
                        gpu.config.height,
                        settings.scale_factor,
                        gpu.menu_line_height(),
                    );
                    dirty = true;
                    window.request_redraw();
                }
                WindowEvent::MouseInput {
                    state,
                    button: MouseButton::Left,
                    ..
                } => {
                    if preferences.visible {
                        if state == ElementState::Pressed {
                            let action = preferences
                                .action_at(mouse_pos.x as f32, mouse_pos.y as f32);

                            match action {
                                Some(PrefAction::FontDown) => {
                                    settings.zoom_by(1.0 / 1.05);
                                    gpu.apply_settings(settings.clone());
                                }
                                Some(PrefAction::FontUp) => {
                                    settings.zoom_by(1.05);
                                    gpu.apply_settings(settings.clone());
                                }
                                Some(PrefAction::OpacityDown) => {
                                    settings.opacity = (settings.opacity - 0.05).max(0.0);
                                    gpu.apply_settings(settings.clone());
                                }
                                Some(PrefAction::OpacityUp) => {
                                    settings.opacity = (settings.opacity + 0.05).min(1.0);
                                    gpu.apply_settings(settings.clone());
                                }
                                Some(PrefAction::PaddingDown) => {
                                    let step = 2.0 * settings.scale_factor.max(1.0);
                                    settings.padding = (settings.padding - step).max(0.0);
                                    gpu.apply_settings(settings.clone());
                                }
                                Some(PrefAction::PaddingUp) => {
                                    let step = 2.0 * settings.scale_factor.max(1.0);
                                    settings.padding += step;
                                    gpu.apply_settings(settings.clone());
                                }
                                Some(PrefAction::ScrollbackDown) => {
                                    settings.scrollback = settings.scrollback.saturating_sub(1000).max(100);
                                    terminal.set_scrollback_limit(settings.scrollback);
                                }
                                Some(PrefAction::ScrollbackUp) => {
                                    settings.scrollback = settings.scrollback.saturating_add(1000).min(100_000);
                                    terminal.set_scrollback_limit(settings.scrollback);
                                }
                                Some(PrefAction::ImageOff)
                                | Some(PrefAction::ImageBanner)
                                | Some(PrefAction::ImageFull) => {
                                    match action.unwrap() {
                                        PrefAction::ImageOff => settings.branding_enabled = false,
                                        PrefAction::ImageBanner => {
                                            settings.branding_enabled = true;
                                            settings.branding_mode = "banner".into();
                                        }
                                        PrefAction::ImageFull => {
                                            settings.branding_enabled = true;
                                            settings.branding_mode = "full".into();
                                        }
                                        _ => unreachable!(),
                                    }
                                    gpu.apply_settings(settings.clone());

                                    let size = window.inner_size();
                                    let (cols, rows) = grid_size(size, &settings);
                                    terminal.resize(cols as usize, rows as usize);
                                    terminal.set_scrollback_limit(settings.scrollback);
                                    pty.resize(
                                        cols,
                                        rows,
                                        size.width.min(u16::MAX as u32) as u16,
                                        size.height.min(u16::MAX as u32) as u16,
                                    );
                                    gpu.update_terminal_text(&terminal);
                                }
                                Some(PrefAction::ChooseImage) => {
                                    // Use rfd's asynchronous portal API off the winit event
                                    // loop. The synchronous dialog blocks Wayland event
                                    // dispatch and makes Hyprland report Hafþi as hung.
                                    let dialog_proxy = proxy.clone();
                                    std::thread::spawn(move || {
                                        let chosen = pollster::block_on(
                                            rfd::AsyncFileDialog::new()
                                                .add_filter(
                                                    "Images and GIF",
                                                    &["png", "gif", "jpg", "jpeg", "webp"],
                                                )
                                                .pick_file(),
                                        )
                                        .map(|file| file.path().to_string_lossy().into_owned());

                                        let _ =
                                            dialog_proxy.send_event(AppEvent::ImageChosen(chosen));
                                    });
                                }
                                Some(PrefAction::ClearImage) => {
                                    settings.branding_image = "default".into();
                                    settings.branding_enabled = false;
                                    gpu.apply_settings(settings.clone());
                                }
                                Some(PrefAction::GifFpsDown) => {
                                    settings.branding_max_fps = settings.branding_max_fps.saturating_sub(1).max(1);
                                }
                                Some(PrefAction::GifFpsUp) => {
                                    settings.branding_max_fps = (settings.branding_max_fps + 1).min(30);
                                }
                                Some(PrefAction::Save) => {
                                    let _ = settings.save();
                                    preferences_backup = None;
                                    preferences.close();
                                }
                                Some(PrefAction::Cancel) => {
                                    if let Some(original) = preferences_backup.take() {
                                        settings = original;
                                        gpu.apply_settings(settings.clone());
                                        terminal.set_scrollback_limit(settings.scrollback);
                                    }
                                    preferences.close();
                                }
                                None => {}
                            }

                            let size = window.inner_size();
                            let (cols, rows) = grid_size(size, &settings);
                            let (old_cols, old_rows) = terminal.dimensions();
                            if cols as usize != old_cols || rows as usize != old_rows {
                                terminal.resize(cols as usize, rows as usize);
                                pty.resize(
                                    cols,
                                    rows,
                                    size.width.min(u16::MAX as u32) as u16,
                                    size.height.min(u16::MAX as u32) as u16,
                                );
                            }
                            gpu.update_terminal_text(&terminal);
                            dirty = true;
                            window.request_redraw();
                        }
                    } else if context_menu.visible {
                        if state == ElementState::Pressed {
                            let action = context_menu
                                .action_at(mouse_pos.x as f32, mouse_pos.y as f32);
                            context_menu.close();

                            match action {
                                Some(MenuAction::Copy) => {
                                    if let (Some(selection_range), Some(clipboard)) =
                                        (selection, clipboard.as_mut())
                                    {
                                        let text = terminal
                                            .selected_text(selection_range.0, selection_range.1);
                                        if !text.is_empty() {
                                            let _ = clipboard.set_text(text);
                                        }
                                    }
                                }
                                Some(MenuAction::Paste) => {
                                    if let Some(clipboard) = clipboard.as_mut() {
                                        if let Ok(text) = clipboard.get_text() {
                                            terminal.scroll_to_bottom();
                                            pty.write(text.as_bytes());
                                            selection = None;
                                        }
                                    }
                                }
                                Some(MenuAction::SelectAll) => {
                                    selection = Some(terminal.select_all_visible());
                                }
                                Some(MenuAction::NewWindow) => {
                                    if let Ok(exe) = std::env::current_exe() {
                                        let _ = std::process::Command::new(exe).spawn();
                                    }
                                }
                                Some(MenuAction::IncreaseFont) => {
                                    selection = None;
                                    terminal.scroll_to_bottom();
                                    settings.zoom_by(1.10);
                                    gpu.apply_settings(settings.clone());

                                    let size = window.inner_size();
                                    let (cols, rows) = grid_size(size, &settings);
                                    terminal.resize(cols as usize, rows as usize);
                                    pty.resize(
                                        cols,
                                        rows,
                                        size.width.min(u16::MAX as u32) as u16,
                                        size.height.min(u16::MAX as u32) as u16,
                                    );
                                    gpu.update_terminal_text(&terminal);
                                }
                                Some(MenuAction::DecreaseFont) => {
                                    selection = None;
                                    terminal.scroll_to_bottom();
                                    settings.zoom_by(1.0 / 1.10);
                                    gpu.apply_settings(settings.clone());

                                    let size = window.inner_size();
                                    let (cols, rows) = grid_size(size, &settings);
                                    terminal.resize(cols as usize, rows as usize);
                                    pty.resize(
                                        cols,
                                        rows,
                                        size.width.min(u16::MAX as u32) as u16,
                                        size.height.min(u16::MAX as u32) as u16,
                                    );
                                    gpu.update_terminal_text(&terminal);
                                }
                                Some(MenuAction::ResetFont) => {
                                    selection = None;
                                    terminal.scroll_to_bottom();
                                    let mut fresh = Settings::load();
                                    fresh.apply_scale_factor(window.scale_factor());
                                    settings = fresh;
                                    gpu.apply_settings(settings.clone());

                                    let size = window.inner_size();
                                    let (cols, rows) = grid_size(size, &settings);
                                    terminal.resize(cols as usize, rows as usize);
                                    pty.resize(
                                        cols,
                                        rows,
                                        size.width.min(u16::MAX as u32) as u16,
                                        size.height.min(u16::MAX as u32) as u16,
                                    );
                                    gpu.update_terminal_text(&terminal);
                                }
                                Some(MenuAction::ClearScrollback) => {
                                    terminal.clear_scrollback();
                                    selection = None;
                                }
                                Some(MenuAction::Preferences) => {
                                    preferences_backup = Some(settings.clone());
                                    preferences.open(
                                        gpu.config.width,
                                        gpu.config.height,
                                        settings.scale_factor,
                                    );
                                }
                                Some(MenuAction::EditConfig) => {
                                    let path = config_path();
                                    let launched = std::process::Command::new("kate")
                                        .arg(&path)
                                        .spawn()
                                        .is_ok();
                                    if !launched {
                                        let _ = std::process::Command::new("xdg-open")
                                            .arg(path)
                                            .spawn();
                                    }
                                }
                                Some(MenuAction::Quit) => {
                                    elwt.exit();
                                }
                                None => {}
                            }

                            dirty = true;
                            window.request_redraw();
                        }
                    } else {
                        match state {
                            ElementState::Pressed => {
                                let cell = mouse_to_cell(mouse_pos, &terminal, &settings);
                                selection = Some((cell, cell));
                                selecting = true;
                            }
                            ElementState::Released => {
                                selecting = false;
                            }
                        }

                        dirty = true;
                        window.request_redraw();
                    }
                }
                WindowEvent::MouseWheel { delta, .. } => {
                    if preferences.visible {
                        return;
                    }
                    if context_menu.visible {
                        context_menu.close();
                    }

                    let rows = match delta {
                        MouseScrollDelta::LineDelta(_, y) => {
                            if y > 0.0 { 3 } else if y < 0.0 { -3 } else { 0 }
                        }
                        MouseScrollDelta::PixelDelta(pos) => {
                            if pos.y > 0.0 { 3 } else if pos.y < 0.0 { -3 } else { 0 }
                        }
                    };

                    if rows != 0 {
                        terminal.scroll_view(rows);
                        selection = None;
                        gpu.update_terminal_text(&terminal);
                        dirty = true;
                        window.request_redraw();
                    }
                }
                WindowEvent::Resized(size) => {
                    gpu.resize(size);
                    if preferences.visible && size.width > 0 && size.height > 0 {
                        preferences.open(size.width, size.height, settings.scale_factor);
                    }

                    let (cols, rows) = grid_size(size, &settings);
                    let (old_cols, old_rows) = terminal.dimensions();
                    if cols as usize != old_cols || rows as usize != old_rows {
                        terminal.resize(cols as usize, rows as usize);
                        pty.resize(
                            cols,
                            rows,
                            size.width.min(u16::MAX as u32) as u16,
                            size.height.min(u16::MAX as u32) as u16,
                        );
                        gpu.update_terminal_text(&terminal);
                    }

                    dirty = true;
                    window.request_redraw();
                }
                WindowEvent::RedrawRequested => {
                    if terminal.take_dirty() {
                        gpu.update_terminal_text(&terminal);
                        dirty = true;
                    }

                    if dirty {
                        if let Err(err) = gpu.render(&terminal, selection, &context_menu, &preferences) {
                            eprintln!("render error: {err:#}");
                        }
                        dirty = false;
                    }
                }
                _ => {}
            },
            _ => {}
        }
    })?;

    Ok(())
}
