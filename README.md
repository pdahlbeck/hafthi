# Hafthi

**Hafthi is a GPU-accelerated terminal emulator for Linux/Wayland, written in Rust and rendered with wgpu.**

Hafthi grew out of the Footsole project, but the current codebase is a native GPU terminal rather than a GTK/VTE wrapper. The name comes from the Gutasaga and reflects the project's roots in Gotland.

> **Status:** active development. Linux/Wayland is the only supported platform for now.

## Current stack

- Rust
- winit
- wgpu / Vulkan
- glyphon GPU text rendering
- portable-pty
- vte parser
- native Wayland transparency
- `ext-background-effect-v1` for sharp transparency without compositor blur on Hyprland

## Implemented

- GPU-rendered terminal text
- real PTY shell operation
- ANSI 16-color, 256-color and truecolor support
- bold text
- GPU cursor and selection
- mouse selection
- Copy / Paste
- scrollback
- right-click context menu
- live font zoom
- Preferences panel
- transparency / opacity control
- terminal padding control
- configurable scrollback
- branding/GIF settings
- asynchronous image/GIF chooser
- HiDPI / Wayland scaling
- native Wayland no-blur through `ext-background-effect-v1`

## Still in development

- actual GPU rendering of selected PNG/GIF branding
- split panes
- Open File Manager Here
- Copy Current Path
- URL detection / Open Link / Copy Link
- broader terminal escape-sequence compatibility
- installer/package integration
- Preferences UI polish
- performance profiling and long-session testing

## Build and run

```bash
git clone https://github.com/pdahlbeck/hafthi.git
cd hafthi
cargo run --release
```

Hafthi currently targets Linux/Wayland.

## Configuration

Hafthi uses:

```text
~/.config/hafthi/config.ini
```

The GPU renderer currently reads settings including font, opacity, padding, colors, ANSI palette, scrollback and branding settings.

## Transparency on Hyprland

Hafthi supports true alpha transparency and uses the staging Wayland `ext-background-effect-v1` protocol with an empty blur region. On supported Hyprland versions this keeps a transparent terminal background sharp instead of applying compositor blur.

At `opacity=0`, the intended result is a fully transparent terminal background with the wallpaper remaining crisp behind the text.

## License

MIT
