// Writes the current stroke into a paint layer (or mask) using the snapshot
// taken at stroke start as the base, so stroke opacity caps the build-up.
// Paint channels store (value, alpha) pairs; color stores rgb + alpha.

struct ApplyU {
    color: vec4<f32>,   // sRGB-encoded rgb
    rmh: vec4<f32>,     // roughness, metallic, height, mask value
    ch: vec4<f32>,      // channel enabled flags
    p: vec4<f32>,       // stroke opacity, erase, unused, unused
    g: vec4<f32>,       // grain strength (0 off), tiling, in uv (else tri-planar world), invert
};

@group(0) @binding(0) var<uniform> u: ApplyU;
@group(0) @binding(1) var stroke: texture_2d<f32>;
@group(0) @binding(2) var b_color: texture_2d<f32>;
@group(0) @binding(3) var b_rough: texture_2d<f32>;
@group(0) @binding(4) var b_metal: texture_2d<f32>;
@group(0) @binding(5) var b_height: texture_2d<f32>;
@group(0) @binding(6) var pos_map: texture_2d<f32>;
@group(0) @binding(7) var nrm_map: texture_2d<f32>;
@group(0) @binding(8) var grain_tex: texture_2d<f32>;
@group(0) @binding(9) var grain_samp: sampler;

fn grain_at(q: vec2<f32>) -> f32 {
    return textureSampleLevel(grain_tex, grain_samp, q, 0.0).r;
}

// Brush grain: a texture fixed to the surface that caps how much paint each
// texel takes, so strokes build up a broken, textured look (scratchy, dusty,
// chipped) instead of a flat one. Tri-planar in world space, or in UV space
// for the UV projection.
fn grain(c: vec2<i32>) -> f32 {
    if (u.g.x <= 0.0) {
        return 1.0;
    }
    var v: f32;
    if (u.g.z > 0.5) {
        let uv = (vec2<f32>(c) + 0.5) / vec2<f32>(textureDimensions(stroke));
        v = grain_at(uv * u.g.y);
    } else {
        let p = textureLoad(pos_map, c, 0).xyz * u.g.y;
        let n = textureLoad(nrm_map, c, 0).xyz * 2.0 - 1.0;
        var w = pow(abs(n), vec3<f32>(4.0));
        w /= max(w.x + w.y + w.z, 1e-4);
        v = grain_at(vec2<f32>(p.z, -p.y)) * w.x + grain_at(p.xz) * w.y + grain_at(vec2<f32>(p.x, -p.y)) * w.z;
    }
    if (u.g.w > 0.5) {
        v = 1.0 - v;
    }
    return mix(1.0, v, u.g.x);
}

fn over(base: vec2<f32>, v: f32, m: f32, erase: bool) -> vec2<f32> {
    if (erase) {
        return vec2<f32>(base.x, base.y * (1.0 - m));
    }
    let a = m + base.y * (1.0 - m);
    if (a < 1e-5) {
        return vec2<f32>(v, 0.0);
    }
    return vec2<f32>((v * m + base.x * base.y * (1.0 - m)) / a, a);
}

struct PaintOut {
    @location(0) color: vec4<f32>,
    @location(1) rough: vec4<f32>,
    @location(2) metal: vec4<f32>,
    @location(3) height: vec4<f32>,
};

@fragment
fn fs_paint(in: FsOut) -> PaintOut {
    let c = vec2<i32>(in.pos.xy);
    let m = textureLoad(stroke, c, 0).r * u.p.x * grain(c);
    let erase = u.p.y > 0.5;
    var out: PaintOut;
    if (u.ch.x > 0.5) {
        let b = textureLoad(b_color, c, 0);
        var rgb = b.rgb;
        var a: f32;
        if (erase) {
            a = b.a * (1.0 - m);
        } else {
            a = m + b.a * (1.0 - m);
            if (a > 1e-5) {
                rgb = (u.color.rgb * m + b.rgb * b.a * (1.0 - m)) / a;
            } else {
                rgb = u.color.rgb;
            }
        }
        out.color = vec4<f32>(rgb, a);
    }
    if (u.ch.y > 0.5) {
        out.rough = vec4<f32>(over(textureLoad(b_rough, c, 0).rg, u.rmh.x, m, erase), 0.0, 0.0);
    }
    if (u.ch.z > 0.5) {
        out.metal = vec4<f32>(over(textureLoad(b_metal, c, 0).rg, u.rmh.y, m, erase), 0.0, 0.0);
    }
    if (u.ch.w > 0.5) {
        out.height = vec4<f32>(over(textureLoad(b_height, c, 0).rg, u.rmh.z, m, erase), 0.0, 0.0);
    }
    return out;
}

@fragment
fn fs_mask(in: FsOut) -> @location(0) vec4<f32> {
    let c = vec2<i32>(in.pos.xy);
    let m = textureLoad(stroke, c, 0).r * u.p.x * grain(c);
    let base = textureLoad(b_color, c, 0).r;
    let target_value = select(u.rmh.w, 0.0, u.p.y > 0.5);
    return vec4<f32>(mix(base, target_value, m), 0.0, 0.0, 1.0);
}
