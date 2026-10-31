// Low-poly surface in UV space for the baker: full-precision position,
// normal and tangent per texel, then dilated past island borders.
// Position w: 1 = inside a UV island, 0.5 = dilated padding, 0 = empty.

struct VIn {
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) tangent: vec4<f32>,
};

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) wpos: vec3<f32>,
    @location(1) n: vec3<f32>,
    @location(2) t: vec4<f32>,
    // Name group for match-by-name bakes: each group draws as its own instance.
    @location(3) @interpolate(flat) group: u32,
    // Tangent w unsmoothed: handedness, times the triangle's BVH slot + 2 when
    // drawn from per-corner low-poly vertices (1 otherwise).
    @location(4) @interpolate(flat) tw: f32,
};

@vertex
fn vs_uv(v: VIn, @builtin(instance_index) group: u32) -> VOut {
    var out: VOut;
    out.group = group;
    out.clip = vec4<f32>(v.uv.x * 2.0 - 1.0, 1.0 - v.uv.y * 2.0, 0.5, 1.0);
    out.wpos = v.pos;
    out.n = v.normal;
    out.t = v.tangent;
    out.tw = v.tangent.w;
    return out;
}

struct Out3 {
    @location(0) pos: vec4<f32>,
    @location(1) nrm: vec4<f32>,
    @location(2) tan: vec4<f32>,
};

@fragment
fn fs_uv(in: VOut) -> Out3 {
    var out: Out3;
    out.pos = vec4<f32>(in.wpos, 1.0);
    // Normal w carries the name group (0 = unmatched).
    // Interpolated but not normalized: the tangent-space encoding needs the
    // exact frame a MikkTSpace pixel shader reconstructs.
    out.nrm = vec4<f32>(in.n, f32(in.group));
    out.tan = vec4<f32>(in.t.xyz, in.tw);
    return out;
}

@group(0) @binding(0) var src_pos: texture_2d<f32>;
@group(0) @binding(1) var src_nrm: texture_2d<f32>;
@group(0) @binding(2) var src_tan: texture_2d<f32>;

@fragment
fn fs_dilate(in: FsOut) -> Out3 {
    let c = vec2<i32>(in.pos.xy);
    var out: Out3;
    let p = textureLoad(src_pos, c, 0);
    if (p.a > 0.25) {
        out.pos = p;
        out.nrm = textureLoad(src_nrm, c, 0);
        out.tan = textureLoad(src_tan, c, 0);
        return out;
    }
    // Copy one neighbor instead of averaging: a texel between two islands
    // would otherwise get the midpoint of two unrelated surface points, which
    // usually lies inside the mesh (black AO seams under bilinear filtering).
    // Edge neighbors win over corner neighbors.
    let dims = vec2<i32>(textureDimensions(src_pos));
    let offsets = array<vec2<i32>, 8>(
        vec2<i32>(1, 0), vec2<i32>(-1, 0), vec2<i32>(0, 1), vec2<i32>(0, -1),
        vec2<i32>(1, 1), vec2<i32>(-1, 1), vec2<i32>(1, -1), vec2<i32>(-1, -1),
    );
    for (var i = 0; i < 8; i++) {
        let q = clamp(c + offsets[i], vec2<i32>(0), dims - 1);
        let s = textureLoad(src_pos, q, 0);
        if (s.a > 0.25) {
            out.pos = vec4<f32>(s.xyz, 0.5);
            out.nrm = textureLoad(src_nrm, q, 0);
            out.tan = textureLoad(src_tan, q, 0);
            return out;
        }
    }
    out.pos = vec4<f32>(0.0);
    out.nrm = vec4<f32>(0.0);
    out.tan = vec4<f32>(0.0);
    return out;
}
