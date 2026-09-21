#[cfg(not(target_os = "linux"))]
compile_error!("Hafþi currently supports Linux/Wayland only.");

mod background;
mod menu;
mod preferences;
mod pty;
mod settings;
mod terminal;
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
    adapter_name: String,
    alpha_mode: CompositeAlphaMode,
    settings: Settings,

    font_system: FontSystem,
    swash_cache: SwashCache,
    atlas: TextAtlas,
    text_renderer: TextRenderer,
    text_buffer: Buffer,
    menu_buffer: Buffer,
    prefs_buffer: Buffer,
    prefs_minus_buffer: Buffer,
    prefs_plus_buffer: Buffer,
    prefs_toggle_buffer: Buffer,
    prefs_choose_buffer: Buffer,
    prefs_cancel_buffer: Buffer,
    prefs_save_buffer: Buffer,
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

        let menu_font_size = settings.font_size * 0.78;
        let menu_line_height = settings.line_height * 0.92;
        let mut menu_buffer = Buffer::new(
            &mut font_system,
            Metrics::new(menu_font_size, menu_line_height),
        );
        menu_buffer.set_size(&mut font_system, config.width as f32, config.height as f32);

        let mut prefs_buffer = Buffer::new(
            &mut font_system,
            Metrics::new(settings.font_size * 0.86, settings.line_height * 1.08),
        );
        prefs_buffer.set_size(&mut font_system, config.width as f32, config.height as f32);

        let button_metrics = Metrics::new(settings.font_size * 0.72, settings.line_height * 0.96);

        let mut prefs_minus_buffer = Buffer::new(&mut font_system, button_metrics);
        prefs_minus_buffer.set_text(&mut font_system, "−", Attrs::new().family(Family::Name(&settings.font_family)), Shaping::Advanced);
        prefs_minus_buffer.shape_until_scroll(&mut font_system);

        let mut prefs_plus_buffer = Buffer::new(&mut font_system, button_metrics);
        prefs_plus_buffer.set_text(&mut font_system, "+", Attrs::new().family(Family::Name(&settings.font_family)), Shaping::Advanced);
        prefs_plus_buffer.shape_until_scroll(&mut font_system);

        let mut prefs_toggle_buffer = Buffer::new(&mut font_system, button_metrics);
        prefs_toggle_buffer.set_text(&mut font_system, "On / Off", Attrs::new().family(Family::Name(&settings.font_family)), Shaping::Advanced);
        prefs_toggle_buffer.shape_until_scroll(&mut font_system);

        let mut prefs_choose_buffer = Buffer::new(&mut font_system, button_metrics);
        prefs_choose_buffer.set_text(&mut font_system, "Choose image…", Attrs::new().family(Family::Name(&settings.font_family)), Shaping::Advanced);
        prefs_choose_buffer.shape_until_scroll(&mut font_system);

        let mut prefs_cancel_buffer = Buffer::new(&mut font_system, button_metrics);
        prefs_cancel_buffer.set_text(&mut font_system, "Cancel", Attrs::new().family(Family::Name(&settings.font_family)), Shaping::Advanced);
        prefs_cancel_buffer.shape_until_scroll(&mut font_system);

        let mut prefs_save_buffer = Buffer::new(&mut font_system, button_metrics);
        prefs_save_buffer.set_text(&mut font_system, "Save", Attrs::new().family(Family::Name(&settings.font_family)), Shaping::Advanced);
        prefs_save_buffer.shape_until_scroll(&mut font_system);

        Ok(Self {
            surface,
            device,
            queue,
            config,
            size,
            adapter_name: info.name,
            alpha_mode,
            settings,
            font_system,
            swash_cache,
            atlas,
            text_renderer,
            text_buffer,
            menu_buffer,
            prefs_buffer,
            prefs_minus_buffer,
            prefs_plus_buffer,
            prefs_toggle_buffer,
            prefs_choose_buffer,
            prefs_cancel_buffer,
            prefs_save_buffer,
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
        self.prefs_buffer
            .set_size(&mut self.font_system, size.width as f32, size.height as f32);
        for buffer in [
            &mut self.prefs_minus_buffer,
            &mut self.prefs_plus_buffer,
            &mut self.prefs_toggle_buffer,
            &mut self.prefs_choose_buffer,
            &mut self.prefs_cancel_buffer,
            &mut self.prefs_save_buffer,
        ] {
            buffer.set_size(&mut self.font_system, size.width as f32, size.height as f32);
        }
    }

    fn menu_font_size(&self) -> f32 {
        self.settings.font_size * 0.78
    }

    fn menu_line_height(&self) -> f32 {
        self.settings.line_height * 0.92
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
        let menu_font_size = font_size * 0.78;
        let menu_line_height = line_height * 0.92;

        self.text_buffer.set_metrics(
            &mut self.font_system,
            Metrics::new(font_size, line_height),
        );
        self.menu_buffer.set_metrics(
            &mut self.font_system,
            Metrics::new(menu_font_size, menu_line_height),
        );
        self.prefs_buffer.set_metrics(
            &mut self.font_system,
            Metrics::new(font_size * 0.86, line_height * 1.08),
        );
        let button_metrics = Metrics::new(font_size * 0.72, line_height * 0.96);
        for buffer in [
            &mut self.prefs_minus_buffer,
            &mut self.prefs_plus_buffer,
            &mut self.prefs_toggle_buffer,
            &mut self.prefs_choose_buffer,
            &mut self.prefs_cancel_buffer,
            &mut self.prefs_save_buffer,
        ] {
            buffer.set_metrics(&mut self.font_system, button_metrics);
        }
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

    fn preferences_text(&self) -> String {
        let image_chars: Vec<char> = self.settings.branding_image.chars().collect();
        let image = if image_chars.len() > 34 {
            format!("…{}", image_chars[image_chars.len() - 33..].iter().collect::<String>())
        } else {
            self.settings.branding_image.clone()
        };

        format!(
            "Font size                 {:.1} px\n\
Transparency              {:>3}%\n\
Padding                   {:.0} px\n\
Scrollback                {:>6}\n\
Background image          {:<8}\n\
Image / GIF               {:<34}\n\
GIF max FPS               {:>2}\n\
\n",
            self.settings.font_size,
            (self.settings.opacity * 100.0).round() as u32,
            self.settings.padding / self.settings.scale_factor.max(1.0),
            self.settings.scrollback,
            if self.settings.branding_enabled { "On" } else { "Off" },
            image,
            self.settings.branding_max_fps,
        )
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
            self.menu_buffer.set_text(
                &mut self.font_system,
                &menu.text(),
                Attrs::new()
                    .family(Family::Name(&self.settings.font_family))
                    .color(Color::rgb(
                        self.settings.foreground.r,
                        self.settings.foreground.g,
                        self.settings.foreground.b,
                    )),
                Shaping::Advanced,
            );
            self.menu_buffer.shape_until_scroll(&mut self.font_system);
        }

        if prefs.visible {
            let prefs_text = self.preferences_text();
            self.prefs_buffer.set_text(
                &mut self.font_system,
                &prefs_text,
                Attrs::new()
                    .family(Family::Name(&self.settings.font_family))
                    .color(Color::rgb(
                        self.settings.foreground.r,
                        self.settings.foreground.g,
                        self.settings.foreground.b,
                    )),
                Shaping::Advanced,
            );
            self.prefs_buffer.shape_until_scroll(&mut self.font_system);

        }

        let terminal_area = TextArea {
            buffer: &self.text_buffer,
            left: self.settings.padding,
            top: self.settings.padding,
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
            let prefs_area = TextArea {
                buffer: &self.prefs_buffer,
                left: prefs.x + 18.0 * self.settings.scale_factor,
                top: prefs.y + 10.0 * self.settings.scale_factor,
                scale: 1.0,
                bounds: TextBounds {
                    left: prefs.x as i32,
                    top: prefs.y as i32,
                    right: (prefs.x + prefs.width) as i32,
                    bottom: (prefs.y + prefs.height()) as i32,
                },
                default_color: Color::rgb(
                    self.settings.foreground.r,
                    self.settings.foreground.g,
                    self.settings.foreground.b,
                ),
            };

            let mut areas = vec![terminal_area, prefs_area];

            for button in prefs.button_rects() {
                let (buffer, x_factor) = match button.action {
                    PrefAction::FontDown
                    | PrefAction::OpacityDown
                    | PrefAction::PaddingDown
                    | PrefAction::ScrollbackDown
                    | PrefAction::GifFpsDown => (&self.prefs_minus_buffer, 0.43),

                    PrefAction::FontUp
                    | PrefAction::OpacityUp
                    | PrefAction::PaddingUp
                    | PrefAction::ScrollbackUp
                    | PrefAction::GifFpsUp => (&self.prefs_plus_buffer, 0.43),

                    PrefAction::ToggleBranding => (&self.prefs_toggle_buffer, 0.16),
                    PrefAction::ChooseImage => (&self.prefs_choose_buffer, 0.10),
                    PrefAction::Cancel => (&self.prefs_cancel_buffer, 0.20),
                    PrefAction::Save => (&self.prefs_save_buffer, 0.28),
                };

                areas.push(TextArea {
                    buffer,
                    left: button.x + button.w * x_factor,
                    top: button.y,
                    scale: 1.0,
                    bounds: TextBounds {
                        left: button.x as i32,
                        top: button.y as i32,
                        right: (button.x + button.w) as i32,
                        bottom: (button.y + button.h) as i32,
                    },
                    default_color: Color::rgb(
                        self.settings.foreground.r,
                        self.settings.foreground.g,
                        self.settings.foreground.b,
                    ),
                });
            }

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
            let menu_area = TextArea {
                buffer: &self.menu_buffer,
                left: menu.x + 12.0 * self.settings.scale_factor,
                top: menu.y,
                scale: 1.0,
                bounds: TextBounds {
                    left: menu.x as i32,
                    top: menu.y as i32,
                    right: (menu.x + menu.width) as i32,
                    bottom: (menu.y + menu.height()) as i32,
                },
                default_color: Color::rgb(
                    self.settings.foreground.r,
                    self.settings.foreground.g,
                    self.settings.foreground.b,
                ),
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
                    [terminal_area, menu_area],
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
                let y = self.settings.padding + row as f32 * self.settings.line_height;
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
                self.settings.padding + cy as f32 * self.settings.line_height,
                cursor_w.max(1.0),
                self.settings.line_height,
                Settings::rgba_f32(self.settings.cursor, 0.55),
            );
        }

        if prefs.visible {
            RectRenderer::push_rect(
                &mut rect_vertices,
                self.config.width,
                self.config.height,
                prefs.x,
                prefs.y,
                prefs.width,
                prefs.height(),
                [0.965, 0.957, 0.945, 1.0],
            );

            for button in prefs.button_rects() {
                RectRenderer::push_rect(
                    &mut rect_vertices,
                    self.config.width,
                    self.config.height,
                    button.x,
                    button.y,
                    button.w,
                    button.h,
                    if prefs.hovered == Some(button.action) {
                        [0.78, 0.84, 0.90, 1.0]
                    } else {
                        [0.89, 0.89, 0.87, 1.0]
                    },
                );
            }
        }

        if menu.visible {
            // Opaque menu surface for readability over transparent terminals.
            RectRenderer::push_rect(
                &mut rect_vertices,
                self.config.width,
                self.config.height,
                menu.x,
                menu.y,
                menu.width,
                menu.height(),
                Settings::rgba_f32(self.settings.background, 0.98),
            );

            if let Some(index) = menu.hovered {
                RectRenderer::push_rect(
                    &mut rect_vertices,
                    self.config.width,
                    self.config.height,
                    menu.x + 2.0,
                    menu.y + index as f32 * menu.row_height,
                    menu.width - 4.0,
                    menu.row_height,
                    Settings::rgba_f32(self.settings.selection_background, 0.95),
                );
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
                self.background_renderer.draw(
                    &mut pass,
                    &self.queue,
                    self.config.width,
                    self.config.height,
                    self.settings.opacity as f32,
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

fn grid_size(size: PhysicalSize<u32>, settings: &Settings) -> (u16, u16) {
    let usable_w = (size.width as f32 - settings.padding * 2.0).max(settings.cell_width);
    let usable_h = (size.height as f32 - settings.padding * 2.0).max(settings.line_height);
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
        (((position.y as f32) - settings.padding).max(0.0) / settings.line_height).floor() as usize;
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
                    settings.branding_enabled = true;
                    gpu.apply_settings(settings.clone());
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
                                Some(PrefAction::ToggleBranding) => {
                                    settings.branding_enabled = !settings.branding_enabled;
                                    gpu.apply_settings(settings.clone());
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
