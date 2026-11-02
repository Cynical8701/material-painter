//! Opening a Substance 3D Painter project (`.spp`).
//!
//! The file is read with the reader adapted from SubCraft (`crate::spp`, MIT licensed, credited
//! there). What comes across:
//! * the model, every mesh of it, with its UVs and normals;
//! * the texture set's size and the final look of its base color, roughness, metallic and
//!   height channels: Painter's flattened result as of its last save, as one paint layer;
//! * the layer stack's structure, rebuilt as far as Material Painter has equivalents (see
//!   `app::spp_open`), next to that flattened layer.
//!
//! What does not, and is listed in the import report: procedural (Substance `.sbsar`)
//! materials, filters and generators (Material Painter has no Substance engine, and never
//! reads the contents of those archives: they are Adobe's), brush strokes, Painter's other
//! channels (normal details, emissive, opacity, ambient occlusion), its baked mesh maps (Material
//! Painter bakes its own) and any texture set past the first.

use crate::mesh::Mesh;
use crate::spp::{CacheFormat, LayerInfo, SppFile, channel_bits};
use glam::Vec2;
use std::path::Path;

/// Largest texture set an import makes.
pub const MAX_RESOLUTION: u32 = 4096;

/// A Painter channel as one of Material Painter's paint-layer channels: its size and texels in
/// `engine::PAINT_FORMATS[channel]` layout.
pub type ChannelData = (u32, Vec<u8>);

/// What an import made of a project.
pub struct Imported {
    pub mesh: Mesh,
    pub set_name: String,
    pub resolution: u32,
    /// Base color, roughness, metallic, height (Material Painter's channel order).
    pub channels: [Option<ChannelData>; 4],
    /// The texture set's layer stack, bottom to top, for rebuilding.
    pub layers: Vec<LayerInfo>,
    /// What came across, for the report.
    pub came: Vec<String>,
    /// What didn't, and why.
    pub left: Vec<String>,
}

/// Painter channel flag for each of Material Painter's channels.
const CHANNELS: [(u64, &str); 4] =
    [(channel_bits::BASE_COLOR, "Base color"), (channel_bits::ROUGHNESS, "Roughness"), (channel_bits::METALLIC, "Metallic"), (channel_bits::HEIGHT, "Height")];

fn channel_name(bits: u64) -> String {
    match bits {
        channel_bits::NORMAL => "Normal (painted normal details)".into(),
        channel_bits::EMISSIVE => "Emissive".into(),
        channel_bits::OPACITY => "Opacity".into(),
        channel_bits::AMBIENT_OCCLUSION => "Ambient occlusion".into(),
        0x4 => "Specular edge color".into(),
        0x20 => "Displacement".into(),
        0x40 => "Glossiness".into(),
        b => format!("channel {b:#x}"),
    }
}

