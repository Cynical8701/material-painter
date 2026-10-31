//! Bake cage preview: the projection shell drawn over the 3D view, colored by
//! what the bake's rays will find from points across the low poly.
//!
//! Probing the high poly is the slow part and runs once per scene on a worker
//! thread; recoloring for new front/rear distances is a cheap pass over the
//! stored hits, so the preview follows the sliders live.

use crate::bake::{BakeSettings, base_name, components};
use crate::bvh::Bvh;
use crate::mesh::{HighPoly, Mesh, unique_edges};
use crate::viewport::OverlayVertex;
use glam::Vec3;
use std::path::PathBuf;

/// How far the probe looks along each vertex normal: the largest front and
/// rear distance the bake settings allow.
const REACH: f32 = 0.5;

/// What the bake ray from one low-poly vertex finds.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    /// Hits its own surface with room to spare.
    Ok,
    /// Hits, but the high poly is close to the cage or to the rear limit.
    Tight,
    /// Finds nothing: raise front/rear or fix the cage.
    Missed,
    /// Reaches another part's surface before its own: that part bleeds in.
    Overlap,
}

impl Status {
    pub fn color(self) -> [u8; 3] {
        match self {
            Status::Ok => [60, 220, 120],
            Status::Tight => [255, 190, 40],
            Status::Missed => [255, 60, 60],
            Status::Overlap => [230, 70, 230],
        }
    }
}

/// Inputs the probe depends on; a change means probing again.
#[derive(Clone, PartialEq, Debug)]
pub struct ProbeKey {
    pub mesh: String,
    pub vertex_count: usize,
    pub high_poly: Option<PathBuf>,
    pub cage: Option<PathBuf>,
    pub match_by_name: bool,
}

impl ProbeKey {
    pub fn new(mesh: &Mesh, settings: &BakeSettings) -> ProbeKey {
        ProbeKey {
            mesh: mesh.name.clone(),
            vertex_count: mesh.vertices.len(),
            high_poly: settings.high_poly.clone(),
            cage: settings.high_poly.as_ref().and(settings.cage.clone()),
            match_by_name: settings.match_by_name,
        }
    }
}

struct Probe {
    /// Low-poly vertex position.
    pos: Vec3,
    /// Unit direction from the low poly toward the cage.
    dir: Vec3,
    /// Distance to the loaded cage along `dir` (replaces the front distance).
    cage_len: Option<f32>,
    /// Low-poly triangle the sample lies on.
    tri: u32,
    /// High-poly crossings as (offset along `dir` from `pos`, part), filtered
    /// to the parts the bake lets this vertex see.
    hits: Vec<(f32, u32)>,
}

/// Probe results for one scene.
pub struct CageProbe {
    pub key: ProbeKey,
    probes: Vec<Probe>,
    indices: Vec<u32>,
    edges: Vec<u32>,
    pub has_high: bool,
}

/// Share of sample points (spread over the low poly's area) in each state.
#[derive(Clone, Copy, Default, Debug)]
pub struct CageStats {
    pub ok: f32,
    pub tight: f32,
    pub missed: f32,
    pub overlap: f32,
}

