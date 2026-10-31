//! Project files (.mpp): a JSON manifest plus LZ4-compressed binary blobs
//! (the mesh, painted channels, masks and bake fixes). Baked mesh maps aren't
//! stored; opening a project re-bakes them with the saved settings, which
//! keeps files small.
//!
//! Layout: magic `MPPROJ\0` + format version byte, u64 manifest length,
//! manifest (UTF-8 JSON), u64 blob count, then per blob a u64 length and the
//! compressed bytes. Saving writes a temporary file and renames it over the
//! target, so a failed save never damages an existing project.

use super::PainterApp;
use crate::bake::{IdSource, Quality};
use crate::engine::{Blend, Content, FillParams, Folder, GenKind, Generator, Layer, Levels, Mask, MatValues, Projection};
use crate::environment::EnvSource;
use crate::gpu::Tex;
use crate::mesh::{Mesh, Vertex};
use crate::post::Tonemap;
use crate::viewport::{Background, OrbitCam};
use eframe::{egui, wgpu};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const MAGIC: &[u8; 8] = b"MPPROJ\x00\x01";

/// Per-user app data: recent projects and the autosave.
pub(super) fn data_dir() -> Option<PathBuf> {
    // Tests never touch the user's settings, recovery files or nodes.
    if cfg!(test) {
        return Some(std::env::temp_dir().join("MaterialPainterTest"));
    }
    let base = std::env::var_os("APPDATA")
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME"))
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("MaterialPainter"))
}

/// This instance's recovery copy of unsaved work, written every few edits.
/// Each running instance has its own, guarded by a lock file it holds open
/// for its lifetime; an autosave whose lock is free belongs to a session that
/// ended without closing cleanly.
pub(super) fn autosave_path() -> Option<PathBuf> {
    static LOCK: std::sync::OnceLock<Option<std::fs::File>> = std::sync::OnceLock::new();
    let dir = data_dir()?;
    let pid = std::process::id();
    LOCK.get_or_init(|| {
        std::fs::create_dir_all(&dir).ok()?;
        let file = std::fs::File::create(dir.join(format!("autosave-{pid}.lock"))).ok()?;
        file.try_lock().ok()?;
        Some(file)
    })
    .as_ref()?;
    Some(dir.join(format!("autosave-{pid}.mpp")))
}

pub(super) fn is_autosave(path: &Path) -> bool {
    data_dir().is_some_and(|d| path.parent() == Some(d.as_path()))
        && path.file_name().is_some_and(|n| n.to_string_lossy().starts_with("autosave-"))
}

/// The newest autosave left by a session that didn't close cleanly.
pub(super) fn pending_recovery() -> Option<PathBuf> {
    let own = autosave_path();
    let mut found: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(data_dir()?)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| is_autosave(p) && p.extension().is_some_and(|x| x == EXTENSION) && Some(p) != own.as_ref())
        .filter(|p| {
            // A lock we can take means its owner is gone.
            let lock = p.with_extension("lock");
            std::fs::File::open(&lock).map_or(true, |f| f.try_lock().is_ok())
        })
        .filter_map(|p| Some((std::fs::metadata(&p).ok()?.modified().ok()?, p)))
        .collect();
    found.sort();
    found.pop().map(|(_, p)| p)
}

/// Deletes an autosave and its lock file.
pub(super) fn remove_recovery(path: &Path) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(path.with_extension("lock"));
}

/// Deletes this instance's autosave (the work was saved or discarded).
pub(super) fn remove_autosave() {
    if let Some(p) = autosave_path() {
        let _ = std::fs::remove_file(p);
    }
}

pub(super) fn load_recent() -> Vec<PathBuf> {
    let Some(text) = data_dir().and_then(|d| std::fs::read_to_string(d.join("recent.json")).ok()) else { return Vec::new() };
    let list: Vec<String> = serde_json::from_str(&text).unwrap_or_default();
    list.into_iter().map(PathBuf::from).filter(|p| p.is_file()).collect()
}
pub const EXTENSION: &str = "mpp";

/// Texture formats a project can hold, by name.
const FORMATS: [wgpu::TextureFormat; 6] = [
    wgpu::TextureFormat::Rgba8Unorm,
    wgpu::TextureFormat::Rg8Unorm,
    wgpu::TextureFormat::Rg16Float,
    wgpu::TextureFormat::R8Unorm,
    wgpu::TextureFormat::R16Float,
    wgpu::TextureFormat::Rgba16Float,
];