/// Reads a project. Runs on a worker thread: decoding a 4K project takes a few seconds.
pub fn import(path: &Path) -> Result<Imported, String> {
    let err = |e: crate::spp::SppError| format!("Can't read {}: {e}", path.display());
    let mut file = SppFile::open(path).map_err(err)?;
    let project = file.project().map_err(err)?;
    let Some(set) = project.texture_sets.first().cloned() else {
        return Err("The project has no texture set.".into());
    };
    let mut came = Vec::new();
    let mut left = Vec::new();

    // The model: every mesh, as one.
    let meshes = file.meshes().map_err(err)?;
    if meshes.is_empty() {
        return Err("The project has no model.".into());
    }
    let (mut positions, mut normals, mut uvs, mut indices) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let (mut objects, mut tri_object) = (Vec::new(), Vec::new());
    for (k, m) in meshes.iter().enumerate() {
        let base = positions.len() as u32;
        positions.extend(&m.positions);
        normals.extend((0..m.positions.len()).map(|i| m.normals.get(i).copied().unwrap_or(glam::Vec3::Y)));
        // Painter's v points up; Material Painter's texture rows start at the top (as its OBJ
        // and FBX import).
        uvs.extend((0..m.positions.len()).map(|i| m.uvs.get(i).map_or(Vec2::ZERO, |uv| Vec2::new(uv.x, 1.0 - uv.y))));
        for t in &m.triangles {
            indices.extend(t.iter().map(|i| i + base));
            tri_object.push(k as u32);
        }
        objects.push(m.name.clone());
    }
    let tris = indices.len() / 3;
    let name = path.file_stem().map_or("Painter project".into(), |s| s.to_string_lossy().into_owned());
    let mut mesh = Mesh::from_parts(name, positions, Some(normals), uvs, indices);
    if mesh.indices.len() / 3 == tri_object.len() {
        mesh.tri_object = tri_object;
        mesh.objects = objects;
    }
    came.push(format!("The model: {} mesh{} ({tris} triangles), with its UVs and normals.", meshes.len(), if meshes.len() == 1 { "" } else { "es" }));

    let size = 1u32 << set.width_log2.clamp(7, 13);
    let resolution = size.min(MAX_RESOLUTION);
    came.push(format!(
        "Texture set \"{}\" at {resolution}²{}.",
        set.name,
        if size > MAX_RESOLUTION { format!(" (Painter's {size}² is past Material Painter's largest)") } else { String::new() }
    ));
    if project.texture_sets.len() > 1 {
        let others: Vec<&str> = project.texture_sets[1..].iter().map(|s| s.name.as_str()).collect();
        left.push(format!(
            "Texture sets after the first ({}): Material Painter works on one texture set; open each part's mesh separately to paint it.",
            others.join(", ")
        ));
    }

    // The flattened channels.
    let mut channels: [Option<ChannelData>; 4] = Default::default();
    let mut got = Vec::new();
    for (k, (bits, label)) in CHANNELS.iter().enumerate() {
        match file.channel_texture(&set, *bits, MAX_RESOLUTION) {
            Ok(Some(img)) => {
                let p = &img.plane;
                if p.width != p.height {
                    left.push(format!("{label}: not square ({}x{}), skipped.", p.width, p.height));
                    continue;
                }
                let data = match (k, img.format) {
                    (0, CacheFormat::Srgb8Alpha8) => to_u8(&p.data),
                    (1 | 2, CacheFormat::Rg8) => to_u8(&p.data),
                    (3, CacheFormat::Rg16f) => p.data.iter().flat_map(|v| crate::gpu::f32_to_f16(*v).to_le_bytes()).collect(),
                    (_, f) => {
                        left.push(format!("{label}: stored as {f:?}, which this import doesn't read."));
                        continue;
                    }
                };
                channels[k] = Some((p.width, data));
                got.push(*label);
            }
            Ok(None) => {
                // Painter only stores channels it has computed.
                if set.channels.iter().any(|c| c.bits == *bits) {
                    left.push(format!("{label}: Painter hadn't stored its result (open and save the project in Painter to refresh it)."));
                }
            }
            Err(e) => left.push(format!("{label}: couldn't be read ({e}).")),
        }
    }
    if got.is_empty() {
        left.push("No painted result was stored in the project: the model comes in unpainted.".into());
    } else {
        came.push(format!("The final look of {}: Painter's result as of its last save, as one paint layer.", got.join(", ").to_lowercase()));
    }
    let other: Vec<String> = set.channels.iter().filter(|c| !CHANNELS.iter().any(|(b, _)| *b == c.bits)).map(|c| channel_name(c.bits)).collect();
    if !other.is_empty() {
        left.push(format!("Painter channels Material Painter doesn't have: {}.", other.join(", ")));
    }
    left.push("Painter's baked mesh maps: Material Painter bakes its own from the model. If Painter baked from a high poly, load it in the bake window.".into());

    // What the layer stack holds, for the report.
    let mut procedural = 0;
    let mut strokes = 0;
    crate::spp::document::walk_layers(
        &set.layers,
        &mut |l, _| {
            procedural += l.sources.iter().filter(|s| matches!(s, crate::spp::SourceInfo::Procedural { .. })).count();
            strokes += l.strokes;
        },
        0,
    );
    if procedural > 0 {
        left.push(format!(
            "{procedural} procedural source{} (Substance materials, generators): Material Painter can't run them; their result is in the flattened layer.",
            if procedural == 1 { "" } else { "s" }
        ));
    }
    if strokes > 0 {
        left.push(format!("{strokes} brush stroke{}: kept in the flattened layer, not as editable strokes.", if strokes == 1 { "" } else { "s" }));
    }

    Ok(Imported { mesh, set_name: set.name.clone(), resolution, channels, layers: set.layers.clone(), came, left })
}