impl CageProbe {
    /// Probes the high poly from sample points across the low poly. Runs on
    /// a worker thread.
    pub fn build(mesh: &Mesh, key: ProbeKey) -> Result<CageProbe, String> {
        let normals: Vec<Vec3> = mesh.vertices.iter().map(|v| Vec3::from_array(v.normal).normalize_or_zero()).collect();
        let positions: Vec<Vec3> = mesh.vertices.iter().map(|v| Vec3::from_array(v.pos)).collect();

        // A loaded cage gives each low vertex its own ray start.
        let mut cage_pos: Option<Vec<Vec3>> = None;
        if let Some(path) = &key.cage {
            let cage = HighPoly::load(path, mesh.center, mesh.scale)?;
            if cage.indices.len() != mesh.indices.len() {
                return Err(format!(
                    "The cage has {} triangles but the low poly has {}; a cage must be the low poly with its vertices moved.",
                    cage.indices.len() / 3,
                    mesh.indices.len() / 3
                ));
            }
            let mut c = positions.clone();
            for (&l, &k) in mesh.indices.iter().zip(&cage.indices) {
                c[l as usize] = cage.positions[k as usize];
            }
            cage_pos = Some(c);
        }

        // Sample points: corners, plus a grid across triangles that are large
        // next to the mesh, so big faces are judged over their area rather
        // than at their corners (where rays tend to skim past edges).
        let tri_count = mesh.indices.len() / 3;
        let corner = |t: usize, k: usize| mesh.indices[t * 3 + k] as usize;
        let longest = |t: usize| {
            (0..3).map(|k| positions[corner(t, k)].distance(positions[corner(t, (k + 1) % 3)])).fold(0.0f32, f32::max)
        };
        let mut spacing = (mesh.bounds_max - mesh.bounds_min).length() / 48.0;
        let grid = |t: usize, spacing: f32| ((longest(t) / spacing).ceil() as usize).clamp(1, 12);
        // Keep the sample count bounded on huge meshes.
        while (0..tri_count).map(|t| grid(t, spacing).pow(2)).sum::<usize>() > 400_000 {
            spacing *= 1.5;
        }
        let probe_at = |t: usize, b: [f32; 3]| {
            let mix = |a: &[Vec3]| a[corner(t, 0)] * b[0] + a[corner(t, 1)] * b[1] + a[corner(t, 2)] * b[2];
            let pos = mix(&positions);
            let (dir, cage_len) = match &cage_pos {
                Some(c) => {
                    let span = mix(c) - pos;
                    let len = span.length();
                    // Where the cage touches the low poly the bake falls
                    // back to the normal and the front distance; so does the probe.
                    if len > 1e-6 { (span / len, Some(len)) } else { (mix(&normals).normalize_or_zero(), None) }
                }
                None => (mix(&normals).normalize_or_zero(), None),
            };
            Probe { pos, dir, cage_len, tri: t as u32, hits: Vec::new() }
        };
        let mut probes: Vec<Probe> = Vec::new();
        let mut at_vertex: std::collections::HashMap<usize, u32> = std::collections::HashMap::new();
        let mut indices = Vec::with_capacity(mesh.indices.len());
        for t in 0..tri_count {
            let k = grid(t, spacing);
            // Grid point (i, j): barycentrics (1 - (i + j)/k, i/k, j/k).
            let mut ids = vec![0u32; (k + 1) * (k + 2) / 2];
            let mut n = 0;
            let mut index = vec![vec![0usize; k + 1]; k + 1];
            for i in 0..=k {
                for j in 0..=k - i {
                    index[i][j] = n;
                    n += 1;
                }
            }
            for i in 0..=k {
                for j in 0..=k - i {
                    let b = [1.0 - (i + j) as f32 / k as f32, i as f32 / k as f32, j as f32 / k as f32];
                    let vertex = match (i, j) {
                        (0, 0) => Some(corner(t, 0)),
                        (i, 0) if i == k => Some(corner(t, 1)),
                        (0, j) if j == k => Some(corner(t, 2)),
                        _ => None,
                    };
                    let id = match vertex {
                        Some(v) => *at_vertex.entry(v).or_insert_with(|| {
                            probes.push(probe_at(t, b));
                            probes.len() as u32 - 1
                        }),
                        None => {
                            probes.push(probe_at(t, b));
                            probes.len() as u32 - 1
                        }
                    };
                    ids[index[i][j]] = id;
                }
            }
            for i in 0..k {
                for j in 0..k - i {
                    indices.extend([ids[index[i][j]], ids[index[i + 1][j]], ids[index[i][j + 1]]]);
                    if i + j + 1 < k {
                        indices.extend([ids[index[i + 1][j]], ids[index[i + 1][j + 1]], ids[index[i][j + 1]]]);
                    }
                }
            }
        }
        // The wireframe follows the low poly's own edges, through its corners.
        let edges: Vec<u32> = unique_edges(&mesh.indices).into_iter().map(|v| at_vertex[&(v as usize)]).collect();

        let has_high = key.high_poly.is_some();
        if let Some(path) = &key.high_poly {
            let high = HighPoly::load(path, mesh.center, mesh.scale)?;
            let tris: Vec<[Vec3; 3]> = high.indices.chunks_exact(3).map(|t| [t[0], t[1], t[2]].map(|i| high.positions[i as usize])).collect();
            let bvh = Bvh::build(tris);
            let (tri_part, _) = components(&high.positions, &high.indices);

            // Name groups, as the bake pairs them (0 = sees / is seen by all).
            let mut low_tri_group = vec![0u32; tri_count];
            let mut high_tri_group = vec![0u32; high.indices.len() / 3];
            if key.match_by_name && !mesh.objects.is_empty() && !high.objects.is_empty() {
                let mut groups = std::collections::HashMap::new();
                for name in &mesh.objects {
                    let n = groups.len() as u32 + 1;
                    groups.entry(base_name(name)).or_insert(n);
                }
                let high_group: Vec<u32> = high.objects.iter().map(|n| groups.get(&base_name(n)).copied().unwrap_or(0)).collect();
                let matched: std::collections::HashSet<u32> = high_group.iter().copied().filter(|&g| g > 0).collect();
                if !matched.is_empty() {
                    for (g, &o) in high_tri_group.iter_mut().zip(&high.tri_object) {
                        *g = high_group[o as usize];
                    }
                    for (t, g) in low_tri_group.iter_mut().enumerate() {
                        let group = mesh.tri_object.get(t).map_or(0, |&o| groups[&base_name(&mesh.objects[o as usize])]);
                        *g = if matched.contains(&group) { group } else { 0 };
                    }
                }
            }

            use rayon::prelude::*;
            probes.par_iter_mut().for_each_init(Vec::new, |found, p| {
                let start = p.cage_len.unwrap_or(REACH) + 1e-4;
                let origin = p.pos + p.dir * start;
                bvh.all_hits(origin, -p.dir, start + REACH, found);
                let g = low_tri_group[p.tri as usize];
                p.hits = found
                    .iter()
                    .filter(|&&(_, t)| {
                        let tg = high_tri_group[t as usize];
                        g == 0 || tg == 0 || tg == g
                    })
                    .map(|&(t, tri)| (start - t, tri_part[tri as usize]))
                    .collect();
            });
        }
        Ok(CageProbe { key, probes, indices, edges, has_high })
    }

