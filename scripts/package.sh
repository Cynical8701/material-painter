#!/usr/bin/env bash
# Builds a Linux release and stages it in dist/, then packs a tarball.
#   scripts/package.sh            # build, stage, pack
#   scripts/package.sh --no-build # stage and pack an existing build
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)"
if [[ "${1:-}" != "--no-build" ]]; then
    cargo build --release --locked
fi

name="MaterialPainter-$version-linux-x64"
stage="dist/$name"
rm -rf "$stage"
mkdir -p "$stage"
cp target/release/material_painter "$stage/material-painter"
cp -r assets "$stage/assets"
cp LICENSE THIRD_PARTY_NOTICES.html README.md "$stage/"
cp packaging/linux/material-painter.desktop "$stage/"
cp packaging/linux/install.sh "$stage/"
chmod +x "$stage/material-painter" "$stage/install.sh"

tar -C dist -czf "dist/$name.tar.gz" "$name"
echo "Staged $stage"
echo "Wrote dist/$name.tar.gz"
if [[ -n "${GITHUB_OUTPUT:-}" ]]; then echo "version=$version" >> "$GITHUB_OUTPUT"; fi
