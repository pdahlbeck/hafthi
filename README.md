![Hafþi terminal — preview of the terminal and preferences](assets/hafthi-banner.jpg)

# Hafþi

**Hafþi is a GPU-accelerated terminal emulator for Linux/Wayland, written in Rust and rendered with wgpu.**

Hafþi grew out of the Footsole project, but the current codebase is a native GPU terminal rather than a GTK/VTE wrapper. The name comes from the Gutasaga and reflects the project's roots in Gotland.

> **Version 0.7.20:** removes unused preferences and theme constants reported during installation. Linux/Wayland is the only supported platform.

### Optional Linux command help

Open **Preferences → Command help** (or press **Ctrl+Shift+H**) and turn the feature on. Click **Install tgpt** to place `sudo pacman -S --needed tgpt` at your shell prompt, then press Enter to install it. When enabled, **Ask tgpt…** also appears in the right-click menu and opens the question field directly. Type a question and press Enter or click **Ask tgpt**. The terminal shows a short `hafthi --ask 'your question'` command and then the answer; the full instructions to tgpt stay inside Hafþi. Answers use the language selected by your Linux locale (`LC_ALL`, `LANGUAGE`, `LC_MESSAGES`, or `LANG`), with English as a fallback. Your question is sent to the online provider; suggested commands are never run automatically. This is an optional service and requires an internet connection. Press Space to toggle the feature when the Command help page is open, or Enter to focus its question field.

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

## Run in Podman (Wayland)

On Pop!_OS 24.04/COSMIC or another Wayland desktop, install Podman and run this from the cloned repository as your normal desktop user:

```bash
sudo apt install podman
cd ~/hafthi
bash run-podman.sh
```

The first launch builds a local image from `Containerfile`; subsequent launches reuse it. After pulling new commits, rebuild it with `bash run-podman.sh --build`. Rust/Cargo are installed only in the build stage of the image, so the host does not need them. An Intel GPU is sufficient if it has working Mesa/Vulkan drivers. This needs a **Wayland** login session, a local rootless Podman installation with `crun`, and access to `/dev/dri`.

The launcher connects only the session's Wayland socket, D-Bus session socket (for the file picker), GPU devices, and installed fonts. Container settings are stored in `~/.local/share/hafthi-podman/config`. Shell commands run **inside the container**, with the container's files and programs, not as commands on the host. Host files are not mounted; a host file selected in the file chooser may therefore be inaccessible to Hafþi. The optional tgpt command help requires tgpt to be installed **inside the image**; it is not bundled.

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
