<img width="1271" height="1237" alt="ChatGPT Image Sep 23, 2026, 03_50_34 PM" src="https://github.com/user-attachments/assets/d217d0dc-f9d0-422b-a0ce-887af79d86e7" />


# Hafþi

**Hafþi is a GPU-accelerated terminal emulator for Linux/Wayland, written in Rust and rendered with wgpu.**

Hafþi grew out of the Footsole project, but the current codebase is a native GPU terminal rather than a GTK/VTE wrapper. The name comes from the Gutasaga and reflects the project's roots in Gotland.

> **Version 0.7.28:** Micro joins the optional tools as a text editor with its own Hafþi window. Linux/Wayland is the only supported platform.

### Optional Fish and Starship

Open **Preferences → Extras**, then select **Fish** or **Starship** to see whether it is installed, change its settings, and visit its GitHub project. You can also turn either tool on or off in the Extras overview. On Arch Linux, install either or both yourself:

```bash
sudo pacman -S --needed fish starship
```

On other distributions, use your package manager. With **Use Fish when installed** enabled (the default), Hafþi launches Fish if it finds an executable; otherwise it opens your account's normal login shell. The Fish welcome message is hidden by default inside Hafþi and can be shown with **Show greeting in Hafþi**. This does not change `config.fish` or another terminal.

When Fish and Starship are both installed, Hafþi initializes Starship in Fish by default. If your Fish configuration already initializes Starship, Hafþi does not initialize it again. You can turn off Hafþi's automatic initialization in Preferences; your own `config.fish` still takes effect. Changes to these shell options take effect when you restart Hafþi. Neither Fish nor Starship is bundled with Hafþi. If you use another shell, follow [Starship's setup instructions](https://starship.rs/guide/) for that shell.

### Optional Linux command help

Open **Preferences → Extras → tgpt** (or press **Ctrl+Shift+H**) and turn the feature on. You can also switch it on or off in the Extras overview. Its settings page links to the tgpt GitHub project. Click **Install tgpt** to place `sudo pacman -S --needed tgpt` at your shell prompt, then press Enter to install it. When enabled, **Ask tgpt…** also appears in the right-click menu and opens the question field directly. Type a question and press Enter or click **Ask tgpt**. The terminal shows a short `hafthi --ask 'your question'` command and then the answer; the full instructions to tgpt stay inside Hafþi. Answers use the language selected by your Linux locale (`LC_ALL`, `LANGUAGE`, `LC_MESSAGES`, or `LANG`), with English as a fallback. Your question is sent to the online provider; suggested commands are never run automatically. This is an optional service and requires an internet connection. Press Space to toggle the feature when the tgpt settings page is open, or Enter to focus its question field.

### Optional Sampler and Yazi

Install [Sampler](https://github.com/sqshq/sampler) yourself from the AUR with `paru -S sampler` (or another AUR helper). Install [Yazi](https://github.com/sxyazi/yazi) from the Arch repositories with `sudo pacman -S --needed yazi`. On other distributions, use the appropriate packages. In **Preferences → Extras**, turn each tool on and select **Open Sampler** or **Open Yazi**. Each runs in its own Hafþi window; closing it leaves your shell window open. The install buttons type commands at the shell prompt without running them.

The first Sampler launch creates `~/.config/hafthi/sampler.yml`, a copy of the included example dashboard. **Edit dashboard…** opens it using your desktop's default handler. Your edits persist through upgrades; remove that file if you want the bundled example copied again. The dashboard runs local commands and contacts github.com for a response-time chart. Review the config before running it. Yazi opens in your home directory. For image previews on Hyprland or Niri, install [Überzug++](https://github.com/jstkdng/ueberzugpp) yourself with `sudo pacman -S --needed ueberzugpp`, restart Hafþi, and open Yazi. The Yazi plugin page shows whether it is detected. Yazi selects its Wayland adapter automatically; check with `ya env` in Hafþi if previews do not appear. Überzug++ displays the image through Wayland; Hafþi does not advertise a native terminal image protocol. For other environments, Yazi may use a different supported adapter or Chafa as a text-based fallback.

### Optional Micro

Install [Micro](https://github.com/micro-editor/micro) yourself on Arch with `sudo pacman -S --needed micro wl-clipboard`. The Wayland clipboard package lets Micro share copied text with other applications. Enable Micro in **Preferences → Extras**, then choose **Open Micro** to edit in a separate Hafþi window. The install button only types the command in your shell for you to review. You can also use `micro filename` directly in the terminal; Hafþi does not change your default editor.

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
- optional Fish shell, Fish greeting control and Starship prompt
- optional tgpt command help
- optional Sampler dashboard, Yazi file manager and Micro editor, launched in separate windows
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

The launcher connects only the session's Wayland socket, D-Bus session socket (for the file picker), GPU devices, and installed fonts. Container settings are stored in `~/.local/share/hafthi-podman/config`. Shell commands run **inside the container**, with the container's files and programs, not as commands on the host. Host files are not mounted; a host file selected in the file chooser may therefore be inaccessible to Hafþi. Optional Fish, Starship, tgpt, Sampler, Yazi and Micro must also be installed **inside the image** to work in Podman; they are not bundled.

## Configuration

Hafþi uses:

```text
~/.config/hafthi/config.ini
```

The GPU renderer currently reads settings including font, opacity, padding, colors, ANSI palette, scrollback, branding and shell options. Preferences saves the shell options under `[shell]` as `use_fish`, `show_fish_greeting` and `use_starship`, and the optional tgpt, Sampler, Yazi and Micro switches under `[plugins]`.

## Transparency on Hyprland

Hafþi supports true alpha transparency and uses the staging Wayland `ext-background-effect-v1` protocol with an empty blur region. On supported Hyprland versions this keeps a transparent terminal background sharp instead of applying compositor blur.

At `opacity=0`, the intended result is a fully transparent terminal background with the wallpaper remaining crisp behind the text.

## License

MIT
