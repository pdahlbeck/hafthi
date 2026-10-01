#!/usr/bin/env bash
# Runs inside a dedicated PTY. Keep package-manager prompts interactive.
set -euo pipefail

build_dir=''
finish() {
    local status=$?
    trap - EXIT
    if [[ -n "$build_dir" ]]; then rm -rf -- "$build_dir"; fi
    if (( status == 0 )); then
        printf '\nYay is ready. You can close this window.\n'
    else
        printf '\nYay installation stopped (exit %s). See the messages above.\n' "$status"
    fi
    read -r -p 'Press Enter to close… ' _ || true
    exit "$status"
}
trap finish EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

if [[ -r /etc/os-release ]]; then
    . /etc/os-release
else
    . /usr/lib/os-release
fi
case " ${ID:-} ${ID_LIKE:-} " in
    *' arch '*|*' archlinux '*) ;;
    *) printf 'Yay installation is supported only on Arch-based systems.\n'; exit 1 ;;
esac
if (( $(id -u) == 0 )); then
    printf 'Open Hafþi as your normal user. makepkg must not run as root.\n'
    exit 1
fi
if command -v yay >/dev/null 2>&1; then
    yay --version
    exit 0
fi
command -v pacman >/dev/null
command -v sudo >/dev/null
printf 'Installing Yay. Package confirmations and your sudo password may be required.\n'

# Prefer the distribution's own package when one is available.
if pacman -Si yay >/dev/null 2>&1; then
    sudo pacman -S --needed git base-devel yay
else
    sudo pacman -S --needed git base-devel
    build_dir=$(mktemp -d -t hafthi-yay.XXXXXXXX)
    git clone -- https://aur.archlinux.org/yay.git "$build_dir/yay"
    cd "$build_dir/yay"
    makepkg -si
fi
yay --version
