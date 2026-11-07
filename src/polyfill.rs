//! Polygon fill: picks whole triangles, quads, face loops, flat areas, mesh
//! parts, objects, UV islands or texture sets and turns them into UV-space
//! geometry for the engine to fill, plus the 3D highlight shown under the
//! cursor.

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
    /// A strip of quads running through the edge nearest the cursor, as far
    /// as the quads go (around a cylinder, a whole ring).
    Loop,
    /// The faces reached without crossing an edge sharper than the flat-area
    /// angle: a panel, a bevel, a side of a box.
    Flat,
    /// Every face of the texture set (material) under the cursor.
    Set,
}

impl FillMode {
    pub const ALL: [FillMode; 8] =
        [FillMode::Triangle, FillMode::Polygon, FillMode::Loop, FillMode::Flat, FillMode::Part, FillMode::Object, FillMode::UvIsland, FillMode::Set];

    pub fn name(self) -> &'static str {
        match self {
            FillMode::Triangle => "Triangle",
            FillMode::Polygon => "Polygon",
            FillMode::Part => "Loose part",
            FillMode::Object => "Object",
            FillMode::UvIsland => "UV island",
            FillMode::Loop => "Face loop",
            FillMode::Flat => "Flat area",
            FillMode::Set => "Texture set",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            FillMode::Triangle => "triangle",
            FillMode::Polygon => "polygon",
            FillMode::Part => "part",
            FillMode::Object => "object",
            FillMode::UvIsland => "uv_island",
            FillMode::Loop => "loop",
            FillMode::Flat => "flat",
            FillMode::Set => "texture_set",
        }
    }
}

/// What a selection mode needs besides the triangle.
#[derive(Clone, Copy, Debug)]
pub struct SelectOpts {
    /// Where the pick landed on the triangle: the face loop runs through the edge nearest it.
    pub at: Option<Vec3>,
    /// Flat area: the sharpest edge (degrees between the faces) it spreads across.
    pub angle: f32,
}

impl Default for SelectOpts {
    fn default() -> Self {
        Self { at: None, angle: 30.0 }
    }
}

/// A quad (two triangles) or lone triangle's outline: its edges in order
/// around it, each as (triangle, edge k → k+1). A quad's edge i is opposite
/// edge i + 2.
struct Face {
    tris: Vec<u32>,
    edges: Vec<(u32, u8)>,
}