struct Writer {
    blobs: Vec<Vec<u8>>,
}

impl Writer {
    fn blob(&mut self, raw: &[u8]) -> Value {
        self.blobs.push(lz4_flex::compress_prepend_size(raw));
        json!(self.blobs.len() - 1)
    }
}

struct Reader {
    blobs: Vec<Vec<u8>>,
}

impl Reader {
    fn blob(&self, v: &Value) -> Result<Vec<u8>, String> {
        let i = v.as_u64().ok_or("missing blob reference")? as usize;
        let packed = self.blobs.get(i).ok_or(format!("blob {i} is missing"))?;
        lz4_flex::decompress_size_prepended(packed).map_err(|e| format!("blob {i} is damaged: {e}"))
    }
}

fn f(v: &Value, key: &str, default: f32) -> f32 {
    v.get(key).and_then(Value::as_f64).map_or(default, |x| x as f32)
}

fn arr<const N: usize>(v: &Value, key: &str, default: [f32; N]) -> [f32; N] {
    match v.get(key).and_then(Value::as_array) {
        Some(a) if a.len() == N => std::array::from_fn(|i| a[i].as_f64().unwrap_or(default[i] as f64) as f32),
        _ => default,
    }
}

fn b(v: &Value, key: &str, default: bool) -> bool {
    v.get(key).and_then(Value::as_bool).unwrap_or(default)
}

fn s<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

fn fill_json(p: &FillParams) -> Value {
    json!({
        "enabled": p.mat.enabled,
        "color": p.mat.color,
        "roughness": p.mat.rough,
        "metallic": p.mat.metal,
        "height": p.mat.height,
        "color_variation": p.color_var,
        "variation_scale": p.var_scale,
        "roughness_variation": p.rough_var,
        "height_variation": p.height_var,
        "seed": p.seed,
        "part_variation": p.part_var,
        "projection": format!("{:?}", p.projection),
        "tiling": p.tiling,
        "rotation": p.rotation,
        "offset": p.offset,
        "triplanar_sharpness": p.tri_sharpness,
        "graph": p.graph,
    })
}

fn fill_from(v: &Value) -> FillParams {
    let d = FillParams::default();
    let enabled = match v.get("enabled").and_then(Value::as_array) {
        Some(a) if a.len() == 4 => std::array::from_fn(|i| a[i].as_bool().unwrap_or(true)),
        _ => d.mat.enabled,
    };
    FillParams {
        mat: MatValues {
            enabled,
            color: arr(v, "color", d.mat.color),
            rough: f(v, "roughness", d.mat.rough),
            metal: f(v, "metallic", d.mat.metal),
            height: f(v, "height", d.mat.height),
        },
        color_var: f(v, "color_variation", d.color_var),
        var_scale: f(v, "variation_scale", d.var_scale),
        rough_var: f(v, "roughness_variation", d.rough_var),
        height_var: f(v, "height_variation", d.height_var),
        seed: f(v, "seed", d.seed),
        part_var: f(v, "part_variation", d.part_var),
        projection: s(v, "projection").and_then(Projection::from_key).unwrap_or(Projection::Uv),
        tiling: f(v, "tiling", d.tiling),
        rotation: f(v, "rotation", d.rotation),
        offset: arr(v, "offset", d.offset),
        tri_sharpness: f(v, "triplanar_sharpness", d.tri_sharpness),
        graph: v["graph"].as_u64(),
    }
}

fn generator_json(g: &Generator) -> Value {
    json!({
        "kind": g.kind.name(),
        "weights": g.weights,
        "level": g.level,
        "contrast": g.contrast,
        "scale": g.scale,
        "noise": g.noise,
        "invert": g.invert,
        "seed": g.seed,
        "edge_width": g.edge_width,
        "id_color": g.id_color,
        "id_tolerance": g.id_tolerance,
    })
}

