#!/usr/bin/env bash
# Adds Material Painter to your app menu (no root needed). Run it from the
# unpacked folder; the app stays where it is, so keep the folder.
#   ./install.sh            # add the menu entry
#   ./install.sh --remove   # remove it
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
apps="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
icons="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/512x512/apps"
entry="$apps/material-painter.desktop"

if [[ "${1:-}" == "--remove" ]]; then
    rm -f "$entry" "$icons/material-painter.png"
    echo "Removed the Material Painter menu entry."
    exit 0
fi

mkdir -p "$apps" "$icons"
cp "$here/assets/ui/icon.png" "$icons/material-painter.png"
sed "s|^Exec=.*|Exec=\"$here/material-painter\"|" "$here/material-painter.desktop" > "$entry"
chmod +x "$entry"
command -v update-desktop-database >/dev/null && update-desktop-database "$apps" || true
echo "Material Painter is in your app menu. Run ./install.sh --remove to take it out."
