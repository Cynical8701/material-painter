# Opening Substance 3D Painter projects

**File → Open Substance 3D Painter project (.spp)…** starts a new project from a `.spp` file, so
you can move work over from Painter. When it's done, a report lists exactly what came across and
what didn't.

## What comes across

- **The model**: every mesh in the project, with its UVs and normals.
- **The texture set's size** (up to 4096²).
- **The final look** of base color, roughness, metallic and height: Painter's own result as of
  its last save, as one paint layer, *Painter result (flattened)*. This is the project exactly as
  it looked in Painter.
- **The layer stack's structure**, rebuilt in a hidden folder, *Painter layers (rebuilt)*:
  - folders and layers with their names, visibility and opacity;
  - fills of flat values (color, roughness, metallic, height), exactly;
  - masks: their starting value, Levels, and Painter's dirt, edge wear, curvature, ambient
    occlusion, position and noise masks as the closest Material Painter smart masks
    (approximately);
  - blend modes Normal, Multiply and Subtract; Overlay-like modes approximately.

To edit the structure instead of the flattened result, show *Painter layers (rebuilt)* and hide
*Painter result (flattened)*.

## What doesn't (the limits)

- **Procedural materials, generators and filters** (Substance `.sbsar` graphs): Material Painter
  has no Substance engine, so it can't run them. Their result is in the flattened layer; in the
  rebuilt stack those layers are hidden, empty placeholders with their names.
- **Brush strokes**: kept in the flattened layer, not as strokes you can edit. Paint layers come
  in as named placeholders.
- **Image fills**: not rebuilt (the image is in the flattened result).
- **Painted mask strokes**: not rebuilt.
- **Channels Material Painter doesn't have**: Painter's normal channel (painted normal details),
  emissive, opacity, ambient occlusion and others.
- **Baked mesh maps**: Material Painter bakes its own from the model when the project opens. If
  Painter baked from a high poly, load the same high poly in the bake window.
- **More than one texture set**: only the first comes in.
- **Texture sets larger than 4096²** come in at 4096².
- A channel Painter never computed before saving isn't stored in the file; open and save the
  project in Painter to refresh it.

## What is read, and what isn't

Only your own project's data is read: the model, the layer settings and the final textures you
made. The contents of Substance (`.sbsar`) materials are Adobe's assets; Material Painter never
opens them. It doesn't use any Adobe software or library to read the file.

Material Painter is not affiliated with or endorsed by Adobe. Substance 3D Painter is a trademark
of Adobe.

## Credits

Reading `.spp` files is adapted from [SubCraft](https://github.com/Aalzard/subcraft) by Aalzard
and the SubCraft contributors (MIT License): its HDF5, model, document, texture-cache and
JPEG 2000 readers and its synthetic test project, in [`src/spp`](../src/spp) with the license
text. Material Painter adds the reading of Painter's older (version 0) document format and its
own import into Material Painter's layers.
