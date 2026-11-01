# Material Painter

![Smart materials painted in Material Painter](assets/ui/welcome.jpg)

<p align="center">
  <a href="https://ko-fi.com/allaeddinerakik"><img src="https://ko-fi.com/img/githubbutton_sm.svg" alt="Support Material Painter on Ko-fi" height="44"></a>
  &nbsp;
  <a href="https://github.com/alla2001/material-painter/releases/latest"><img src="https://img.shields.io/github/v/release/alla2001/material-painter?label=Download&style=for-the-badge&color=3d8eff" alt="Download" height="36"></a>
  &nbsp;
  <a href="https://discord.gg/TqKD5xKp7Y"><img src="https://img.shields.io/badge/Discord-Join-5865F2?style=for-the-badge&logo=discord&logoColor=white" alt="Join the Discord" height="36"></a>
</p>

<p align="center"><b>Material Painter is free and open source. If it saves you time, <a href="https://ko-fi.com/allaeddinerakik">buy me a coffee on Ko-fi</a> to keep it going.</b></p>

A fast, lightweight PBR texture painter for game and film assets: paint
materials onto 3D models with layers, smart materials and masks, bake mesh
maps, build materials in a node graph and write your own node shaders.

![Material Designer: a node graph next to the material on the mesh](docs/images/material-designer.png)

> **Early access.** Material Painter works and is in daily use, but it is
> young: expect rough edges, and please report what you find.

## Features

- **Material Paint**: a layer stack with paint, fill and folder layers, blend
  modes and opacity; brushes with tips, grain, pen pressure, stabilizer,
  symmetry and screen, surface or UV projection; polygon fill by triangle,
  quad, loose part, object or UV island.
- **Smart materials and masks**: dozens of ready-made materials, smart mask
  generators (edge wear, dirt, dust, rain streaks, drips, cavity grime...),
  mask layers with blend modes, and mask filters (levels, blur, warp, noise
  breakup...).
- **Mesh map baking**: ambient occlusion, curvature, cavity, thickness,
  normal and height from a high poly, position, world normal, ID and more,
  with cages and match-by-name.
- **Material Designer**: a node graph of tileable textures that drives fill
  layers and masks, with live previews and a 3D view of the result.
- **Shader Maker**: write custom nodes in WGSL with syntax highlighting, a
  live preview and compiler errors pointing at your lines.
- **Export**: presets for Unreal, Unity (URP, HDRP, Built-in), Godot, glTF,
  Roblox and Blender, or your own channel packing; PNG, TIFF, TGA, JPEG and
  EXR at 8, 16 or 32 bits.
- **Preview shaders**: build the 3D view's look as a node graph (toon,
  clay, unlit, rim light, or your own) and texture with it; shading nodes
  read the painted channels, baked maps and lighting, and custom ones are
  plain WGSL like any node.
- **Path tool**: place points on the mesh or the UV map and lay the brush
  evenly along the curve, for clean stitches, seams and panel lines.
- **Asset library**: about 2,900 free CC0 materials from Poly Haven and
  ambientCG, one click from the shelf, plus a community library of material
  graphs and nodes that users share from inside the app. Downloads are plain
  image files with a `meta.json`, so nothing is locked into Material Painter.
- **Docking workspaces**, path-traced preview, HDRI lighting, and an MCP
  server so AI agents can drive the app (off by default).

| Material Paint | Shader Maker |
| --- | --- |
| ![Painting workspace](docs/images/material-paint.png) | ![Shader Maker](docs/images/shader-maker.png) |

![Ready-made material graphs](docs/images/material-presets.png)

## Download

Ready-to-run builds are on the [releases page](https://github.com/alla2001/material-painter/releases/latest),
all free; donations on [Ko-fi](https://ko-fi.com/allaeddinerakik) fund development.

- **Windows**: the installer, or the zip to unpack and run.
- **Linux**: the tarball. Unpack it, run `./material-painter`, and
  optionally `./install.sh` to add it to your app menu.

**Requirements**: 64-bit Windows 10 or 11, or Linux (x64, glibc 2.35 or newer:
Ubuntu 22.04, Fedora 36, Debian 12 and later), and a GPU with DirectX 12 or
Vulkan support (most cards from 2016 on). 8 GB of video memory is
comfortable for 4K texture sets.

## Building from source

Install [Rust](https://rustup.rs) (stable), then:

```sh
git clone https://github.com/alla2001/material-painter
cd material-painter
cargo run --release
```

`cargo test` runs the unit tests. The GPU tests render offscreen (no window)
and write images to `target/bake_test/`:

```sh
cargo test --bin material_painter -- --ignored --test-threads 1
```

## Community

Questions, renders and shared materials: join the [Discord](https://discord.gg/TqKD5xKp7Y).
Bugs and feature requests are welcome there or as [GitHub issues](https://github.com/alla2001/material-painter/issues).

Want to help build it? See [CONTRIBUTING.md](CONTRIBUTING.md) (AI-assisted contributions are welcome; [AGENTS.md](AGENTS.md) is the guide for coding agents).

If Material Painter is useful to you, a donation keeps development going:

<a href="https://ko-fi.com/allaeddinerakik"><img src="https://ko-fi.com/img/githubbutton_sm.svg" alt="Support Material Painter on Ko-fi" height="44"></a>

## License

Material Painter is free software under the
[GNU General Public License, version 3 or later](LICENSE).

Third-party code is listed with its licenses in
[THIRD_PARTY_NOTICES.html](THIRD_PARTY_NOTICES.html). The bundled HDRIs and
PBR textures are from [Poly Haven](https://polyhaven.com) (CC0), and the
icons are [Phosphor](https://phosphoricons.com) (MIT).
