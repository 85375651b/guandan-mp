#!/bin/sh
# Install a compiled game, its launcher, and its Wayland/X11 icons for this user.
set -eu

project_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
game_binary=${1:-"$project_dir/target/release/guandan"}
data_dir=${XDG_DATA_HOME:-"$HOME/.local/share"}
bin_dir="$HOME/.local/bin"

if [ "$#" -gt 1 ] || [ ! -f "$game_binary" ] || [ ! -x "$game_binary" ]; then
    echo "Usage: $0 [path-to-compiled-guandan]" >&2
    echo "Build first: cargo build --release --locked --bin guandan" >&2
    exit 1
fi

install -Dm755 "$game_binary" "$bin_dir/guandan"
install -Dm644 "$project_dir/assets/icons/guandan.svg" \
    "$data_dir/icons/hicolor/scalable/apps/guandan.svg"
install -Dm644 "$project_dir/assets/icons/guandan.png" \
    "$data_dir/icons/hicolor/256x256/apps/guandan.png"
install -Dm644 "$project_dir/packaging/guandan.desktop" \
    "$data_dir/applications/guandan.desktop"

# Exec follows desktop-entry quoting, not shell quoting. Use an absolute path so
# launching from a desktop session does not depend on ~/.local/bin being on PATH.
escaped_binary=$(printf '%s' "$bin_dir/guandan" | sed 's/\\/\\\\\\\\/g; s/"/\\\\"/g; s/`/\\\\`/g; s/\$/\\\\$/g; s/%/%%/g')
{
    while IFS= read -r line; do
        case "$line" in
            Exec=*) printf 'Exec="%s"\n' "$escaped_binary" ;;
            *) printf '%s\n' "$line" ;;
        esac
    done < "$project_dir/packaging/guandan.desktop"
} > "$data_dir/applications/guandan.desktop"

# Refresh caches where available; desktops without these tools use the files.
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$data_dir/applications" || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache --force --ignore-theme-index "$data_dir/icons/hicolor" >/dev/null 2>&1 || true
fi
if command -v kbuildsycoca6 >/dev/null 2>&1; then
    kbuildsycoca6 --noincremental >/dev/null 2>&1 || true
fi
printf 'Installed Guandan and its navy spade icon. Relaunch the game to use it.\n'
