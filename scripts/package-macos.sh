#!/usr/bin/env bash
# Builds a universal (Apple Silicon + Intel) macOS app and packs it in a .dmg.
# Runs on macOS (needs lipo, sips, iconutil, codesign and hdiutil).
#   scripts/package-macos.sh
# The app is ad-hoc signed (it runs on Apple Silicon) but not notarized:
# on first launch users right-click it and choose Open.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)"
export MACOSX_DEPLOYMENT_TARGET=11.0
for target in aarch64-apple-darwin x86_64-apple-darwin; do
    cargo build --release --locked --target "$target"
done

name="Material Painter"
stage="dist/macos"
app="$stage/$name.app"
rm -rf "$stage"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"

lipo -create -output "$app/Contents/MacOS/MaterialPainter" \
    target/aarch64-apple-darwin/release/material_painter \
    target/x86_64-apple-darwin/release/material_painter
cp -R assets "$app/Contents/Resources/assets"
cp LICENSE THIRD_PARTY_NOTICES.html README.md "$app/Contents/Resources/"

# Icon: every size macOS asks for, from the 512 px app icon.
iconset="$stage/AppIcon.iconset"
mkdir -p "$iconset"
for size in 16 32 128 256 512; do
    sips -z "$size" "$size" assets/ui/icon.png --out "$iconset/icon_${size}x${size}.png" >/dev/null
    double=$((size * 2))
    if [[ $double -le 512 ]]; then
        sips -z "$double" "$double" assets/ui/icon.png --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
    fi
done
cp assets/ui/icon.png "$iconset/icon_512x512@2x.png"
iconutil -c icns "$iconset" -o "$app/Contents/Resources/AppIcon.icns"
rm -rf "$iconset"

cat > "$app/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>$name</string>
    <key>CFBundleDisplayName</key><string>$name</string>
    <key>CFBundleIdentifier</key><string>com.alla2001.materialpainter</string>
    <key>CFBundleExecutable</key><string>MaterialPainter</string>
    <key>CFBundleIconFile</key><string>AppIcon</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$version</string>
    <key>CFBundleVersion</key><string>$version</string>
    <key>LSMinimumSystemVersion</key><string>11.0</string>
    <key>LSApplicationCategoryType</key><string>public.app-category.graphics-design</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSHumanReadableCopyright</key><string>GPL-3.0-or-later</string>
</dict>
</plist>
EOF

# Ad-hoc signature: Apple Silicon refuses to run unsigned code.
codesign --force --deep --sign - "$app"
codesign --verify --deep --strict "$app"

# A disk image with the app and a shortcut to Applications to drag it onto.
ln -s /Applications "$stage/Applications"
dmg="dist/MaterialPainter-$version-macos-universal.dmg"
rm -f "$dmg"
hdiutil create -volname "$name" -srcfolder "$stage" -ov -format UDZO "$dmg"
echo "Wrote $dmg"
if [[ -n "${GITHUB_OUTPUT:-}" ]]; then echo "version=$version" >> "$GITHUB_OUTPUT"; fi
