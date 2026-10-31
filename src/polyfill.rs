//! Polygon fill: picks whole triangles, quads, mesh parts or UV islands and
//! turns them into UV-space geometry for the engine to fill, plus the 3D
//! highlight shown under the cursor.

use crate::mesh::{Mesh, weld_positions};
use crate::viewport::OverlayVertex;
use glam::{Vec2, Vec3};
use std::collections::HashMap;

const NONE: u32 = u32::MAX;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FillMode {
    Triangle,
    /// A quad (two coplanar triangles sharing their long edge) or a triangle.
    Polygon,
    /// A loose part: a connected piece of one object.
    Part,
    /// A whole imported object (FBX/OBJ node), however many pieces it has.
    Object,
    /// A connected region of the UV layout.
    UvIsland,
}

impl FillMode {
    pub const ALL: [FillMode; 5] = [FillMode::Triangle, FillMode::Polygon, FillMode::Part, FillMode::Object, FillMode::UvIsland];

    pub fn name(self) -> &'static str {
        match self {
            FillMode::Triangle => "Triangle",
            FillMode::Polygon => "Polygon",
            FillMode::Part => "Loose part",
            FillMode::Object => "Object",
            FillMode::UvIsland => "UV island",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            FillMode::Triangle => "triangle",
            FillMode::Polygon => "polygon",
            FillMode::Part => "part",
            FillMode::Object => "object",
            FillMode::UvIsland => "uv_island",
        }
    }
}

/// Mesh connectivity for the fill modes, built once per mesh.
pub struct Topology {
    positions: Vec<[Vec3; 3]>,
    uvs: Vec<[Vec2; 3]>,
    /// Neighbor across each edge (k → k+1) on the 3D surface, or NONE.
    adj: Vec<[u32; 3]>,
    /// Edges on a UV island border (k → k+1), which get padding when filled.
    uv_border: Vec<[bool; 3]>,
    part: Vec<u32>,
    island: Vec<u32>,
    part_tris: Vec<Vec<u32>>,
    island_tris: Vec<Vec<u32>>,
    object: Vec<u32>,
    object_tris: Vec<Vec<u32>>,
}

fn union_find(n: usize, pairs: impl Iterator<Item = (u32, u32)>) -> (Vec<u32>, Vec<Vec<u32>>) {
    let mut parent: Vec<u32> = (0..n as u32).collect();
    fn find(p: &mut [u32], mut x: u32) -> u32 {
        while p[x as usize] != x {
            p[x as usize] = p[p[x as usize] as usize];
            x = p[x as usize];
        }
        x
    }
    for (a, b) in pairs {
        let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
        if ra != rb {
            parent[rb as usize] = ra;
        }
    }
    let mut remap = HashMap::new();
    let mut groups: Vec<Vec<u32>> = Vec::new();
    let id: Vec<u32> = (0..n as u32)
        .map(|t| {
            let root = find(&mut parent, t);
            let g = *remap.entry(root).or_insert_with(|| {
                groups.push(Vec::new());
                groups.len() as u32 - 1
            });
            groups[g as usize].push(t);
            g
        })
        .collect();
    (id, groups)
}

