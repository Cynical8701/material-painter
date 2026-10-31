// One step of a layer's mask filter stack (engine::MaskFilter), applied to
// the rendered mask: levels, invert, contrast, blur (one direction per
// pass), sharpen, warp and noise breakup.

struct FilterU {
    p: vec4<f32>,   // kind, a, b, c (see FilterKind)
    q: vec4<f32>,   // blur direction x, y, texels per reference texel, unused
};

@group(0) @binding(0) var<uniform> f: FilterU;
@group(0) @binding(1) var src: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;
// A mask layer's own values, for the combine step.
@group(0) @binding(3) var layer_tex: texture_2d<f32>;

fn at(c: vec2<i32>) -> f32 {
    let dims = vec2<i32>(textureDimensions(src));
    return textureLoad(src, clamp(c, vec2<i32>(0), dims - 1), 0).r;
}

@fragment
fn fs_filter(in: FsOut) -> @location(0) vec4<f32> {
    let c = vec2<i32>(in.pos.xy);
    let dims = vec2<f32>(textureDimensions(src));
    let uv = in.pos.xy / dims;
    let s = at(c);
    let a = f.p.y;
    let b = f.p.z;
    var v = s;
    switch i32(f.p.x + 0.5) {
        // Levels: black, white, gamma (above 1 brightens).
        case 0: {
            v = pow(clamp((s - a) / max(b - a, 1e-4), 0.0, 1.0), 1.0 / max(f.p.w, 1e-3));
        }
        case 1: {
            v = 1.0 - s;
        }
        // Contrast around a position: a histogram scan.
        case 2: {
            let w = max((1.0 - b) * 0.5, 1e-4);
            v = smoothstep(a - w, a + w, s);
        }
        // Gaussian blur along one direction, radius a (reference texels).
        case 3: {
            let r = a * f.q.z;
            let n = min(i32(ceil(r)), 64);
            let sigma = max(r * 0.5, 0.5);
            let dir = vec2<i32>(i32(f.q.x), i32(f.q.y));
            var sum = 0.0;
            var wsum = 0.0;
            for (var k = -n; k <= n; k++) {
                let w = exp(-f32(k * k) / (2.0 * sigma * sigma));
                sum += at(c + dir * k) * w;
                wsum += w;
            }
            v = sum / wsum;
        }
        // Sharpen: amount a over a ring of radius b.
        case 4: {
            let r = i32(max(round(b * f.q.z), 1.0));
            let ring = (at(c + vec2<i32>(r, 0)) + at(c - vec2<i32>(r, 0)) + at(c + vec2<i32>(0, r)) + at(c - vec2<i32>(0, r))) * 0.25;
            v = s + a * (s - ring);
        }
        // Warp: pushed around by noise, intensity a, scale b.
        case 5: {
            let o = vec2<f32>(fbm(vec3<f32>(uv * b, 1.3)), fbm(vec3<f32>(uv * b + vec2<f32>(5.2, 1.7), 7.1))) - 0.5;
            v = textureSampleLevel(src, samp, uv + o * a * 0.2, 0.0).r;
        }
        // Combine a mask layer (scaled by c) over the mask: blend mode a, opacity b.
        case 7: {
            let l = textureLoad(layer_tex, c, 0).r * f.p.w;
            var r: f32;
            switch i32(a + 0.5) {
                case 1: { r = s * l; }
                case 2: { r = s + l; }
                case 3: { r = s - l; }
                case 4: { r = max(s, l); }
                case 5: { r = min(s, l); }
                default: { r = l; }
            }
            v = mix(s, r, clamp(b, 0.0, 1.0));
        }
        // Noise breakup: amount a, scale b, seed c.
        default: {
            let n = smoothstep(0.35, 0.65, fbm(vec3<f32>(uv * b, f.p.w * 1.37)));
            v = s * (1.0 - a * (1.0 - n));
        }
    }
    return vec4<f32>(clamp(v, 0.0, 1.0), 0.0, 0.0, 1.0);
}
