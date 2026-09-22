#[cfg(not(target_os = "linux"))]
compile_error!("Hafþi currently supports Linux/Wayland only.");

mod background;
mod diagnostics;
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
use preferences::{PrefAction, PrefPage, PreferencesPanel, TEXT_SWATCHES};
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

        // The downlevel default limits 2D textures to 2048 pixels, which is
        // smaller than a normal high-DPI desktop window. Request the adapter's
        // actual surface size limit while keeping the other conservative limits.
        let adapter_limits = adapter.limits();
        let max_surface_dimension = adapter_limits.max_texture_dimension_2d;
        diagnostics::record(&format!(
            "GPU: {} ({:?}), max surface dimension: {max_surface_dimension}",
            info.name, info.backend
        ));
        let required_limits = Limits {
            max_texture_dimension_2d: max_surface_dimension,
            ..Limits::downlevel_defaults()
        };

        let (device, queue) = adapter
            .request_device(
                &DeviceDescriptor {
                    label: Some("Hafþi device"),
                    required_features: Features::empty(),
                    required_limits,
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
        anyhow::ensure!(
            config.width <= max_surface_dimension && config.height <= max_surface_dimension,
            "window size {}x{} exceeds GPU maximum surface dimension {max_surface_dimension}",
            config.width,
            config.height,
        );
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

        let max = self.device.limits().max_texture_dimension_2d;
        if size.width > max || size.height > max {
            diagnostics::record(&format!(
                "window resize {}x{} exceeds GPU maximum surface dimension {max}",
                size.width, size.height
            ));
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
        let reload_background = settings.branding_enabled
            && (!self.settings.branding_enabled
                || settings.branding_image != self.settings.branding_image);
        let font_changed = settings.font_size != self.settings.font_size
            || settings.line_height != self.settings.line_height;
        self.settings = settings;

        if reload_background {
            if let Err(err) = self.background_renderer.load(
                &self.device,
                &self.queue,
                &self.settings.branding_image,
            ) {
                eprintln!("Hafþi background: {err:#}");
            }
        }

        if !font_changed {
            return;
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
            let primary = Color::rgb(235, 239, 242);
            let muted = Color::rgb(164, 176, 185);
            let accent = Color::rgb(117, 188, 231);
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

            add("Hafþi", 19.0, 18.0, 155.0, 19.0, primary);
            add("PREFERENCES", 19.0, 49.0, 150.0, 10.0, muted);
            for page in PrefPage::ALL {
                let y = 89.0 + PrefPage::ALL.iter().position(|item| *item == page).unwrap() as f32 * 43.0;
                add(page.title(), 42.0, y, 139.0, 14.0,
                    if prefs.page == page { primary } else { muted });
            }
            add(prefs.page.title(), 213.0, 21.0, 350.0, 19.0, primary);
            match prefs.page {
                PrefPage::Appearance => {
                    add("LOOK & FEEL", 226.0, 91.0, 180.0, 11.0, accent);
                    add("Font size", 226.0, 133.0, 170.0, 15.0, primary);
                    add(&format!("{:.1} px", self.settings.font_size), 544.0, 134.0, 78.0, 13.0, muted);
                    add("Window opacity", 226.0, 190.0, 210.0, 15.0, primary);
                    add(&format!("{}%", (self.settings.opacity * 100.0).round() as u32),
                        629.0, 190.0, 67.0, 14.0, accent);
                    add("Padding", 226.0, 296.0, 170.0, 15.0, primary);
                    add(&format!("{:.0} px", self.settings.logical_padding()),
                        544.0, 297.0, 78.0, 13.0, muted);
                    add("Drag or click the blue bar to adjust window opacity.",
                        226.0, 356.0, 460.0, 12.0, muted);
                }
                PrefPage::Terminal => {
                    add("FONT", 226.0, 91.0, 180.0, 11.0, accent);
                    add(&truncate_label(&self.settings.font_family, 36),
                        226.0, 131.0, 394.0, 15.0, primary);
                    add("TEXT COLOR", 226.0, 200.0, 180.0, 11.0, accent);
                    add("Custom hex", 226.0, 283.0, 220.0, 14.0, primary);
                    let hex = if prefs.color_editing {
                        format!("#{}▏", prefs.color_input)
                    } else {
                        format!("#{:02X}{:02X}{:02X}", self.settings.foreground.r,
                            self.settings.foreground.g, self.settings.foreground.b)
                    };
                    add(&hex, 540.0, 284.0, 155.0, 13.0,
                        if prefs.color_error { Color::rgb(245, 136, 136) } else { primary });
                    add("HISTORY", 226.0, 336.0, 180.0, 11.0, accent);
                    add("Scrollback lines", 226.0, 357.0, 200.0, 15.0, primary);
                    add(&self.settings.scrollback.to_string(), 530.0, 358.0, 90.0, 13.0, muted);
                }
                PrefPage::Background => {
                    add("IMAGE DISPLAY", 226.0, 91.0, 240.0, 11.0, accent);
                    add("Image / GIF", 226.0, 189.0, 260.0, 15.0, primary);
                    let image = &self.settings.branding_image;
                    let filename = std::path::Path::new(image).file_name()
                        .and_then(|name| name.to_str()).unwrap_or(image);
                    let filename = if image == "default" || image.is_empty() {
                        "No image selected".to_string()
                    } else { truncate_label(filename, 35) };
                    add(&filename, 226.0, 223.0, 450.0, 14.0, primary);
                    if image != "default" && !image.is_empty() {
                        add(&truncate_label(image, 58), 226.0, 244.0, 468.0, 10.0, muted);
                    }
                    add("GIF max FPS", 226.0, 354.0, 200.0, 15.0, primary);
                    add(&self.settings.branding_max_fps.to_string(),
                        566.0, 355.0, 56.0, 13.0, muted);
                }
                PrefPage::CommandHelp => {
                    add("OPTIONAL ASSISTANT", 226.0, 91.0, 350.0, 11.0, accent);
                    add("Linux command help", 226.0, 132.0, 330.0, 16.0, primary);
                    add("Describe what you want to do in Linux.",
                        226.0, 159.0, 470.0, 12.0, muted);
                    add("YOUR QUESTION", 226.0, 209.0, 280.0, 11.0, accent);
                    let question = if prefs.question_input.is_empty() {
                        "How do I find a file?".to_string()
                    } else {
                        let text = prefs.question_input.chars().rev().take(48).collect::<String>()
                            .chars().rev().collect::<String>();
                        format!("{}{}", if prefs.question_input.chars().count() > 48 { "…" } else { "" }, text)
                    };
                    add(&question, 239.0, 242.0, 452.0, 14.0,
                        if prefs.question_input.is_empty() { muted } else { primary });
                    if prefs.question_editing { add("▏", 683.0, 242.0, 14.0, 14.0, accent); }
                    add("Answer appears in the terminal. Review commands before use.",
                        226.0, 301.0, 357.0, 11.0, muted);
                    add("Your question is sent online via Pollinations.",
                        370.0, 360.0, 330.0, 11.0, muted);
                }
            }
            for button in prefs.button_rects() {
                let caption = match button.action {
                    PrefAction::FontDown
                    | PrefAction::FontFamilyPrev
                    | PrefAction::PaddingDown
                    | PrefAction::ScrollbackDown
                    | PrefAction::GifFpsDown => "−",
                    PrefAction::FontUp
                    | PrefAction::FontFamilyNext
                    | PrefAction::PaddingUp
                    | PrefAction::ScrollbackUp
                    | PrefAction::GifFpsUp => "+",
                    PrefAction::ImageOff => "Off",
                    PrefAction::ImageBanner => "Banner",
                    PrefAction::ImageFull => "Full",
                    PrefAction::ChooseImage => "Choose…",
                    PrefAction::ClearImage => "Clear",
                    PrefAction::ToggleCommandHelp => if self.settings.command_help_enabled { "On" } else { "Off" },
                    PrefAction::AskQuestion => "Ask tgpt",
                    PrefAction::InstallTgpt => "Install tgpt",
                    PrefAction::Cancel => "Cancel",
                    PrefAction::Save => "Save",
                    PrefAction::SelectPage(_) | PrefAction::OpacitySet(_)
                    | PrefAction::TextColor(_) | PrefAction::EditTextColor
                    | PrefAction::EditQuestion => continue,
                };
                let font_size = if caption == "+" || caption == "−" { 18.0 } else { 13.0 };
                let text_width = caption.chars().count() as f32 * font_size * 0.53;
                let x = (button.x - prefs.x) / prefs.scale
                    + (button.w / prefs.scale - text_width) / 2.0;
                let y = (button.y - prefs.y) / prefs.scale + if font_size > 13.0 { 3.0 } else { 8.0 };
                let color = if button.action == PrefAction::Save {
                    Color::rgb(245, 250, 253)
                } else { primary };
                add(caption, x, y, button.w / prefs.scale, font_size, color);
            }
            drop(add);
            // Text is drawn after all rectangles. Leave terminal glyphs out of
            // this pass so they cannot show through the preferences window.
            let mut areas = Vec::new();
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
            // Dim the terminal and draw the preferences with opaque, dark
            // surfaces independent of the terminal background opacity.
            RectRenderer::push_rect(
                &mut rect_vertices, self.config.width, self.config.height,
                0.0, 0.0, self.config.width as f32, self.config.height as f32,
                [0.02, 0.03, 0.04, 0.55],
            );
            RectRenderer::push_rounded_rect(
                &mut rect_vertices,
                self.config.width,
                self.config.height,
                prefs.x - 2.0 * scale, prefs.y - 2.0 * scale,
                prefs.width + 4.0 * scale, prefs.height() + 4.0 * scale,
                10.0 * scale, [0.12, 0.14, 0.16, 1.0],
            );
            RectRenderer::push_rounded_rect(
                &mut rect_vertices,
                self.config.width,
                self.config.height,
                prefs.x, prefs.y, prefs.width, prefs.height(),
                8.0 * scale, [0.045, 0.052, 0.060, 1.0],
            );
            let mut shape = |x: f32, y: f32, w: f32, h: f32, radius: f32, color| {
                let (x, y) = prefs.pos(x, y);
                RectRenderer::push_rounded_rect(
                    &mut rect_vertices, self.config.width, self.config.height,
                    x, y, w * scale, h * scale, radius * scale, color,
                );
            };
            shape(0.0, 0.0, PreferencesPanel::SIDEBAR_WIDTH, 476.0, 8.0,
                [0.027, 0.033, 0.040, 1.0]);
            shape(8.0, 0.0, 184.0, 476.0, 0.0, [0.027, 0.033, 0.040, 1.0]);
            shape(191.0, 0.0, 1.0, 476.0, 0.0, [0.12, 0.14, 0.16, 1.0]);
            shape(0.0, 67.0, 736.0, 1.0, 0.0, [0.12, 0.14, 0.16, 1.0]);
            shape(207.0, 416.0, 514.0, 1.0, 0.0, [0.10, 0.12, 0.14, 1.0]);
            for page in PrefPage::ALL {
                let index = PrefPage::ALL.iter().position(|item| *item == page).unwrap() as f32;
                let row_y = 83.0 + index * 43.0;
                if prefs.page == page || prefs.hovered_page == Some(page) {
                    let color = if prefs.page == page {
                        [0.035, 0.10, 0.16, 1.0]
                    } else {
                        [0.08, 0.095, 0.11, 1.0]
                    };
                    shape(9.0, row_y, 174.0, 37.0, 4.0, color);
                    if prefs.page == page {
                        shape(9.0, row_y, 3.0, 37.0, 0.0, [0.37, 0.69, 0.87, 1.0]);
                    }
                }
                let y = 95.0 + index * 43.0;
                let icon_color = if prefs.page == page {
                    [0.45, 0.75, 0.93, 1.0]
                } else { [0.52, 0.60, 0.65, 1.0] };
                shape(23.0, y, 11.0, 11.0, 3.0, icon_color);
                if page == PrefPage::Terminal {
                    shape(25.0, y + 3.0, 6.0, 2.0, 0.0, [0.12, 0.16, 0.19, 1.0]);
                }
            }
            let card = [0.055, 0.064, 0.075, 1.0];
            let card_border = [0.12, 0.14, 0.16, 1.0];
            match prefs.page {
                PrefPage::Appearance => {
                    shape(208.0, 78.0, 512.0, 319.0, 5.0, card_border);
                    shape(209.0, 79.0, 510.0, 317.0, 4.0, card);
                    shape(226.0, 175.0, 476.0, 1.0, 0.0, [0.11, 0.13, 0.15, 1.0]);
                    shape(226.0, 273.0, 476.0, 1.0, 0.0, [0.11, 0.13, 0.15, 1.0]);
                    let progress = self.settings.opacity.clamp(0.0, 1.0) as f32;
                    shape(226.0, 235.0, 452.0, 5.0, 2.5, [0.16, 0.19, 0.22, 1.0]);
                    shape(226.0, 235.0, (452.0 * progress).max(2.0), 5.0, 2.5,
                        [0.32, 0.65, 0.86, 1.0]);
                    shape(226.0 + 452.0 * progress - 7.0, 230.0, 15.0, 15.0, 7.5,
                        [0.55, 0.77, 0.90, 1.0]);
                }
                PrefPage::Terminal => {
                    for (y, h) in [(78.0, 100.0), (188.0, 137.0), (331.0, 66.0)] {
                        shape(208.0, y, 512.0, h, 5.0, card_border);
                        shape(209.0, y + 1.0, 510.0, h - 2.0, 4.0, card);
                    }
                }
                PrefPage::CommandHelp => {
                    for (y, h) in [(78.0, 100.0), (191.0, 144.0), (341.0, 56.0)] {
                        shape(208.0, y, 512.0, h, 5.0, card_border);
                        shape(209.0, y + 1.0, 510.0, h - 2.0, 4.0, card);
                    }
                }
                PrefPage::Background => {
                    for (y, h) in [(78.0, 100.0), (182.0, 131.0), (326.0, 70.0)] {
                        shape(208.0, y, 512.0, h, 5.0, card_border);
                        shape(209.0, y + 1.0, 510.0, h - 2.0, 4.0, card);
                    }
                }
            }
            let selected = if !self.settings.branding_enabled {
                PrefAction::ImageOff
            } else if self.settings.branding_mode == "banner" {
                PrefAction::ImageBanner
            } else {
                PrefAction::ImageFull
            };
            for button in prefs.button_rects() {
                if let PrefAction::TextColor(index) = button.action {
                    let swatch = TEXT_SWATCHES[index as usize];
                    let bx = (button.x - prefs.x) / scale;
                    let by = (button.y - prefs.y) / scale;
                    let active = self.settings.foreground == swatch;
                    shape(bx - 2.0, by - 2.0, 40.0, 36.0, 6.0,
                        if active { [0.36, 0.72, 0.95, 1.0] }
                        else { [0.18, 0.21, 0.24, 1.0] });
                    shape(bx, by, 36.0, 32.0, 4.0, Settings::rgba_f32(swatch, 1.0));
                    continue;
                }
                let active = button.action == selected || button.action == PrefAction::Save
                    || (button.action == PrefAction::ToggleCommandHelp && self.settings.command_help_enabled);
                let hover = prefs.hovered == Some(button.action);
                let color = if active {
                    if hover { [0.12, 0.41, 0.63, 1.0] }
                    else { [0.07, 0.30, 0.49, 1.0] }
                } else if hover { [0.12, 0.14, 0.17, 1.0] }
                else { [0.08, 0.095, 0.11, 1.0] };
                let bx = (button.x - prefs.x) / scale;
                let by = (button.y - prefs.y) / scale;
                let bw = button.w / scale;
                let bh = button.h / scale;
                shape(bx - 1.0, by - 1.0, bw + 2.0, bh + 2.0, 5.0,
                    if active { [0.12, 0.43, 0.66, 1.0] }
                    else { [0.14, 0.16, 0.18, 1.0] },
                );
                shape(bx, by, bw, bh, 4.0, color);
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

fn copy_selection_to_clipboard(
    terminal: &TerminalGrid,
    selection: Option<((usize, usize), (usize, usize))>,
    clipboard: Option<&mut Clipboard>,
    recent_copy: &mut Option<String>,
) {
    let Some((start, end)) = selection else {
        diagnostics::record("clipboard copy requested without a selection");
        return;
    };
    let text = terminal.selected_text(start, end);
    if text.is_empty() {
        diagnostics::record("clipboard copy selection is empty");
        return;
    }
    *recent_copy = Some(text.clone());
    match clipboard {
        Some(clipboard) => {
            if let Err(err) = clipboard.set_text(text) {
                diagnostics::record(&format!("clipboard copy failed: {err}"));
            }
        }
        None => diagnostics::record("clipboard unavailable while copying"),
    }
}

fn clipboard_text(clipboard: Option<&mut Clipboard>, recent_copy: &Option<String>) -> Option<String> {
    match clipboard {
        Some(clipboard) => match clipboard.get_text() {
            Ok(text) => Some(text),
            Err(err) => {
                diagnostics::record(&format!("clipboard paste failed: {err}"));
                recent_copy.clone()
            }
        },
        None => recent_copy.clone(),
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
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        diagnostics::record(&format!("panic: {info}"));
        original_hook(info);
    }));
    diagnostics::record("starting");
    let result = run();
    match &result {
        Ok(()) => diagnostics::record("event loop ended"),
        Err(err) => diagnostics::record(&format!("error: {err:#}")),
    }
    result
}

fn response_locale(lc_all: Option<&str>, language: Option<&str>, lc_messages: Option<&str>, lang: Option<&str>) -> String {
    let selected = [lc_all, language, lc_messages, lang]
        .into_iter().flatten().find(|value| !value.is_empty()).unwrap_or("en");
    let code = selected.split(':').next().unwrap_or("en")
        .split('.').next().unwrap_or("en");
    if code == "C" || code == "POSIX" { return "en".into(); }
    if code.is_empty() || code.len() > 24
        || !code.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return "en".into();
    }
    code.to_string()
}

fn system_response_locale() -> String {
    let lc_all = std::env::var("LC_ALL").ok();
    let language = std::env::var("LANGUAGE").ok();
    let lc_messages = std::env::var("LC_MESSAGES").ok();
    let lang = std::env::var("LANG").ok();
    response_locale(lc_all.as_deref(), language.as_deref(), lc_messages.as_deref(), lang.as_deref())
}

fn tgpt_query_command(question: &str) -> String {
    let locale = system_response_locale();
    let prompt = format!(
        "Answer only questions about Linux terminal commands. Respond in the language of the user's Linux locale ({locale}), with a short explanation, a command example and what it does. If unrelated to Linux commands, say you only help with Linux commands. Never suggest running a command automatically. Question: {question}"
    );
    let quoted = prompt.replace('\'', "'\\''");
    format!("if command -v tgpt >/dev/null 2>&1; then tgpt --provider pollinations --quiet --whole '{quoted}'; else printf 'tgpt is not installed. Install it with: sudo pacman -S tgpt\\n'; fi\n")
}

fn append_question_text(input: &mut String, key: &Key) {
    match key {
        Key::Named(NamedKey::Space) => {
            if input.chars().count() < 200 { input.push(' '); }
        }
        Key::Character(ch) => {
            for character in ch.chars().filter(|c| !c.is_control()) {
                if input.chars().count() < 200 { input.push(character); }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod command_help_tests {
    use super::{append_question_text, response_locale, tgpt_query_command};
    use winit::keyboard::{Key, NamedKey};

    #[test]
    fn question_accepts_named_space_between_words() {
        let mut question = String::from("how");
        append_question_text(&mut question, &Key::Named(NamedKey::Space));
        append_question_text(&mut question, &Key::Character("to".into()));
        assert_eq!(question, "how to");
    }

    #[test]
    fn question_is_quoted_as_one_shell_argument() {
        let command = tgpt_query_command("what's `touch /tmp/hafthi-test`?; echo unsafe");
        assert!(command.contains("what'\\''s `touch /tmp/hafthi-test`?; echo unsafe'"));
        assert!(command.contains("--provider pollinations --quiet --whole"));
        assert!(!command.contains(" --shell "));
    }

    #[test]
    fn response_language_follows_linux_locale_precedence() {
        assert_eq!(response_locale(None, None, None, Some("sv_SE.UTF-8")), "sv_SE");
        assert_eq!(response_locale(None, Some("de:en"), Some("sv_SE"), None), "de");
        assert_eq!(response_locale(Some("fr_FR.UTF-8"), Some("de"), None, None), "fr_FR");
        assert_eq!(response_locale(None, None, None, Some("C.UTF-8")), "en");
    }
}

fn run() -> Result<()> {
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
    let mut opacity_dragging = false;
    let mut mouse_pos = winit::dpi::PhysicalPosition::new(0.0, 0.0);
    let mut clipboard = match Clipboard::new() {
        Ok(clipboard) => Some(clipboard),
        Err(err) => {
            diagnostics::record(&format!("clipboard unavailable: {err}"));
            None
        }
    };
    let mut recent_copy: Option<String> = None;
    let mut context_menu = ContextMenu::new();
    let mut preferences = PreferencesPanel::new();
    let mut preferences_backup: Option<Settings> = None;
    let mut font_families: Vec<String> = gpu.font_system.db().faces()
        .filter(|face| face.monospaced)
        .filter_map(|face| face.families.first().map(|(name, _)| name.clone()))
        .collect();
    font_families.push(settings.font_family.clone());
    font_families.sort_by_key(|family| family.to_lowercase());
    font_families.dedup_by(|a, b| a.eq_ignore_ascii_case(b));

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
                diagnostics::record("PTY shell exited; closing window");
                elwt.exit();
            }
            Event::UserEvent(AppEvent::ImageChosen(path)) => {
                if !preferences.visible {
                    return;
                }
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
                WindowEvent::CloseRequested => {
                    diagnostics::record("window close requested");
                    elwt.exit();
                }
                WindowEvent::ModifiersChanged(new_modifiers) => {
                    modifiers = new_modifiers.state();
                }
                WindowEvent::KeyboardInput { event, .. } => {
                    if event.state == ElementState::Pressed {
                        if preferences.visible {
                            if preferences.question_editing {
                                match &event.logical_key {
                                    Key::Named(NamedKey::Escape) => preferences.question_editing = false,
                                    Key::Named(NamedKey::Enter) => {
                                        preferences.question_editing = false;
                                        if settings.command_help_enabled && !preferences.question_input.trim().is_empty() {
                                            let command = tgpt_query_command(preferences.question_input.trim());
                                            let _ = settings.save();
                                            preferences_backup = None;
                                            preferences.close();
                                            terminal.scroll_to_bottom();
                                            pty.write(command.as_bytes());
                                        }
                                    }
                                    Key::Named(NamedKey::Backspace) => { preferences.question_input.pop(); },
                                    Key::Named(NamedKey::Space) | Key::Character(_)
                                        if !modifiers.control_key() && !modifiers.super_key() => {
                                        append_question_text(&mut preferences.question_input, &event.logical_key);
                                    }
                                    _ => {}
                                }
                                dirty = true;
                                window.request_redraw();
                                return;
                            }
                            if preferences.color_editing {
                                match &event.logical_key {
                                    Key::Named(NamedKey::Escape) => {
                                        preferences.color_editing = false;
                                        preferences.color_error = false;
                                    }
                                    Key::Named(NamedKey::Enter) => {
                                        if let Some(color) = settings::parse_hex_color(&preferences.color_input) {
                                            settings.foreground = color;
                                            terminal.set_default_foreground(color);
                                            gpu.apply_settings(settings.clone());
                                            gpu.update_terminal_text(&terminal);
                                            preferences.color_editing = false;
                                            preferences.color_error = false;
                                        } else {
                                            preferences.color_error = true;
                                        }
                                    }
                                    Key::Named(NamedKey::Backspace) => {
                                        preferences.color_input.pop();
                                        preferences.color_error = false;
                                    }
                                    Key::Character(ch) if !modifiers.control_key() => {
                                        for digit in ch.chars().filter(|digit| digit.is_ascii_hexdigit()) {
                                            if preferences.color_input.len() < 6 {
                                                preferences.color_input.push(digit.to_ascii_uppercase());
                                            }
                                        }
                                        preferences.color_error = false;
                                    }
                                    _ => {}
                                }
                                dirty = true;
                                window.request_redraw();
                                return;
                            }
                            if preferences.page == PrefPage::CommandHelp {
                                if event.logical_key == Key::Named(NamedKey::Space) {
                                    settings.command_help_enabled = !settings.command_help_enabled;
                                    gpu.apply_settings(settings.clone());
                                    dirty = true;
                                    window.request_redraw();
                                    return;
                                }
                                if event.logical_key == Key::Named(NamedKey::Enter) {
                                    preferences.question_editing = true;
                                    dirty = true;
                                    window.request_redraw();
                                    return;
                                }
                            }
                            if event.logical_key == Key::Named(NamedKey::Escape) {
                                if let Some(original) = preferences_backup.take() {
                                    settings = original;
                                    terminal.set_default_foreground(settings.foreground);
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
                                Key::Character(ch) if ch.eq_ignore_ascii_case("h") => {
                                    preferences_backup = Some(settings.clone());
                                    preferences.open(gpu.config.width, gpu.config.height, settings.scale_factor);
                                    preferences.page = PrefPage::CommandHelp;
                                    preferences.question_editing = settings.command_help_enabled;
                                    dirty = true;
                                    window.request_redraw();
                                    return;
                                }
                                Key::Character(ch) if ch.eq_ignore_ascii_case("c") => {
                                    copy_selection_to_clipboard(
                                        &terminal, selection, clipboard.as_mut(), &mut recent_copy,
                                    );
                                    return;
                                }
                                Key::Character(ch) if ch.eq_ignore_ascii_case("v") => {
                                    if let Some(text) = clipboard_text(clipboard.as_mut(), &recent_copy) {
                                        terminal.scroll_to_bottom();
                                        pty.write(text.as_bytes());
                                        selection = None;
                                        gpu.update_terminal_text(&terminal);
                                        dirty = true;
                                        window.request_redraw();
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
                        if opacity_dragging {
                            settings.opacity = preferences.opacity_for_x(position.x as f32) as f64 / 100.0;
                            gpu.apply_settings(settings.clone());
                            dirty = true;
                            window.request_redraw();
                        }
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
                    context_menu.set_command_help_enabled(settings.command_help_enabled);
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
                                Some(PrefAction::SelectPage(page)) => {
                                    preferences.page = page;
                                    preferences.hovered = None;
                                    preferences.question_editing = false;
                                }
                                Some(PrefAction::ToggleCommandHelp) => {
                                    settings.command_help_enabled = !settings.command_help_enabled;
                                    gpu.apply_settings(settings.clone());
                                }
                                Some(PrefAction::EditQuestion) => {
                                    preferences.question_editing = true;
                                }
                                Some(PrefAction::AskQuestion) => {
                                    if settings.command_help_enabled && !preferences.question_input.trim().is_empty() {
                                        let command = tgpt_query_command(preferences.question_input.trim());
                                        let _ = settings.save();
                                        preferences_backup = None;
                                        preferences.close();
                                        terminal.scroll_to_bottom();
                                        pty.write(command.as_bytes());
                                    }
                                }
                                Some(PrefAction::InstallTgpt) => {
                                    preferences.close();
                                    if let Some(original) = preferences_backup.take() {
                                        settings = original;
                                        gpu.apply_settings(settings.clone());
                                    }
                                    // Place the installation command at the shell prompt;
                                    // the user decides whether to execute it.
                                    pty.write(b"sudo pacman -S --needed tgpt");
                                }
                                Some(PrefAction::FontFamilyPrev | PrefAction::FontFamilyNext) => {
                                    let index = font_families.iter().position(|family| family == &settings.font_family).unwrap_or(0);
                                    let next = if action == Some(PrefAction::FontFamilyPrev) {
                                        (index + font_families.len() - 1) % font_families.len()
                                    } else { (index + 1) % font_families.len() };
                                    settings.font_family = font_families[next].clone();
                                    gpu.apply_settings(settings.clone());
                                }
                                Some(PrefAction::TextColor(index)) => {
                                    let color = TEXT_SWATCHES[index as usize];
                                    settings.foreground = color;
                                    terminal.set_default_foreground(color);
                                    gpu.apply_settings(settings.clone());
                                    preferences.color_editing = false;
                                    preferences.color_error = false;
                                }
                                Some(PrefAction::EditTextColor) => {
                                    preferences.color_input = format!("{:02X}{:02X}{:02X}",
                                        settings.foreground.r, settings.foreground.g, settings.foreground.b);
                                    preferences.color_editing = true;
                                    preferences.color_error = false;
                                }
                                Some(PrefAction::FontDown) => {
                                    settings.zoom_by(1.0 / 1.05);
                                    gpu.apply_settings(settings.clone());
                                }
                                Some(PrefAction::FontUp) => {
                                    settings.zoom_by(1.05);
                                    gpu.apply_settings(settings.clone());
                                }
                                Some(PrefAction::OpacitySet(value)) => {
                                    opacity_dragging = true;
                                    settings.opacity = value as f64 / 100.0;
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
                                    if preferences.color_editing {
                                        if let Some(color) = settings::parse_hex_color(&preferences.color_input) {
                                            settings.foreground = color;
                                            terminal.set_default_foreground(color);
                                            gpu.apply_settings(settings.clone());
                                        } else {
                                            preferences.color_error = true;
                                            dirty = true;
                                            window.request_redraw();
                                            return;
                                        }
                                    }
                                    let _ = settings.save();
                                    preferences_backup = None;
                                    preferences.close();
                                }
                                Some(PrefAction::Cancel) => {
                                    if let Some(original) = preferences_backup.take() {
                                        settings = original;
                                        terminal.set_default_foreground(settings.foreground);
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
                        } else {
                            opacity_dragging = false;
                        }
                    } else if context_menu.visible {
                        if state == ElementState::Pressed {
                            let action = context_menu
                                .action_at(mouse_pos.x as f32, mouse_pos.y as f32);
                            context_menu.close();

                            match action {
                                Some(MenuAction::Copy) => {
                                    copy_selection_to_clipboard(
                                        &terminal, selection, clipboard.as_mut(), &mut recent_copy,
                                    );
                                }
                                Some(MenuAction::Paste) => {
                                    if let Some(text) = clipboard_text(clipboard.as_mut(), &recent_copy) {
                                        terminal.scroll_to_bottom();
                                        pty.write(text.as_bytes());
                                        selection = None;
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
                                Some(MenuAction::AskTgpt) => {
                                    preferences_backup = Some(settings.clone());
                                    preferences.open(gpu.config.width, gpu.config.height, settings.scale_factor);
                                    preferences.page = PrefPage::CommandHelp;
                                    preferences.question_editing = true;
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
                                    diagnostics::record("Quit chosen from menu");
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