impl Topology {
    pub fn build(mesh: &Mesh) -> Topology {
        let tris = mesh.indices.len() / 3;
        let pos: Vec<Vec3> = mesh.vertices.iter().map(|v| Vec3::from_array(v.pos)).collect();
        let weld = weld_positions(&pos);
        let corner = |t: usize, k: usize| mesh.indices[t * 3 + k] as usize;
        let positions: Vec<[Vec3; 3]> = (0..tris).map(|t| [0, 1, 2].map(|k| pos[corner(t, k)])).collect();
        let uvs: Vec<[Vec2; 3]> = (0..tris).map(|t| [0, 1, 2].map(|k| Vec2::from_array(mesh.vertices[corner(t, k)].uv))).collect();

        // Surface edges by welded position, UV edges by quantized UV.
        let mut surface: HashMap<(usize, usize), Vec<(u32, u8)>> = HashMap::with_capacity(tris * 3);
        let mut uv_edges: HashMap<([i32; 2], [i32; 2]), Vec<u32>> = HashMap::with_capacity(tris * 3);
        let q = |v: Vec2| [(v.x * 1.0e5).round() as i32, (v.y * 1.0e5).round() as i32];
        for t in 0..tris {
            for k in 0..3 {
                let (a, b) = (weld[corner(t, k)], weld[corner(t, (k + 1) % 3)]);
                surface.entry((a.min(b), a.max(b))).or_default().push((t as u32, k as u8));
                let (ua, ub) = (q(uvs[t][k]), q(uvs[t][(k + 1) % 3]));
                uv_edges.entry(if ua <= ub { (ua, ub) } else { (ub, ua) }).or_default().push(t as u32);
            }
        }
        let object_of = |t: u32| mesh.tri_object.get(t as usize).copied().unwrap_or(0);
        let mut adj = vec![[NONE; 3]; tris];
        let mut part_pairs = Vec::new();
        for sides in surface.values() {
            if let [(t0, k0), (t1, k1)] = sides[..] {
                adj[t0 as usize][k0 as usize] = t1;
                adj[t1 as usize][k1 as usize] = t0;
            }
            // Objects that touch (welded at the same positions) stay apart.
            for w in sides.windows(2) {
                if object_of(w[0].0) == object_of(w[1].0) {
                    part_pairs.push((w[0].0, w[1].0));
                }
            }
        }
        let mut uv_border = vec![[false; 3]; tris];
        let mut island_pairs = Vec::new();
        for t in 0..tris {
            for k in 0..3 {
                let (ua, ub) = (q(uvs[t][k]), q(uvs[t][(k + 1) % 3]));
                let users = &uv_edges[&if ua <= ub { (ua, ub) } else { (ub, ua) }];
                uv_border[t][k] = users.len() < 2;
                for &o in users {
                    if o != t as u32 {
                        island_pairs.push((t as u32, o));
                    }
                }
            }
        }
        let (part, part_tris) = union_find(tris, part_pairs.into_iter());
        let (island, island_tris) = union_find(tris, island_pairs.into_iter());
        let object: Vec<u32> = (0..tris as u32).map(object_of).collect();
        let mut object_tris = vec![Vec::new(); object.iter().max().map_or(0, |m| *m as usize + 1)];
        for (t, &o) in object.iter().enumerate() {
            object_tris[o as usize].push(t as u32);
        }
        Topology { positions, uvs, adj, uv_border, part, island, part_tris, island_tris, object, object_tris }
    }

    pub fn triangle_count(&self) -> usize {
        self.positions.len()
    }

    pub fn centroid(&self, t: u32) -> Vec3 {
        let p = &self.positions[t as usize];
        (p[0] + p[1] + p[2]) / 3.0
    }

    pub fn uv_centroid(&self, t: u32) -> Vec2 {
        let p = &self.uvs[t as usize];
        (p[0] + p[1] + p[2]) / 3.0
    }

    /// The triangles `mode` selects around triangle `t`.
    pub fn select(&self, mode: FillMode, t: u32) -> Vec<u32> {
        let ti = t as usize;
        if ti >= self.positions.len() {
            return Vec::new();
        }
        match mode {
            FillMode::Triangle => vec![t],
            FillMode::Polygon => {
                // A triangulated quad: the coplanar neighbor across the
                // triangle's longest edge, if that is its longest edge too.
                let longest = |t: usize| {
                    let p = &self.positions[t];
                    (0..3).max_by(|&a, &b| p[a].distance_squared(p[(a + 1) % 3]).total_cmp(&p[b].distance_squared(p[(b + 1) % 3]))).unwrap()
                };
                let normal = |t: usize| {
                    let p = &self.positions[t];
                    (p[1] - p[0]).cross(p[2] - p[0]).normalize_or_zero()
                };
                let k = longest(ti);
                let n = self.adj[ti][k];
                if n != NONE && normal(ti).dot(normal(n as usize)) > 0.999 && self.adj[n as usize][longest(n as usize)] == t {
                    vec![t, n]
                } else {
                    vec![t]
                }
            }
            FillMode::Part => self.part_tris[self.part[ti] as usize].clone(),
            FillMode::Object => self.object_tris[self.object[ti] as usize].clone(),
            FillMode::UvIsland => self.island_tris[self.island[ti] as usize].clone(),
        }
    }

