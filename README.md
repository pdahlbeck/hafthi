# Hafþi

**Hafþi is a GPU-accelerated terminal emulator for Linux/Wayland, written in Rust and rendered with wgpu.**

Hafþi grew out of the Footsole project, but the current codebase is a native GPU terminal rather than a GTK/VTE wrapper. The name comes from the Gutasaga and reflects the project's roots in Gotland.

> **Version 0.7.8:** supports high-DPI windows larger than the conservative 2048-pixel GPU texture limit and logs unsupported window sizes. Linux/Wayland is the only supported platform.

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
- GPU-rendered preferences panel styled to match the context menu
- explicit Off / Banner / Full image display controls
- transparency / opacity control
- terminal padding control
- configurable scrollback
- branding/GIF settings
- asynchronous image/GIF chooser
- HiDPI / Wayland scaling
- native Wayland no-blur through `ext-background-effect-v1`

## Still in development

- split panes
- Open File Manager Here
- Copy Current Path
- URL detection / Open Link / Copy Link
- broader terminal escape-sequence compatibility
- installer/package integration
- keyboard navigation and accessibility for Preferences
- performance profiling and long-session testing

## Install

```bash
git clone https://github.com/pdahlbeck/hafthi.git
cd hafthi
bash install.sh
```

The installer builds Hafþi in release mode and installs the complete desktop integration for the current user:

- binary: `~/.local/bin/hafthi`
- launcher: `~/.local/share/applications/hafthi.desktop`
- icon: `~/.local/share/icons/hicolor/scalable/apps/hafthi.svg`

The application therefore appears as **Hafþi** with its own icon in compatible Linux application launchers.

To update an existing clone and reinstall the newest version:

```bash
cd /path/to/hafthi
git pull
bash install.sh
```

Restart Hafþi after installation. Your settings in `~/.config/hafthi/config.ini` remain in place.

To run directly from the source tree without installing:

```bash
cargo run --release
```

To uninstall the application files:

```bash
bash uninstall.sh
```

User configuration in `~/.config/hafthi/` is deliberately kept when uninstalling.

Hafþi currently targets Linux/Wayland.

## Configuration

Hafþi uses:

```text
~/.config/hafthi/config.ini
```

The GPU renderer currently reads settings including font, opacity, padding, colors, ANSI palette, scrollback and branding settings.

## Transparency on Hyprland

Hafþi supports true alpha transparency and uses the staging Wayland `ext-background-effect-v1` protocol with an empty blur region. On supported Hyprland versions this keeps a transparent terminal background sharp instead of applying compositor blur.

At `opacity=0`, the intended result is a fully transparent terminal background with the wallpaper remaining crisp behind the text.

## License

MIT