fn generator_from(v: &Value) -> Result<Generator, String> {
    let name = s(v, "kind").ok_or("generator without a kind")?;
    let kind = GenKind::ALL.into_iter().find(|k| k.name() == name).ok_or(format!("unknown generator '{name}'"))?;
    let d = kind.default_generator();
    let mut weights = [0.0; crate::engine::GEN_INPUT_COUNT];
    if let Some(a) = v.get("weights").and_then(Value::as_array) {
        // Older projects have fewer inputs; new ones default to 0.
        for (w, x) in weights.iter_mut().zip(a) {
            *w = x.as_f64().unwrap_or(0.0) as f32;
        }
    }
    Ok(Generator {
        kind,
        weights,
        level: f(v, "level", d.level),
        contrast: f(v, "contrast", d.contrast),
        scale: f(v, "scale", d.scale),
        noise: f(v, "noise", d.noise),
        invert: b(v, "invert", d.invert),
        seed: f(v, "seed", d.seed),
        edge_width: f(v, "edge_width", d.edge_width),
        id_color: arr(v, "id_color", d.id_color),
        id_tolerance: f(v, "id_tolerance", d.id_tolerance),
    })
}

impl PainterApp {
    fn tex_json(&self, w: &mut Writer, t: &Tex) -> Value {
        let format = FORMATS.iter().find(|&&x| x == t.format).map(|x| format!("{x:?}"));
        json!({
            "blob": w.blob(&self.engine.read_tex(t)),
            "format": format.expect("only project formats are saved"),
            "width": t.width,
            "height": t.height,
        })
    }

    fn tex_from(&self, r: &Reader, v: &Value, label: &str) -> Result<Tex, String> {
        let name = s(v, "format").ok_or("texture without a format")?;
        let format = FORMATS.into_iter().find(|x| format!("{x:?}") == name).ok_or(format!("unknown texture format '{name}'"))?;
        let (w, h) = (v["width"].as_u64().unwrap_or(0) as u32, v["height"].as_u64().unwrap_or(0) as u32);
        let bytes = r.blob(&v["blob"])?;
        let expected = (w * h * crate::gpu::bytes_per_texel(format)) as usize;
        if w == 0 || bytes.len() != expected {
            return Err(format!("{label} texture has the wrong size"));
        }
        Ok(self.engine.texture_from_bytes(label, format, w, h, &bytes))
    }

    pub(super) fn save_project(&mut self, path: &Path) -> Result<(), String> {
        let bytes = self.write_project(path)?;
        self.project_path = Some(path.to_path_buf());
        self.saved_serial = self.undo.state_serial();
        self.status = format!("Saved {} ({:.1} MB).", path.display(), bytes as f64 / 1048576.0);
        self.remember_recent(path);
        remove_autosave();
        self.autosaved_serial = self.saved_serial;
        Ok(())
    }

    /// Writes the recovery copy if enough has changed since the last one.
    pub(super) fn autosave_if_due(&mut self) {
        let every = self.prefs.autosave_every.max(1) as u64;
        let serial = self.undo.state_serial();
        if !self.prefs.autosave || self.stroke.is_some() || !self.is_modified() || serial.abs_diff(self.autosaved_serial) < every {
            return;
        }
        let Some(path) = autosave_path() else { return };
        self.autosaved_serial = serial;
        if let Err(e) = self.write_project(&path) {
            self.status = format!("Autosave failed: {e}");
        }
    }

