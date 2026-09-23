use std::{collections::HashMap, fs, path::PathBuf};

use crate::terminal::Rgb;

#[derive(Clone)]
pub struct Settings {
    pub font_family: String,
    pub font_size: f32,
    pub line_height: f32,
    pub cell_width: f32,
    pub padding: f32,
    pub scale_factor: f32,
    pub gpu_font_scale: f32,
    pub opacity: f64,
    pub window_width: u32,
    pub window_height: u32,
    pub scrollback: usize,
    pub command_help_enabled: bool,
    pub use_fish: bool,
    pub show_fish_greeting: bool,
    pub use_starship: bool,
    pub foreground: Rgb,
    pub background: Rgb,
    pub cursor: Rgb,
    pub selection_background: Rgb,
    pub ansi: [Rgb; 16],
    pub branding_enabled: bool,
    pub branding_image: String,
    pub branding_mode: String,
    pub branding_max_fps: u32,
}

impl Default for Settings {
    fn default() -> Self {
        let font_size = 12.0;
        Self {
            font_family: "JetBrains Mono Nerd Font".into(),
            font_size,
            line_height: font_size * 1.45,
            cell_width: font_size * 0.60,
            padding: 25.0,
            scale_factor: 1.0,
            gpu_font_scale: 1.8,
            opacity: 0.35,
            window_width: 980,
            window_height: 640,
            scrollback: 15_000,
            command_help_enabled: false,
            use_fish: true,
            show_fish_greeting: false,
            use_starship: true,
            foreground: Rgb::new(0, 0, 0),
            background: Rgb::new(246, 244, 241),
            cursor: Rgb::new(0, 0, 0),
            selection_background: Rgb::new(205, 214, 220),
            branding_enabled: true,
            branding_image: "default".into(),
            branding_mode: "full".into(),
            branding_max_fps: 8,
            ansi: [
                Rgb::new(0, 0, 0),
                Rgb::new(166, 41, 53),
                Rgb::new(53, 107, 45),
                Rgb::new(117, 84, 0),
                Rgb::new(36, 76, 145),
                Rgb::new(105, 71, 143),
                Rgb::new(0, 0, 0),
                Rgb::new(155, 155, 155),
                Rgb::new(68, 68, 68),
                Rgb::new(196, 61, 72),
                Rgb::new(77, 131, 63),
                Rgb::new(150, 107, 0),
                Rgb::new(54, 93, 160),
                Rgb::new(130, 87, 163),
                Rgb::new(0, 0, 0),
                Rgb::new(229, 229, 229),
            ],
        }
    }
}

fn parse_hex(value: &str, fallback: Rgb) -> Rgb {
    parse_hex_color(value).unwrap_or(fallback)
}

pub fn parse_hex_color(value: &str) -> Option<Rgb> {
    let s = value.trim().trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let Ok(v) = u32::from_str_radix(s, 16) else {
        return None;
    };
    Some(Rgb::new(
        ((v >> 16) & 0xff) as u8,
        ((v >> 8) & 0xff) as u8,
        (v & 0xff) as u8,
    ))
}

pub fn config_path() -> PathBuf {
    if let Some(dir) = std::env::var_os("XDG_CONFIG_HOME") {
        PathBuf::from(dir).join("hafthi/config.ini")
    } else {
        dirs_home().join(".config/hafthi/config.ini")
    }
}

fn dirs_home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn read_ini() -> HashMap<(String, String), String> {
    let Ok(text) = fs::read_to_string(config_path()) else {
        return HashMap::new();
    };

    let mut section = String::new();
    let mut values = HashMap::new();

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].trim().to_ascii_lowercase();
            continue;
        }

        if let Some((key, value)) = line.split_once('=') {
            values.insert(
                (section.clone(), key.trim().to_ascii_lowercase()),
                value.trim().to_string(),
            );
        }
    }

    values
}