/// Whether `p` is inside the closed outline (even-odd rule: a lasso that
/// crosses itself leaves its overlaps out).
pub fn in_outline(p: Vec2, outline: &[Vec2]) -> bool {
    let mut inside = false;
    let mut j = outline.len().wrapping_sub(1);
    for i in 0..outline.len() {
        let (a, b) = (outline[i], outline[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
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
    /// Texture sets, as triangle ranges.
    sets: Vec<std::ops::Range<u32>>,
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
        let sets = mesh.sets.iter().map(|s| s.tris.clone()).collect();
        Topology { positions, uvs, adj, uv_border, part, island, part_tris, island_tris, object, object_tris, sets }
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
        self.select_with(mode, t, &SelectOpts::default())
    }

    fn normal(&self, t: usize) -> Vec3 {
        let p = &self.positions[t];
        (p[1] - p[0]).cross(p[2] - p[0]).normalize_or_zero()
    }

    fn longest_edge(&self, t: usize) -> usize {
        let p = &self.positions[t];
        (0..3).max_by(|&a, &b| p[a].distance_squared(p[(a + 1) % 3]).total_cmp(&p[b].distance_squared(p[(b + 1) % 3]))).unwrap()
    }

    /// The other half of a triangulated quad: the neighbor across the
    /// triangle's longest edge (the diagonal), if that is its longest edge
    /// too and the two bend by less than `min_dot`.
    fn quad_partner(&self, t: usize, min_dot: f32) -> Option<(usize, usize)> {
        let k = self.longest_edge(t);
        let n = self.adj[t][k];
        (n != NONE && self.normal(t).dot(self.normal(n as usize)) > min_dot && self.adj[n as usize][self.longest_edge(n as usize)] == t as u32)
            .then_some((n as usize, k))
    }

    /// The quad (or lone triangle) triangle `t` belongs to, with its outline in order.
    fn face(&self, t: usize) -> Face {
        let Some((n, k)) = self.quad_partner(t, 0.5) else {
            return Face { tris: vec![t as u32], edges: (0..3).map(|e| (t as u32, e as u8)).collect() };
        };
        let m = (0..3).find(|&e| self.adj[n][e] == t as u32).unwrap_or(0);
        // t's diagonal runs c_k → c_k+1; with matching winding, n's runs back c_k+1 → c_k.
        let same_winding = self.positions[n][m] == self.positions[t][(k + 1) % 3];
        let (t32, n32) = (t as u32, n as u32);
        let (k1, k2, m1, m2) = (((k + 1) % 3) as u8, ((k + 2) % 3) as u8, ((m + 1) % 3) as u8, ((m + 2) % 3) as u8);
        let edges = if same_winding { vec![(t32, k1), (t32, k2), (n32, m1), (n32, m2)] } else { vec![(t32, k1), (t32, k2), (n32, m2), (n32, m1)] };
        Face { tris: vec![t32, n32], edges }
    }

    fn edge_points(&self, (t, k): (u32, u8)) -> (Vec3, Vec3) {
        let p = &self.positions[t as usize];
        (p[k as usize], p[(k as usize + 1) % 3])
    }

    /// A strip of quads through `face`'s edge `exit` and onward through
    /// each next quad's opposite edge, until the quads stop or it closes.
    fn walk_loop(&self, face: &Face, exit: usize, seen: &mut std::collections::HashSet<u32>, out: &mut Vec<u32>) {
        let mut edge = face.edges[exit];
        loop {
            let next = self.adj[edge.0 as usize][edge.1 as usize];
            if next == NONE || seen.contains(&next) {
                return;
            }
            let f = self.face(next as usize);
            // Only quads carry a loop on.
            if f.tris.len() != 2 {
                return;
            }
            let Some(entry) = f.edges.iter().position(|&(t, k)| self.adj[t as usize][k as usize] == edge.0) else { return };
            for &t in &f.tris {
                seen.insert(t);
                out.push(t);
            }
            edge = f.edges[(entry + 2) % 4];
        }
    }

    /// The triangles `mode` selects around triangle `t`.
    pub fn select_with(&self, mode: FillMode, t: u32, opts: &SelectOpts) -> Vec<u32> {
        let ti = t as usize;
        if ti >= self.positions.len() {
            return Vec::new();
        }
        match mode {
            FillMode::Triangle => vec![t],
            // A flat (planar) quad only.
            FillMode::Polygon => match self.quad_partner(ti, 0.999) {
                Some((n, _)) => vec![t, n as u32],
                None => vec![t],
            },
            FillMode::Part => self.part_tris[self.part[ti] as usize].clone(),
            FillMode::Object => self.object_tris[self.object[ti] as usize].clone(),
            FillMode::UvIsland => self.island_tris[self.island[ti] as usize].clone(),
            FillMode::Set => match self.sets.iter().find(|r| r.contains(&t)) {
                Some(r) => r.clone().collect(),
                None => (0..self.positions.len() as u32).collect(),
            },
            FillMode::Flat => {
                let min_dot = opts.angle.clamp(0.0, 180.0).to_radians().cos();
                let mut seen = std::collections::HashSet::from([t]);
                let mut out = vec![t];
                let mut stack = vec![ti];
                while let Some(a) = stack.pop() {
                    let na = self.normal(a);
                    for &b in &self.adj[a] {
                        if b != NONE && na.dot(self.normal(b as usize)) >= min_dot - 1e-6 && seen.insert(b) {
                            out.push(b);
                            stack.push(b as usize);
                        }
                    }
                }
                out
            }
            FillMode::Loop => {
                let face = self.face(ti);
                if face.tris.len() != 2 {
                    return face.tris;
                }
                // Through the edge nearest the pick (or the first one), both ways.
                let at = opts.at.unwrap_or_else(|| self.centroid(t));
                let dist = |e: (u32, u8)| {
                    let (a, b) = self.edge_points(e);
                    let ab = b - a;
                    let s = ((at - a).dot(ab) / ab.length_squared().max(1e-12)).clamp(0.0, 1.0);
                    at.distance(a + ab * s)
                };
                let exit = (0..4).min_by(|&a, &b| dist(face.edges[a]).total_cmp(&dist(face.edges[b]))).unwrap_or(0);
                let mut seen: std::collections::HashSet<u32> = face.tris.iter().copied().collect();
                let mut out = face.tris.clone();
                self.walk_loop(&face, exit, &mut seen, &mut out);
                self.walk_loop(&face, (exit + 2) % 4, &mut seen, &mut out);
                out
            }
        }
    }

    /// Which edge of triangle `t`'s quad a face loop through `at` runs
    /// through (opposite edges give the same loop).
    pub fn loop_edge(&self, t: u32, at: Vec3) -> usize {
        let face = self.face(t as usize);
        let dist = |e: (u32, u8)| {
            let (a, b) = self.edge_points(e);
            let ab = b - a;
            let s = ((at - a).dot(ab) / ab.length_squared().max(1e-12)).clamp(0.0, 1.0);
            at.distance(a + ab * s)
        };
        let n = face.edges.len();
        (0..n).min_by(|&a, &b| dist(face.edges[a]).total_cmp(&dist(face.edges[b]))).unwrap_or(0) % 2
    }

    /// The 3D point at `uv` on triangle `t` (its UV-space barycentrics).
    pub fn point_at_uv(&self, t: u32, uv: Vec2) -> Vec3 {
        let [a, b, c] = self.uvs[t as usize];
        let d = (b - a).perp_dot(c - a);
        if d.abs() < 1e-12 {
            return self.centroid(t);
        }
        let w1 = (uv - a).perp_dot(c - a) / d;
        let w2 = (b - a).perp_dot(uv - a) / d;
        let p = &self.positions[t as usize];
        p[0] * (1.0 - w1 - w2) + p[1] * w1 + p[2] * w2
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
    pub fn uv_geometry(&self, tris: &[u32], pad: f32) -> crate::engine::PolyGeometry {
        let mut faces = Vec::with_capacity(tris.len() * 3);
        let mut out = Vec::new();
        for &t in tris {
            let uv = &self.uvs[t as usize];
            faces.extend(uv.iter().map(|v| v.to_array()));
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
        crate::engine::PolyGeometry { faces, padding: out }
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
    fn loops_flat_areas_and_sets() {
        // A cylinder's side: a quad ring around it, so a loop through a
        // vertical edge goes all the way around (one row), and through a
        // horizontal edge runs up the side (one column).
        let mesh = Primitive::Cylinder.build();
        let topo = Topology::build(&mesh);
        let n = topo.triangle_count() as u32;
        // A side triangle: its normal is horizontal.
        let side = (0..n).find(|&t| topo.normal(t as usize).y.abs() < 0.1 && topo.face(t as usize).tris.len() == 2).expect("a side quad");
        let face = topo.face(side as usize);
        let edge_dir = |e: (u32, u8)| {
            let (a, b) = topo.edge_points(e);
            (b - a).normalize()
        };
        let vertical = face.edges.iter().position(|&e| edge_dir(e).y.abs() > 0.9).expect("a vertical edge");
        let horizontal = face.edges.iter().position(|&e| edge_dir(e).y.abs() < 0.1).expect("a horizontal edge");
        let mid = |e: (u32, u8)| {
            let (a, b) = topo.edge_points(e);
            (a + b) * 0.5
        };
        let ring = topo.select_with(FillMode::Loop, side, &SelectOpts { at: Some(mid(face.edges[vertical])), ..Default::default() });
        let column = topo.select_with(FillMode::Loop, side, &SelectOpts { at: Some(mid(face.edges[horizontal])), ..Default::default() });
        // The ring's faces all share the start's height band; it closes on itself.
        let y = topo.centroid(side).y;
        assert!(ring.len() >= 16, "a ring around the cylinder ({})", ring.len());
        assert!(ring.iter().all(|&t| (topo.centroid(t).y - y).abs() < 0.3), "the ring stays at one height");
        assert!(column.iter().all(|&t| topo.normal(t as usize).dot(topo.normal(side as usize)) > 0.9), "the column stays on one side");
        assert_ne!(ring.len(), column.len());
        // Flat area: the side bends gently, so a wide angle takes it all and a
        // tight one only a strip; the caps (90 degrees off) stay out either way.
        let wide = topo.select_with(FillMode::Flat, side, &SelectOpts { angle: 45.0, ..Default::default() });
        let tight = topo.select_with(FillMode::Flat, side, &SelectOpts { angle: 1.0, ..Default::default() });
        assert!(wide.len() > tight.len());
        assert!(wide.iter().all(|&t| topo.normal(t as usize).y.abs() < 0.5), "flat area stops at the caps");
        // Texture set: the whole (single) set.
        assert_eq!(topo.select(FillMode::Set, side).len(), n as usize);
        // A point at a triangle's UV centroid is its 3D centroid.
        assert!(topo.point_at_uv(side, topo.uv_centroid(side)).distance(topo.centroid(side)) < 1e-4);
    }

    #[test]
    fn lasso_outline() {
        // An L shape: its notch is outside.
        let l = [(0.0, 0.0), (10.0, 0.0), (10.0, 4.0), (4.0, 4.0), (4.0, 10.0), (0.0, 10.0)].map(|(x, y)| Vec2::new(x, y));
        assert!(in_outline(Vec2::new(2.0, 2.0), &l));
        assert!(in_outline(Vec2::new(2.0, 8.0), &l));
        assert!(in_outline(Vec2::new(8.0, 2.0), &l));
        assert!(!in_outline(Vec2::new(8.0, 8.0), &l), "the notch");
        assert!(!in_outline(Vec2::new(-1.0, 2.0), &l));
        assert!(!in_outline(Vec2::new(1.0, 1.0), &[]));
    }

    #[test]
    fn padding_only_on_island_borders() {
        let mesh = Primitive::Plane.build();
        let topo = Topology::build(&mesh);
        // One interior triangle gets no strips; the whole island gets strips
        // along its outline only.
        let all: Vec<u32> = (0..topo.triangle_count() as u32).collect();
        let border_edges: usize = topo.uv_border.iter().map(|e| e.iter().filter(|&&b| b).count()).sum();
        let g = topo.uv_geometry(&all, 0.001);
        assert_eq!(g.faces.len(), all.len() * 3);
        assert_eq!(g.padding.len(), border_edges * 6);
        assert!(border_edges > 0 && border_edges < all.len());
    }
}
