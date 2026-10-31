// Applies per-map levels and painted bake fixes to one packed mesh-map texture.
// Each component has: levels (black, white, gamma, has_fix) and a kind
// (0 = scalar, 1 = normal component: a fix flattens it, 2 = passthrough).
// Fix textures hold (value, coverage).

struct ResolveU {
    levels: array<vec4<f32>, 4>,
    kind: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: ResolveU;
@group(0) @binding(1) var raw: texture_2d<f32>;
@group(0) @binding(2) var fix0: texture_2d<f32>;
@group(0) @binding(3) var fix1: texture_2d<f32>;
@group(0) @binding(4) var fix2: texture_2d<f32>;
@group(0) @binding(5) var fix3: texture_2d<f32>;
@group(0) @binding(6) var samp: sampler;

fn resolve(v: f32, lv: vec4<f32>, kind: f32, fix: vec2<f32>) -> f32 {
    var r = v;
    if (kind < 0.5) {
        // Gamma above 1 brightens the midtones, like Photoshop / Substance levels.
        r = pow(clamp((v - lv.x) / max(lv.y - lv.x, 1e-4), 0.0, 1.0), 1.0 / max(lv.z, 1e-3));
        if (lv.w > 0.5) {
            r = mix(r, fix.x, fix.y);
        }
    } else if (kind < 1.5) {
        if (lv.w > 0.5) {
            r = mix(r, 0.0, fix.y);
        }
    }
    return r;
}

@fragment
fn fs_resolve(in: FsOut) -> @location(0) vec4<f32> {
    let c = vec2<i32>(in.pos.xy);
    let v = textureSampleLevel(raw, samp, in.uv, 0.0);
    var f = array<vec2<f32>, 4>(vec2<f32>(0.0), vec2<f32>(0.0), vec2<f32>(0.0), vec2<f32>(0.0));
    if (u.levels[0].w > 0.5) {
        f[0] = textureLoad(fix0, c, 0).rg;
    }
    if (u.levels[1].w > 0.5) {
        f[1] = textureLoad(fix1, c, 0).rg;
    }
    if (u.levels[2].w > 0.5) {
        f[2] = textureLoad(fix2, c, 0).rg;
    }
    if (u.levels[3].w > 0.5) {
        f[3] = textureLoad(fix3, c, 0).rg;
    }
    return vec4<f32>(
        resolve(v.x, u.levels[0], u.kind.x, f[0]),
        resolve(v.y, u.levels[1], u.kind.y, f[1]),
        resolve(v.z, u.levels[2], u.kind.z, f[2]),
        resolve(v.w, u.levels[3], u.kind.w, f[3])
    );
}