impl Settings {
    pub fn load() -> Self {
        let mut out = Self::default();
        let ini = read_ini();

        let get = |section: &str, key: &str| {
            ini.get(&(section.to_string(), key.to_string())).map(String::as_str)
        };

        if let Some(font) = get("general", "font") {
            let mut parts: Vec<&str> = font.split_whitespace().collect();
            if let Some(last) = parts.last().copied() {
                if let Ok(size) = last.parse::<f32>() {
                    out.font_size = size.max(6.0);
                    parts.pop();
                    if !parts.is_empty() {
                        out.font_family = parts.join(" ");
                    }
                }
            }
        }

        // The value in Hafthi's config follows normal desktop font sizing
        // semantics (points/logical units), while glyphon expects physical px.
        // Physical scaling is applied later after winit reports the display DPI.
        out.line_height = out.font_size * 1.45;
        out.cell_width = out.font_size * 0.60;

        if let Some(v) = get("general", "padding").and_then(|v| v.parse::<f32>().ok()) {
            out.padding = v.max(0.0);
        }
        if let Some(v) = get("general", "opacity").and_then(|v| v.parse::<f64>().ok()) {
            out.opacity = v.clamp(0.0, 1.0);
        }
        if let Some(v) = get("general", "scrollback").and_then(|v| v.parse::<usize>().ok()) {
            out.scrollback = v.max(100);
        }
        let enabled = |value: &str| matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on");
        if let Some(v) = get("command_help", "enabled") { out.command_help_enabled = enabled(v); }
        if let Some(v) = get("shell", "use_fish") { out.use_fish = enabled(v); }
        if let Some(v) = get("shell", "show_fish_greeting") { out.show_fish_greeting = enabled(v); }
        if let Some(v) = get("shell", "use_starship") { out.use_starship = enabled(v); }
        if let Some(v) = get("gpu", "font_scale").and_then(|v| v.parse::<f32>().ok()) {
            out.gpu_font_scale = v.clamp(0.75, 3.0);
        }
        if let Some(v) = get("window", "width").and_then(|v| v.parse::<u32>().ok()) {
            out.window_width = v.max(320);
        }
        if let Some(v) = get("window", "height").and_then(|v| v.parse::<u32>().ok()) {
            out.window_height = v.max(200);
        }

        if let Some(v) = get("branding", "enabled") {
            out.branding_enabled = matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on");
        }
        if let Some(v) = get("branding", "image") {
            out.branding_image = v.to_string();
        }
        if let Some(v) = get("branding", "mode") {
            let mode = v.to_ascii_lowercase();
            if matches!(mode.as_str(), "full" | "banner") {
                out.branding_mode = mode;
            }
        }
        if let Some(v) = get("branding", "max_fps").and_then(|v| v.parse::<u32>().ok()) {
            out.branding_max_fps = v.clamp(1, 30);
        }

        if let Some(v) = get("colors", "foreground") {
            out.foreground = parse_hex(v, out.foreground);
        }
        if let Some(v) = get("colors", "background") {
            out.background = parse_hex(v, out.background);
        }
        if let Some(v) = get("colors", "cursor") {
            out.cursor = parse_hex(v, out.cursor);
        }
        if let Some(v) = get("colors", "selection_background") {
            out.selection_background = parse_hex(v, out.selection_background);
        }

        for i in 0..16 {
            let key = format!("color{i}");
            if let Some(v) = ini.get(&("colors".to_string(), key)) {
                out.ansi[i] = parse_hex(v, out.ansi[i]);
            }
        }

        out
    }

    pub fn apply_scale_factor(&mut self, scale_factor: f64) {
        let scale = scale_factor.max(1.0) as f32;
        self.scale_factor = scale;

        // 1 pt = 96/72 CSS px at 100% desktop scale.
        let font_px = self.font_size * (96.0 / 72.0) * scale * self.gpu_font_scale;
        self.font_size = font_px;
        self.line_height = font_px * 1.45;
        self.cell_width = font_px * 0.60;
        self.padding *= scale;
    }

    pub fn zoom_by(&mut self, factor: f32) {
        let factor = factor.clamp(0.5, 2.0);
        self.font_size = (self.font_size * factor).clamp(8.0, 160.0);
        self.line_height = self.font_size * 1.45;
        self.cell_width = self.font_size * 0.60;
    }

