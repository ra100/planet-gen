#!/bin/sh
# Installs Planet Gen for the current user: the binary into ~/.local/bin and a
# desktop entry plus icon so it shows up in the application menu.
# Pass --uninstall to remove them again.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
bin_dir="${XDG_BIN_HOME:-$HOME/.local/bin}"
data_dir="${XDG_DATA_HOME:-$HOME/.local/share}"

if [ "${1:-}" = "--uninstall" ]; then
    rm -f "$bin_dir/planet-gen" \
        "$data_dir/applications/planet-gen.desktop" \
        "$data_dir/icons/hicolor/512x512/apps/planet-gen.png"
    echo "Planet Gen removed."
    exit 0
fi

mkdir -p "$bin_dir" "$data_dir/applications" "$data_dir/icons/hicolor/512x512/apps"
install -m 755 "$here/planet-gen" "$bin_dir/planet-gen"
install -m 644 "$here/planet-gen.png" "$data_dir/icons/hicolor/512x512/apps/planet-gen.png"
sed "s|^Exec=.*|Exec=$bin_dir/planet-gen|" "$here/planet-gen.desktop" \
    > "$data_dir/applications/planet-gen.desktop"
command -v update-desktop-database >/dev/null 2>&1 \
    && update-desktop-database "$data_dir/applications" >/dev/null 2>&1 || true
echo "Planet Gen installed to $bin_dir/planet-gen"