fn to_u8(v: &[f32]) -> Vec<u8> {
    v.iter().map(|x| (x.clamp(0.0, 1.0) * 255.0).round() as u8).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SubCraft's synthetic project (no real Painter file): the model, the stack's layers and
    /// a report that says what was left out.
    #[test]
    fn imports_synthetic_project() {
        let path = std::env::temp_dir().join(format!("mp_spp_fixture_{}.spp", std::process::id()));
        std::fs::write(&path, crate::spp::fixture::project_file()).unwrap();
        let imported = import(&path).expect("imports");
        let _ = std::fs::remove_file(&path);
        assert_eq!(imported.mesh.indices.len() / 3, 2, "the quad");
        assert_eq!(imported.layers.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(), ["Rust", "Dirt"]);
        assert!(imported.came.iter().any(|l| l.starts_with("The model")));
        // Its one cached channel (roughness) is 4x2: not square, reported and skipped.
        assert!(imported.channels.iter().all(Option::is_none));
        assert!(imported.left.iter().any(|l| l.contains("not square")), "{:?}", imported.left);
        assert!(imported.left.iter().any(|l| l.contains("baked mesh maps")));
    }

    /// Painter's version-0 `document.bin` (self-describing fields), as other Painter versions
    /// write it, reads into the same project model.
    #[test]
    fn reads_version_0_document() {
        let mut d = Vec::new();
        d.extend(0x1b7c_2fddu32.to_le_bytes());
        d.extend(0u32.to_le_bytes());
        d.extend(81u32.to_le_bytes());
        let name = |d: &mut Vec<u8>, s: &str| {
            d.extend((s.len() as u32).to_le_bytes());
            d.extend(s.as_bytes());
        };
        let object = |d: &mut Vec<u8>, class: &str, fields: u16| {
            d.extend([0u8]);
            d.extend(0u32.to_le_bytes());
            name(d, class);
            d.extend(fields.to_le_bytes());
        };
        d.push(18);
        object(&mut d, "DataDocument", 1);
        name(&mut d, "materials");
        d.push(19);
        d.extend(0u32.to_le_bytes());
        d.extend(1u32.to_le_bytes());
        d.push(18);
        object(&mut d, "DataMaterial", 4);
        name(&mut d, "resolutionLog2");
        d.push(6);
        d.extend(11i32.to_le_bytes());
        d.extend(11i32.to_le_bytes());
        name(&mut d, "sceneMaterialName");
        d.push(16);
        name(&mut d, "Body");
        name(&mut d, "colorManagementWorkflow");
        d.push(9);
        d.extend(0u64.to_le_bytes());
        // A texture set needs a stack (here an empty one).
        name(&mut d, "stacks");
        d.push(19);
        d.extend(0u32.to_le_bytes());
        d.extend(1u32.to_le_bytes());
        d.push(18);
        object(&mut d, "DataMaterialStack", 0);
        let root = crate::spp::document::parse(&d).expect("version 0 parses");
        let info = crate::spp::document::project_info(&root);
        assert_eq!(info.texture_sets.len(), 1);
        assert_eq!(info.texture_sets[0].name, "Body");
        assert_eq!(info.texture_sets[0].width_log2, 11);
    }
}