    /// Writes the project file; returns its size. Doesn't change which file
    /// the document is saved to.
    fn write_project(&mut self, path: &Path) -> Result<usize, String> {
        self.engine.commit_props(&mut self.undo);
        let mut w = Writer { blobs: Vec::new() };
        let mesh = &self.mesh;
        let mesh_json = json!({
            "name": mesh.name,
            "center": mesh.center.to_array(),
            "scale": mesh.scale,
            "vertices": w.blob(bytemuck::cast_slice(&mesh.vertices)),
            "indices": w.blob(bytemuck::cast_slice(&mesh.indices)),
            "objects": mesh.objects,
            "tri_object": w.blob(bytemuck::cast_slice(&mesh.tri_object)),
            "tri_color": mesh.tri_color.as_ref().map(|c| w.blob(bytemuck::cast_slice(c))),
        });
        let mut layers = Vec::new();
        for l in &self.engine.layers {
            let content = match &l.content {
                Content::Paint(ch) => {
                    let channels: Vec<Value> = ch.iter().map(|t| t.as_ref().map_or(Value::Null, |t| self.tex_json(&mut w, t))).collect();
                    json!({"kind": "paint", "channels": channels})
                }
                Content::Fill(p) => json!({"kind": "fill", "fill": fill_json(p)}),
                Content::Folder(fo) => json!({"kind": "folder", "open": fo.open}),
            };
            let mask = l.mask.as_ref().map(|m| {
                json!({
                    "paint": m.paint.as_ref().map(|t| self.tex_json(&mut w, t)),
                    "generator": m.generator.as_ref().map(generator_json),
                    "graph": m.graph,
                    "filters": m.filters.iter().map(|f| json!({"kind": f.kind.key(), "enabled": f.enabled, "a": f.a, "b": f.b, "c": f.c})).collect::<Vec<_>>(),
                    "layers": m.layers.iter().map(|ml| {
                        let source = match &ml.source {
                            crate::engine::MaskSource::Generator(g) => json!({"generator": generator_json(g)}),
                            crate::engine::MaskSource::Fill(v) => json!({"fill": v}),
                        };
                        json!({"source": source, "blend": ml.blend.key(), "opacity": ml.opacity, "enabled": ml.enabled})
                    }).collect::<Vec<_>>(),
                })
            });
            // A graph's outputs are rendered again on load, not stored.
            let from_graph = matches!(&l.content, Content::Fill(p) if p.graph.is_some());
            let images: Vec<Value> = l
                .images
                .iter()
                .map(|im| im.as_ref().filter(|_| !from_graph))
                .map(|im| im.map_or(Value::Null, |im| json!({"name": im.name, "data": w.blob(&im.encoded)})))
                .collect();
            layers.push(json!({
                "images": images,
                "id": l.id,
                "name": l.name,
                "visible": l.visible,
                "opacity": l.opacity,
                "blend": l.blend.name(),
                "parent": l.parent,
                "content": content,
                "mask": mask,
            }));
        }
        let fixes: Vec<Value> = (0..self.engine.map_fix.len())
            .filter_map(|i| Some(json!({"map": i, "texture": self.tex_json(&mut w, self.engine.map_fix[i].as_ref()?)})))
            .collect();
        let levels: Vec<Value> = self.engine.map_levels.iter().map(|l| json!([l.black, l.white, l.gamma])).collect();
        let bs = &self.bake_settings;
        let bake = json!({
            "quality": format!("{:?}", bs.quality),
            "high_poly": bs.high_poly.as_ref().map(|p| p.display().to_string()),
            "id_source": format!("{:?}", bs.id_source),
            "front": bs.front, "rear": bs.rear, "aa": bs.aa,
            "ao_rays": bs.ao_rays, "ao_distance": bs.ao_distance, "cavity_distance": bs.cavity_distance,
            "thickness_rays": bs.thickness_rays, "thickness_distance": bs.thickness_distance,
            "sky_rays": bs.sky_rays, "sky_cone_deg": bs.sky_cone_deg,
            "curvature_radius": bs.curvature_radius, "curvature_scale": bs.curvature_scale,
            "edge_threshold": bs.edge_threshold, "edge_max_distance": bs.edge_max_distance,
            "edge_auto": bs.edge_auto, "hard_edge_angle": bs.hard_edge_angle,
            "match_by_name": bs.match_by_name,
            "cage": bs.cage.as_ref().map(|p| p.display().to_string()),
        });
        let v = &self.view;
        let p = &v.post;
        let env = match &self.env.source {
            EnvSource::Builtin(i) => json!({"builtin": crate::environment::BUILTIN[*i].0}),
            EnvSource::File(path) => json!({"file": path.display().to_string()}),
        };
        let view = json!({
            "environment": env,
            "exposure": v.exposure, "env_rotation": v.env_rotation, "env_intensity": v.env_intensity,
            "background": format!("{:?}", v.background), "bg_blur": v.bg_blur, "bg_intensity": v.bg_intensity,
            "bg_color": v.bg_color, "height_strength": v.height_strength,
            "show_wire": v.show_wire, "use_baked_normal": v.use_baked_normal,
            "key_light": {"on": v.key.on, "azimuth": v.key.azimuth, "elevation": v.key.elevation,
                "intensity": v.key.intensity, "color": v.key.color},
            "tonemap": format!("{:?}", p.tonemap), "contrast": p.contrast, "saturation": p.saturation,
            "temperature": p.temperature, "tint": p.tint, "vignette": p.vignette, "bloom": p.bloom,
            "bloom_threshold": p.bloom_threshold, "chromatic": p.chromatic, "grain": p.grain, "sharpen": p.sharpen,
            "camera": {"yaw": self.cam.yaw, "pitch": self.cam.pitch, "distance": self.cam.dist,
                       "target": self.cam.target.to_array(), "fov": self.cam.fov_y},
        });
        let manifest = json!({
            "app": "Material Painter",
            // 2: map levels gamma brightens above 1 (version 1 darkened).
            "version": 2,
            "resolution": self.engine.res,
            "mesh": mesh_json,
            "layers_bottom_to_top": layers,
            "graphs": self.graphs.to_json(&self.used_graphs()),
            "selected": self.selected,
            "map_fixes": fixes,
            "map_levels": levels,
            "bake": bake,
            "view": view,
        });
        let text = serde_json::to_vec(&manifest).map_err(|e| e.to_string())?;
        let mut out = Vec::with_capacity(text.len() + w.blobs.iter().map(|b| b.len() + 8).sum::<usize>() + 32);
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&(text.len() as u64).to_le_bytes());
        out.extend_from_slice(&text);
        out.extend_from_slice(&(w.blobs.len() as u64).to_le_bytes());
        for blob in &w.blobs {
            out.extend_from_slice(&(blob.len() as u64).to_le_bytes());
            out.extend_from_slice(blob);
        }
        let tmp = path.with_extension("mpp.tmp");
        std::fs::write(&tmp, &out).map_err(|e| format!("can't write {}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, path).map_err(|e| format!("can't replace {}: {e}", path.display()))?;
        Ok(out.len())
    }

    fn remember_recent(&mut self, path: &Path) {
        if is_autosave(path) {
            return;
        }
        self.recent.retain(|p| p != path);
        self.recent.insert(0, path.to_path_buf());
        self.recent.truncate(self.prefs.recent_count.max(1));
        if let Some(dir) = data_dir() {
            let list: Vec<String> = self.recent.iter().map(|p| p.display().to_string()).collect();
            let _ = std::fs::create_dir_all(&dir);
            let _ = std::fs::write(dir.join("recent.json"), json!(list).to_string());
        }
    }

    pub(super) fn open_project(&mut self, path: &Path, ctx: &egui::Context) -> Result<(), String> {
        let data = std::fs::read(path).map_err(|e| format!("can't read {}: {e}", path.display()))?;
        let bad = || format!("{} isn't a Material Painter project (or is damaged)", path.display());
        if data.len() < 24 || &data[..8] != MAGIC {
            return Err(bad());
        }
        let mut at = 8;
        let mut take = |n: usize| -> Result<&[u8], String> {
            let slice = data.get(at..at + n).ok_or_else(bad)?;
            at += n;
            Ok(slice)
        };
        let u64_at = |b: &[u8]| u64::from_le_bytes(b.try_into().unwrap()) as usize;
        let len = u64_at(take(8)?);
        let manifest: Value = serde_json::from_slice(take(len)?).map_err(|e| format!("{}: {e}", bad()))?;
        let count = u64_at(take(8)?);
        let mut blobs = Vec::with_capacity(count);
        for _ in 0..count {
            let n = u64_at(take(8)?);
            blobs.push(take(n)?.to_vec());
        }
        let r = Reader { blobs };

        // Everything is decoded before the open document is touched.
        let m = &manifest["mesh"];
        let vertices: Vec<Vertex> = bytemuck::pod_collect_to_vec(&r.blob(&m["vertices"])?);
        let indices: Vec<u32> = bytemuck::pod_collect_to_vec(&r.blob(&m["indices"])?);
        if indices.iter().any(|&i| i as usize >= vertices.len()) {
            return Err(bad());
        }
        let center = glam::Vec3::from_array(arr(m, "center", [0.0; 3]));
        let tris = indices.len() / 3;
        let mut mesh = Mesh::from_saved(s(m, "name").unwrap_or("Mesh").to_string(), vertices, indices, center, f(m, "scale", 1.0));
        if let Some(names) = m["objects"].as_array() {
            let tri_object: Vec<u32> = bytemuck::pod_collect_to_vec(&r.blob(&m["tri_object"])?);
            if tri_object.len() == tris && tri_object.iter().all(|&o| (o as usize) < names.len()) {
                mesh.objects = names.iter().map(|n| n.as_str().unwrap_or("").to_string()).collect();
                mesh.tri_object = tri_object;
            }
        }
        if !m["tri_color"].is_null() {
            let colors: Vec<[u8; 3]> = r.blob(&m["tri_color"])?.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
            if colors.len() == tris {
                mesh.tri_color = Some(colors);
            }
        }
        let res = manifest["resolution"].as_u64().unwrap_or(2048) as u32;

        let bv = &manifest["bake"];
        let mut bs = crate::bake::BakeSettings::default();
        if let Some(q) = s(bv, "quality") {
            bs.quality = Quality::ALL.into_iter().find(|x| format!("{x:?}") == q).unwrap_or(bs.quality);
        }
        bs.high_poly = s(bv, "high_poly").map(PathBuf::from).filter(|p| p.is_file());
        bs.front = f(bv, "front", bs.front);
        bs.rear = f(bv, "rear", bs.rear);
        bs.aa = bv["aa"].as_u64().map_or(bs.aa, |x| x as u32);
        bs.ao_rays = bv["ao_rays"].as_u64().map_or(bs.ao_rays, |x| x as u32);
        bs.ao_distance = f(bv, "ao_distance", bs.ao_distance);
        bs.cavity_distance = f(bv, "cavity_distance", bs.cavity_distance);
        bs.thickness_rays = bv["thickness_rays"].as_u64().map_or(bs.thickness_rays, |x| x as u32);
        bs.thickness_distance = f(bv, "thickness_distance", bs.thickness_distance);
        bs.sky_rays = bv["sky_rays"].as_u64().map_or(bs.sky_rays, |x| x as u32);
        bs.sky_cone_deg = f(bv, "sky_cone_deg", bs.sky_cone_deg);
        bs.curvature_radius = f(bv, "curvature_radius", bs.curvature_radius);
        bs.curvature_scale = f(bv, "curvature_scale", bs.curvature_scale);
        bs.edge_threshold = f(bv, "edge_threshold", bs.edge_threshold);
        bs.edge_max_distance = f(bv, "edge_max_distance", bs.edge_max_distance);
        bs.edge_auto = b(bv, "edge_auto", bs.edge_auto);
        bs.hard_edge_angle = f(bv, "hard_edge_angle", bs.hard_edge_angle);
        bs.match_by_name = b(bv, "match_by_name", bs.match_by_name);
        bs.cage = s(bv, "cage").map(PathBuf::from).filter(|p| p.is_file());
        bs.id_source = match s(bv, "id_source") {
            Some("Materials") => IdSource::Materials,
            Some("VertexColors") => IdSource::VertexColors,
            _ => IdSource::MeshParts,
        };

        // Switch mesh and resolution first: layers are created at the new size.
        self.bake_settings = bs;
        while !self.engine.layers.is_empty() {
            self.engine.delete_layer(0, &mut self.undo);
        }
        self.load_mesh(mesh, ctx);
        if res != self.engine.res {
            self.engine.set_resolution(res, &self.gpu_mesh, &mut self.undo);
        }
        let mut layers = Vec::new();
        for lv in manifest["layers_bottom_to_top"].as_array().ok_or_else(bad)? {
            let name = s(lv, "name").unwrap_or("Layer").to_string();
            let c = &lv["content"];
            let content = match s(c, "kind") {
                Some("paint") => {
                    let ch = c["channels"].as_array().ok_or_else(bad)?;
                    let mut out: [Option<Tex>; 4] = [None, None, None, None];
                    for (k, t) in ch.iter().enumerate().take(4) {
                        if !t.is_null() {
                            out[k] = Some(self.tex_from(&r, t, "layer")?);
                        }
                    }
                    Content::Paint(out)
                }
                Some("fill") => Content::Fill(fill_from(&c["fill"])),
                Some("folder") => Content::Folder(Folder { open: b(c, "open", true) }),
                _ => return Err(bad()),
            };
            let mask = match lv.get("mask").filter(|m| !m.is_null()) {
                Some(mv) => Some(Mask {
                    paint: match mv.get("paint").filter(|p| !p.is_null()) {
                        Some(t) => Some(self.tex_from(&r, t, "mask")?),
                        None => None,
                    },
                    generator: match mv.get("generator").filter(|g| !g.is_null()) {
                        Some(g) => Some(generator_from(g)?),
                        None => None,
                    },
                    graph: mv["graph"].as_u64(),
                    filters: mv["filters"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|fv| {
                            let kind = crate::engine::FilterKind::ALL.into_iter().find(|k| Some(k.key()) == fv["kind"].as_str())?;
                            let mut f = crate::engine::MaskFilter::new(kind);
                            f.enabled = fv["enabled"].as_bool().unwrap_or(true);
                            f.a = fv["a"].as_f64().map_or(f.a, |v| v as f32);
                            f.b = fv["b"].as_f64().map_or(f.b, |v| v as f32);
                            f.c = fv["c"].as_f64().map_or(f.c, |v| v as f32);
                            Some(f)
                        })
                        .collect(),
                    layers: mv["layers"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|lv| {
                            let src = &lv["source"];
                            let source = match (src.get("generator"), src["fill"].as_f64()) {
                                (Some(g), _) => crate::engine::MaskSource::Generator(generator_from(g).ok()?),
                                (None, Some(v)) => crate::engine::MaskSource::Fill(v as f32),
                                _ => return None,
                            };
                            Some(crate::engine::MaskLayer {
                                source,
                                blend: crate::engine::MaskBlend::ALL.into_iter().find(|b| Some(b.key()) == lv["blend"].as_str()).unwrap_or(crate::engine::MaskBlend::Multiply),
                                opacity: lv["opacity"].as_f64().unwrap_or(1.0) as f32,
                                enabled: lv["enabled"].as_bool().unwrap_or(true),
                            })
                        })
                        .collect(),
                }),
                None => None,
            };
            let mut images: [Option<crate::engine::FillImage>; 4] = Default::default();
            for (k, iv) in lv["images"].as_array().into_iter().flatten().enumerate().take(4) {
                if !iv.is_null() {
                    let data = std::sync::Arc::new(r.blob(&iv["data"])?);
                    images[k] = Some(self.engine.decode_fill_image(s(iv, "name").unwrap_or("image").to_string(), data)?);
                }
            }
            let blend_name = s(lv, "blend").unwrap_or("Normal");
            layers.push(Layer {
                id: lv["id"].as_u64().ok_or_else(bad)?,
                name,
                visible: b(lv, "visible", true),
                opacity: f(lv, "opacity", 1.0),
                blend: Blend::ALL.into_iter().find(|x| x.name() == blend_name).unwrap_or(Blend::Normal),
                content,
                mask,
                parent: lv["parent"].as_u64(),
                images,
                thumb: None,
            });
        }
        self.engine.restore_layers(layers);
        self.graphs.clear(&self.rs);
        for gv in manifest["graphs"].as_array().into_iter().flatten() {
            let Some(id) = gv["id"].as_u64() else { continue };
            match crate::graph::Graph::from_json(&gv["graph"], &self.graphs.lib) {
                Ok(g) => {
                    self.graphs.insert(&self.rs, g, Some(id));
                }
                Err(e) => self.status = format!("A material graph didn't load: {e}"),
            }
        }
        self.repush_graphs();
        for fv in manifest["map_fixes"].as_array().into_iter().flatten() {
            let map = fv["map"].as_u64().unwrap_or(u64::MAX) as usize;
            if map < self.engine.map_fix.len() {
                self.engine.map_fix[map] = Some(self.tex_from(&r, &fv["texture"], "map fix")?);
            }
        }
        for (dst, lv) in self.engine.map_levels.iter_mut().zip(manifest["map_levels"].as_array().into_iter().flatten()) {
            let a = lv.as_array().filter(|a| a.len() == 3);
            if let Some(a) = a {
                let g = |i: usize| a[i].as_f64().unwrap_or(0.0) as f32;
                // Version 1 projects stored the inverse gamma; keep their look.
                let gamma = if manifest["version"].as_u64().unwrap_or(1) < 2 { 1.0 / g(2).max(1e-3) } else { g(2) };
                *dst = Levels { black: g(0), white: g(1), gamma };
            }
        }
        self.engine.map_levels_changed();

        let v = &manifest["view"];
        let d = crate::viewport::ViewSettings::default();
        let vs = &mut self.view;
        vs.exposure = f(v, "exposure", d.exposure);
        vs.env_rotation = f(v, "env_rotation", d.env_rotation);
        vs.env_intensity = f(v, "env_intensity", d.env_intensity);
        vs.background = Background::ALL.into_iter().find(|x| Some(format!("{x:?}").as_str()) == s(v, "background")).unwrap_or(d.background);
        vs.bg_blur = f(v, "bg_blur", d.bg_blur);
        vs.bg_intensity = f(v, "bg_intensity", d.bg_intensity);
        vs.bg_color = arr(v, "bg_color", d.bg_color);
        vs.height_strength = f(v, "height_strength", d.height_strength);
        vs.show_wire = b(v, "show_wire", d.show_wire);
        vs.use_baked_normal = b(v, "use_baked_normal", d.use_baked_normal);
        let k = &v["key_light"];
        // Projects saved before the key light existed open as they looked.
        vs.key = if k.is_null() {
            crate::viewport::KeyLight { on: false, ..d.key }
        } else {
            crate::viewport::KeyLight {
                on: b(k, "on", d.key.on),
                azimuth: f(k, "azimuth", d.key.azimuth),
                elevation: f(k, "elevation", d.key.elevation),
                intensity: f(k, "intensity", d.key.intensity),
                color: arr(k, "color", d.key.color),
            }
        };
        let p = &mut vs.post;
        p.tonemap = Tonemap::ALL.into_iter().find(|x| Some(format!("{x:?}").as_str()) == s(v, "tonemap")).unwrap_or(d.post.tonemap);
        p.contrast = f(v, "contrast", d.post.contrast);
        p.saturation = f(v, "saturation", d.post.saturation);
        p.temperature = f(v, "temperature", d.post.temperature);
        p.tint = f(v, "tint", d.post.tint);
        p.vignette = f(v, "vignette", d.post.vignette);
        p.bloom = f(v, "bloom", d.post.bloom);
        p.bloom_threshold = f(v, "bloom_threshold", d.post.bloom_threshold);
        p.chromatic = f(v, "chromatic", d.post.chromatic);
        p.grain = f(v, "grain", d.post.grain);
        p.sharpen = f(v, "sharpen", d.post.sharpen);
        let cam = &v["camera"];
        let dc = OrbitCam::default();
        self.cam = OrbitCam {
            yaw: f(cam, "yaw", dc.yaw),
            pitch: f(cam, "pitch", dc.pitch),
            dist: f(cam, "distance", dc.dist),
            target: glam::Vec3::from_array(arr(cam, "target", dc.target.to_array())),
            fov_y: f(cam, "fov", dc.fov_y),
        };
        let env = &v["environment"];
        let source = match (s(env, "builtin"), s(env, "file")) {
            (Some(name), _) => Some(EnvSource::parse(name)),
            (_, Some(file)) if Path::new(file).is_file() => Some(EnvSource::File(PathBuf::from(file))),
            _ => None,
        };
        if let Some(src) = source.filter(|src| *src != self.env.source) {
            self.load_env(src, ctx);
        }

        self.selected = manifest["selected"].as_u64().map_or(0, |x| x as usize).min(self.engine.layers.len().saturating_sub(1));
        self.undo.clear();
        self.saved_serial = self.undo.state_serial();
        self.autosaved_serial = self.saved_serial;
        if is_autosave(path) {
            // Recovered work isn't saved anywhere yet; its file goes once opened.
            remove_recovery(path);
            self.project_path = None;
            self.saved_serial = u64::MAX;
            self.status = "Recovered your unsaved work. Save it to keep it.".into();
        } else {
            self.project_path = Some(path.to_path_buf());
            self.remember_recent(path);
            self.status = format!("Opened {}.", path.display());
        }
        Ok(())
    }

    /// Unsaved edits since the last save or open.
    pub(super) fn is_modified(&self) -> bool {
        self.undo.state_serial() != self.saved_serial || self.engine.has_uncommitted_props()
    }

    pub(super) fn save_dialog(&self) -> Option<PathBuf> {
        let name = self.project_path.as_ref().and_then(|p| p.file_name()).map_or_else(
            || format!("{}.{EXTENSION}", self.mesh.name.replace(' ', "")),
            |n| n.to_string_lossy().into_owned(),
        );
        rfd::FileDialog::new().add_filter("Material Painter project", &[EXTENSION]).set_file_name(name).save_file()
    }

    pub(super) fn open_dialog() -> Option<PathBuf> {
        rfd::FileDialog::new().add_filter("Material Painter project", &[EXTENSION]).pick_file()
    }
}
