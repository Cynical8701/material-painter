// Final viewport pass: HDR scene in, display image out. Lens effects and
// exposure work on scene-linear light; contrast, saturation, vignette and
// grain work on the tone-mapped image.

struct PostU {
    a: vec4<f32>,  // exposure, tone mapper, contrast, saturation
    b: vec4<f32>,  // temperature, tint, vignette, bloom strength
    c: vec4<f32>,  // chromatic aberration, grain, sharpen, raw (1 = pass through)
    d: vec4<f32>,  // texel size xy, aspect, unused
};

@group(0) @binding(0) var<uniform> pu: PostU;
@group(0) @binding(1) var hdr: texture_2d<f32>;
@group(0) @binding(2) var bloom_tex: texture_2d<f32>;
@group(0) @binding(3) var psamp: sampler;

fn luminance(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

// Narkowicz's fit of the ACES filmic curve.
fn tm_aces(x: vec3<f32>) -> vec3<f32> {
    return clamp((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14), vec3<f32>(0.0), vec3<f32>(1.0));
}

// Log-encoded sigmoid in the spirit of AgX: highlights desaturate toward
// white instead of clipping to pure hues.
fn tm_agx(c: vec3<f32>) -> vec3<f32> {
    let l = luminance(c);
    // Pull channels slightly together so bright saturated colors roll off.
    let inset = mix(vec3<f32>(l), c, 0.85);
    let lo = -10.0;
    let hi = 6.5;
    let x = clamp((log2(max(inset, vec3<f32>(1e-10))) - lo) / (hi - lo), vec3<f32>(0.0), vec3<f32>(1.0));
    let xp = pow(x, vec3<f32>(1.7));
    let y = xp / (xp + pow(1.0 - x, vec3<f32>(1.7)));
    // Give back some of the saturation the inset took.
    let yl = luminance(y);
    let out = max(mix(vec3<f32>(yl), y, 1.12), vec3<f32>(0.0));
    return pow(out, vec3<f32>(2.2));
}

// Khronos PBR Neutral: keeps base colors faithful, compresses only highlights.
fn tm_neutral(c_in: vec3<f32>) -> vec3<f32> {
    let start = 0.8 - 0.04;
    let desat = 0.15;
    var c = c_in;
    let x = min(c.r, min(c.g, c.b));
    let offset = select(0.04, x - 6.25 * x * x, x < 0.08);
    c -= offset;
    let peak = max(c.r, max(c.g, c.b));
    if (peak < start) {
        return c;
    }
    let d = 1.0 - start;
    let new_peak = 1.0 - d * d / (peak + d - start);
    c *= new_peak / peak;
    let g = 1.0 - 1.0 / (desat * (peak - new_peak) + 1.0);
    return mix(c, vec3<f32>(new_peak), g);
}

// Hable's filmic curve.
fn hable(x: vec3<f32>) -> vec3<f32> {
    let a = 0.15;
    let b = 0.50;
    let c = 0.10;
    let d = 0.20;
    let e = 0.02;
    let f = 0.30;
    return ((x * (a * x + c * b) + d * e) / (x * (a * x + b) + d * f)) - e / f;
}

fn tm_filmic(c: vec3<f32>) -> vec3<f32> {
    let white = hable(vec3<f32>(11.2));
    return clamp(hable(c * 2.0) / white, vec3<f32>(0.0), vec3<f32>(1.0));
}

fn tonemap(c: vec3<f32>) -> vec3<f32> {
    switch i32(pu.a.y) {
        case 0: { return tm_aces(c); }
        case 1: { return tm_agx(c); }
        case 2: { return clamp(tm_neutral(c), vec3<f32>(0.0), vec3<f32>(1.0)); }
        case 3: { return tm_filmic(c); }
        default: { return clamp(c, vec3<f32>(0.0), vec3<f32>(1.0)); }
    }
}

fn hash2(p: vec2<f32>) -> f32 {
    let u = pcg3(bitcast<vec3<u32>>(vec3<i32>(vec3<f32>(p, 17.0))));
    return f32(u.x) * (1.0 / 4294967295.0);
}

@fragment
fn fs_post(in: FsOut) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let base = textureSampleLevel(hdr, psamp, uv, 0.0);
    if (pu.c.w > 0.5) {
        return base;
    }
    let t = pu.d.xy;
    // Work on straight color; edges against a transparent background arrive premultiplied.
    let inv_a = select(0.0, 1.0 / base.a, base.a > 1e-4);
    var c = base.rgb * inv_a;

    // Chromatic aberration: red and blue pushed apart toward the edges.
    let ca = pu.c.x;
    if (ca > 0.0) {
        let off = (uv - 0.5) * ca * 0.012;
        c.r = textureSampleLevel(hdr, psamp, uv + off, 0.0).r * inv_a;
        c.b = textureSampleLevel(hdr, psamp, uv - off, 0.0).b * inv_a;
    }
    // Unsharp mask against the four neighbors.
    let sharpen = pu.c.z;
    if (sharpen > 0.0) {
        let n = textureSampleLevel(hdr, psamp, uv + vec2<f32>(t.x, 0.0), 0.0).rgb
            + textureSampleLevel(hdr, psamp, uv - vec2<f32>(t.x, 0.0), 0.0).rgb
            + textureSampleLevel(hdr, psamp, uv + vec2<f32>(0.0, t.y), 0.0).rgb
            + textureSampleLevel(hdr, psamp, uv - vec2<f32>(0.0, t.y), 0.0).rgb;
        c = max(c + (c - n * 0.25 * inv_a) * sharpen, vec3<f32>(0.0));
    }
    c += textureSampleLevel(bloom_tex, psamp, uv, 0.0).rgb * pu.b.w;
    c *= pu.a.x;

    // White balance: temperature along blue-amber, tint along green-magenta.
    let temp = pu.b.x;
    let tint = pu.b.y;
    c *= vec3<f32>(1.0 + 0.25 * temp, 1.0 - 0.2 * tint, 1.0 - 0.25 * temp);

    c = tonemap(max(c, vec3<f32>(0.0)));

    // Grading in a perceptual space.
    var p = pow(c, vec3<f32>(1.0 / 2.2));
    p = (p - 0.5) * pu.a.z + 0.5;
    let l = luminance(p);
    p = mix(vec3<f32>(l), p, pu.a.w);
    let v = (uv - 0.5) * vec2<f32>(pu.d.z, 1.0);
    p *= 1.0 - pu.b.z * smoothstep(0.25, 0.95, length(v) * 1.3);
    let grain = pu.c.y;
    if (grain > 0.0) {
        p += (hash2(in.pos.xy) - 0.5) * grain * 0.12;
    }
    c = pow(clamp(p, vec3<f32>(0.0), vec3<f32>(1.0)), vec3<f32>(2.2));
    // Premultiplied out, like the scene came in.
    return vec4<f32>(c * base.a, base.a);
}
