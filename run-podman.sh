#!/usr/bin/env bash
set -euo pipefail

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
image_name="hafthi:local"

if ! command -v podman >/dev/null 2>&1; then
    echo "Podman is required: sudo apt install podman" >&2
    exit 1
fi
if [[ "$(id -u)" -eq 0 ]]; then
    echo "Run this script as your regular desktop user, not with sudo." >&2
    exit 1
fi
if [[ "${XDG_SESSION_TYPE:-}" != "wayland" ]]; then
    echo "A Wayland desktop session is required." >&2
    exit 1
fi
if [[ -z "${XDG_RUNTIME_DIR:-}" || -z "${WAYLAND_DISPLAY:-}" ]]; then
    echo "XDG_RUNTIME_DIR and WAYLAND_DISPLAY must be set by the desktop session." >&2
    exit 1
fi

if [[ "$WAYLAND_DISPLAY" == /* ]]; then
    wayland_socket="$WAYLAND_DISPLAY"
else
    wayland_socket="$XDG_RUNTIME_DIR/$WAYLAND_DISPLAY"
fi
if [[ ! -S "$wayland_socket" ]]; then
    echo "Wayland socket not found: $wayland_socket" >&2
    exit 1
fi
if [[ ! -d /dev/dri ]]; then
    echo "No /dev/dri device found; GPU rendering is required." >&2
    exit 1
fi

if [[ "${1:-}" == "--build" ]] || ! podman image exists "$image_name"; then
    podman build --tag "$image_name" --file "$root_dir/Containerfile" "$root_dir"
fi

config_dir="${XDG_DATA_HOME:-$HOME/.local/share}/hafthi-podman/config"
mkdir -p "$config_dir"

args=(
    --rm
    --userns=keep-id:uid=1000,gid=1000
    --group-add=keep-groups
    --device=/dev/dri
    --env XDG_RUNTIME_DIR=/tmp
    --env WAYLAND_DISPLAY=hafthi-wayland
    --env "LANG=${LANG:-C.UTF-8}"
    --mount "type=bind,src=$wayland_socket,dst=/tmp/hafthi-wayland"
    --mount "type=bind,src=$config_dir,dst=/home/hafthi/.config/hafthi"
)

for locale_var in LC_ALL LC_MESSAGES LANGUAGE; do
    if [[ -n "${!locale_var:-}" ]]; then
        args+=(--env "$locale_var=${!locale_var}")
    fi
done

# The desktop portal provides the native file chooser used by Preferences.
if [[ -S "$XDG_RUNTIME_DIR/bus" ]]; then
    args+=(
        --env DBUS_SESSION_BUS_ADDRESS=unix:path=/tmp/hafthi-bus
        --mount "type=bind,src=$XDG_RUNTIME_DIR/bus,dst=/tmp/hafthi-bus"
    )
fi

# Use the desktop's fonts, including any Nerd Fonts installed by the user.
if [[ -d /usr/share/fonts ]]; then
    args+=(--mount type=bind,src=/usr/share/fonts,dst=/usr/share/fonts,readonly)
fi
if [[ -d "$HOME/.local/share/fonts" ]]; then
    args+=(--mount "type=bind,src=$HOME/.local/share/fonts,dst=/home/hafthi/.local/share/fonts,readonly")
fi
if [[ -d "$HOME/.fonts" ]]; then
    args+=(--mount "type=bind,src=$HOME/.fonts,dst=/home/hafthi/.fonts,readonly")
fi

exec podman run "${args[@]}" "$image_name"