    pub fn logical_font_size(&self) -> f32 {
        let denom = (96.0 / 72.0) * self.scale_factor.max(1.0) * self.gpu_font_scale.max(0.01);
        self.font_size / denom
    }

    pub fn logical_padding(&self) -> f32 {
        self.padding / self.scale_factor.max(1.0)
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = config_path();
        let mut text = fs::read_to_string(&path).unwrap_or_default();

        fn set_key(text: &mut String, section: &str, key: &str, value: &str) {
            let header = format!("[{section}]");
            let mut lines: Vec<String> = text.lines().map(ToOwned::to_owned).collect();

            let section_start = lines.iter().position(|line| line.trim().eq_ignore_ascii_case(&header));

            if let Some(start) = section_start {
                let mut insert_at = lines.len();
                for i in start + 1..lines.len() {
                    let trimmed = lines[i].trim();
                    if trimmed.starts_with('[') && trimmed.ends_with(']') {
                        insert_at = i;
                        break;
                    }

                    if let Some((existing_key, _)) = trimmed.split_once('=') {
                        if existing_key.trim().eq_ignore_ascii_case(key) {
                            lines[i] = format!("{key}={value}");
                            *text = lines.join("\n") + "\n";
                            return;
                        }
                    }
                }

                lines.insert(insert_at, format!("{key}={value}"));
            } else {
                if !lines.is_empty() && !lines.last().is_some_and(|line| line.is_empty()) {
                    lines.push(String::new());
                }
                lines.push(header);
                lines.push(format!("{key}={value}"));
            }

            *text = lines.join("\n") + "\n";
        }

        let font = format!("{} {:.1}", self.font_family, self.logical_font_size());
        set_key(&mut text, "general", "font", &font);
        set_key(&mut text, "general", "opacity", &format!("{:.2}", self.opacity));
        set_key(&mut text, "general", "padding", &format!("{:.0}", self.logical_padding()));
        set_key(&mut text, "general", "scrollback", &self.scrollback.to_string());
        set_key(&mut text, "command_help", "enabled",
            if self.command_help_enabled { "true" } else { "false" });
        set_key(&mut text, "shell", "use_fish", if self.use_fish { "true" } else { "false" });
        set_key(&mut text, "shell", "show_fish_greeting",
            if self.show_fish_greeting { "true" } else { "false" });
        set_key(&mut text, "shell", "use_starship", if self.use_starship { "true" } else { "false" });
        set_key(&mut text, "colors", "foreground", &format!(
            "#{:02x}{:02x}{:02x}", self.foreground.r, self.foreground.g, self.foreground.b
        ));

        set_key(
            &mut text,
            "branding",
            "enabled",
            if self.branding_enabled { "true" } else { "false" },
        );
        set_key(&mut text, "branding", "image", &self.branding_image);
        set_key(&mut text, "branding", "mode", &self.branding_mode);
        set_key(
            &mut text,
            "branding",
            "max_fps",
            &self.branding_max_fps.to_string(),
        );

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, text)
    }

    pub fn rgba(color: Rgb, alpha: f64) -> wgpu::Color {
        wgpu::Color {
            r: color.r as f64 / 255.0,
            g: color.g as f64 / 255.0,
            b: color.b as f64 / 255.0,
            a: alpha,
        }
    }

    pub fn rgba_premultiplied(color: Rgb, alpha: f64) -> wgpu::Color {
        let a = alpha.clamp(0.0, 1.0);
        wgpu::Color {
            r: (color.r as f64 / 255.0) * a,
            g: (color.g as f64 / 255.0) * a,
            b: (color.b as f64 / 255.0) * a,
            a,
        }
    }

    pub fn rgba_f32(color: Rgb, alpha: f32) -> [f32; 4] {
        [
            color.r as f32 / 255.0,
            color.g as f32 / 255.0,
            color.b as f32 / 255.0,
            alpha,
        ]
    }
}