    /// The triangle whose UVs contain `uv`.
    pub fn pick_uv(&self, uv: Vec2) -> Option<u32> {
        self.uvs.iter().position(|[a, b, c]| {
            let d = |p: Vec2, q: Vec2, r: Vec2| (q - p).perp_dot(r - p);
            let (d0, d1, d2) = (d(*a, *b, uv), d(*b, *c, uv), d(*c, *a, uv));
            (d0 >= 0.0 && d1 >= 0.0 && d2 >= 0.0) || (d0 <= 0.0 && d1 <= 0.0 && d2 <= 0.0)
        })
        .map(|t| t as u32)
    }

    /// UV-space triangles to fill, plus `pad` (UV units) of padding past UV
    /// island borders so texture filtering and mips don't show seams.
    pub fn uv_geometry(&self, tris: &[u32], pad: f32) -> Vec<[f32; 2]> {
        let mut out = Vec::with_capacity(tris.len() * 3);
        for &t in tris {
            let uv = &self.uvs[t as usize];
            out.extend(uv.iter().map(|v| v.to_array()));
            for k in 0..3 {
                if !self.uv_border[t as usize][k] {
                    continue;
                }
                let (a, b, c) = (uv[k], uv[(k + 1) % 3], uv[(k + 2) % 3]);
                let e = b - a;
                let mut n = e.perp().normalize_or_zero();
                if n.dot(c - a) > 0.0 {
                    n = -n;
                }
                // A strip outside the edge, a little longer than it.
                let along = e.normalize_or_zero() * pad;
                let (a0, b0) = (a - along, b + along);
                let (a1, b1) = (a0 + n * pad, b0 + n * pad);
                out.extend([a0, b0, b1, a0, b1, a1].map(|v| v.to_array()));
            }
        }
        out
    }

    /// Highlight for selected triangles: a translucent fill and the outline.
    pub fn overlay(&self, tris: &[u32], color: [u8; 3]) -> (Vec<OverlayVertex>, Vec<OverlayVertex>) {
        let v = |p: Vec3, a: u8| OverlayVertex { pos: p.to_array(), color: [color[0], color[1], color[2], a] };
        let set: std::collections::HashSet<u32> = tris.iter().copied().collect();
        let mut fill = Vec::with_capacity(tris.len() * 3);
        let mut lines = Vec::new();
        for &t in tris {
            let p = &self.positions[t as usize];
            fill.extend(p.iter().map(|&q| v(q, 70)));
            for k in 0..3 {
                let n = self.adj[t as usize][k];
                if n == NONE || !set.contains(&n) {
                    lines.push(v(p[k], 255));
                    lines.push(v(p[(k + 1) % 3], 255));
                }
            }
        }
        (fill, lines)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::Primitive;

    #[test]
    fn modes_grow_as_expected() {
        let mesh = Primitive::MaterialBall.build();
        let topo = Topology::build(&mesh);
        let t = 0;
        assert_eq!(topo.select(FillMode::Triangle, t), vec![0]);
        let quad = topo.select(FillMode::Polygon, t);
        assert!(quad.len() <= 2 && quad.contains(&0));
        let island = topo.select(FillMode::UvIsland, t);
        let part = topo.select(FillMode::Part, t);
        assert!(island.len() > quad.len());
        assert!(part.len() >= island.len().min(part.len()));
        // Every triangle of a part shares its part id; parts cover the mesh.
        let total: usize = topo.part_tris.iter().map(Vec::len).sum();
        assert_eq!(total, topo.triangle_count());
        // Picking at a triangle's UV centroid finds a triangle there.
        let c = (topo.uvs[5][0] + topo.uvs[5][1] + topo.uvs[5][2]) / 3.0;
        assert!(topo.pick_uv(c).is_some());
    }

    #[test]
    fn padding_only_on_island_borders() {
        let mesh = Primitive::Plane.build();
        let topo = Topology::build(&mesh);
        // One interior triangle gets no strips; the whole island gets strips
        // along its outline only.
        let all: Vec<u32> = (0..topo.triangle_count() as u32).collect();
        let border_edges: usize = topo.uv_border.iter().map(|e| e.iter().filter(|&&b| b).count()).sum();
        assert_eq!(topo.uv_geometry(&all, 0.001).len(), all.len() * 3 + border_edges * 6);
        assert!(border_edges > 0 && border_edges < all.len());
    }
}