    fn status(&self, p: &Probe, front: f32, rear: f32) -> Status {
        let front = p.cage_len.unwrap_or(front);
        // The bake ray starts on the cage and keeps the first surface it meets.
        let first = p.hits.iter().filter(|h| h.0 <= front && h.0 >= -rear).max_by(|a, b| a.0.total_cmp(&b.0));
        let Some(&(h, part)) = first else {
            return Status::Missed;
        };
        // The surface the vertex really belongs to is the nearest one.
        let own = p.hits.iter().min_by(|a, b| a.0.abs().total_cmp(&b.0.abs())).map(|o| o.1);
        if own != Some(part) {
            Status::Overlap
        } else if h > front * 0.85 || h < -rear * 0.85 {
            Status::Tight
        } else {
            Status::Ok
        }
    }

    /// Overlay geometry for the given distances: a translucent shell and its
    /// wireframe, colored per vertex.
    pub fn overlay(&self, front: f32, rear: f32) -> (Vec<OverlayVertex>, Vec<OverlayVertex>, CageStats) {
        let mut stats = CageStats::default();
        let colors: Vec<[u8; 3]> = self
            .probes
            .iter()
            .map(|p| {
                if !self.has_high {
                    return [90, 160, 255];
                }
                let s = self.status(p, front, rear);
                match s {
                    Status::Ok => stats.ok += 1.0,
                    Status::Tight => stats.tight += 1.0,
                    Status::Missed => stats.missed += 1.0,
                    Status::Overlap => stats.overlap += 1.0,
                }
                s.color()
            })
            .collect();
        let n = self.probes.len().max(1) as f32;
        stats = CageStats { ok: stats.ok / n, tight: stats.tight / n, missed: stats.missed / n, overlap: stats.overlap / n };
        let shell: Vec<Vec3> = self.probes.iter().map(|p| p.pos + p.dir * p.cage_len.unwrap_or(front)).collect();
        let vertex = |i: u32, alpha: u8| {
            let c = colors[i as usize];
            OverlayVertex { pos: shell[i as usize].to_array(), color: [c[0], c[1], c[2], alpha] }
        };
        let tris = self.indices.iter().map(|&i| vertex(i, 46)).collect();
        let lines = self.edges.iter().map(|&i| vertex(i, 120)).collect();
        (tris, lines, stats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::Primitive;

    /// A sphere baked from a slightly larger sphere: covered when the front
    /// distance reaches it, missed when it doesn't.
    #[test]
    fn statuses_follow_the_distances() {
        let low = Primitive::Sphere.build();
        let dir = std::env::temp_dir().join(format!("mp_cage_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("high.obj");
        // The high poly: the same sphere scaled up 5% (in the low poly's
        // original units, which `HighPoly::load` normalizes like the low).
        let mut obj = String::new();
        for v in &low.vertices {
            let p = Vec3::from_array(v.pos) / low.scale + low.center;
            let p = (p - low.center) * 1.05 + low.center;
            obj.push_str(&format!("v {} {} {}\n", p.x, p.y, p.z));
        }
        for t in low.indices.chunks_exact(3) {
            obj.push_str(&format!("f {} {} {}\n", t[0] + 1, t[1] + 1, t[2] + 1));
        }
        std::fs::write(&path, obj).unwrap();
        let key = ProbeKey { mesh: low.name.clone(), vertex_count: low.vertices.len(), high_poly: Some(path), cage: None, match_by_name: true };
        let probe = CageProbe::build(&low, key).unwrap();
        // The shell sits ~0.05 outside (the sphere has radius ~1).
        let (_, _, near) = probe.overlay(0.02, 0.02);
        assert!(near.missed > 0.95, "{near:?}");
        let (tris, lines, far) = probe.overlay(0.2, 0.05);
        assert!(far.ok > 0.95, "{far:?}");
        assert!(tris.len() >= low.indices.len() && tris.len() % 3 == 0);
        assert!(!lines.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
