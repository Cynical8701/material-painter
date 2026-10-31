// Bloom: a downsample chain (soft threshold and firefly-safe averaging on the
// first step) followed by tent-filtered upsamples added back up the chain.

struct BloomU {
    p: vec4<f32>,  // source texel size xy, threshold, first pass (1) or not (0)
};

@group(0) @binding(0) var<uniform> bu: BloomU;
@group(0) @binding(1) var bsrc: texture_2d<f32>;
@group(0) @binding(2) var bsamp: sampler;

fn luma(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

// Keeps what is above the threshold, with a soft knee below it.
fn bright(c: vec3<f32>) -> vec3<f32> {
    let thr = bu.p.z;
    let knee = thr * 0.5 + 1e-4;
    let l = luma(c);
    var soft = clamp(l - thr + knee, 0.0, 2.0 * knee);
    soft = soft * soft / (4.0 * knee);
    return c * max(soft, l - thr) / max(l, 1e-4);
}

@fragment
fn fs_down(in: FsOut) -> @location(0) vec4<f32> {
    let t = bu.p.xy;
    // Four bilinear taps cover a 4x4 texel footprint.
    var taps = array<vec3<f32>, 4>(
        textureSampleLevel(bsrc, bsamp, in.uv + t * vec2<f32>(-1.0, -1.0), 0.0).rgb,
        textureSampleLevel(bsrc, bsamp, in.uv + t * vec2<f32>(1.0, -1.0), 0.0).rgb,
        textureSampleLevel(bsrc, bsamp, in.uv + t * vec2<f32>(-1.0, 1.0), 0.0).rgb,
        textureSampleLevel(bsrc, bsamp, in.uv + t * vec2<f32>(1.0, 1.0), 0.0).rgb,
    );
    var sum = vec3<f32>(0.0);
    var wsum = 0.0;
    for (var i = 0; i < 4; i++) {
        var c = taps[i];
        var w = 1.0;
        if (bu.p.w > 0.5) {
            c = bright(c);
            // Weighting by inverse brightness stops single hot pixels from flickering.
            w = 1.0 / (1.0 + luma(c));
        }
        sum += c * w;
        wsum += w;
    }
    return vec4<f32>(sum / wsum, 1.0);
}

@fragment
fn fs_up(in: FsOut) -> @location(0) vec4<f32> {
    let t = bu.p.xy;
    var c = textureSampleLevel(bsrc, bsamp, in.uv, 0.0).rgb * 4.0;
    c += textureSampleLevel(bsrc, bsamp, in.uv + vec2<f32>(t.x, 0.0), 0.0).rgb * 2.0;
    c += textureSampleLevel(bsrc, bsamp, in.uv - vec2<f32>(t.x, 0.0), 0.0).rgb * 2.0;
    c += textureSampleLevel(bsrc, bsamp, in.uv + vec2<f32>(0.0, t.y), 0.0).rgb * 2.0;
    c += textureSampleLevel(bsrc, bsamp, in.uv - vec2<f32>(0.0, t.y), 0.0).rgb * 2.0;
    c += textureSampleLevel(bsrc, bsamp, in.uv + t, 0.0).rgb;
    c += textureSampleLevel(bsrc, bsamp, in.uv - t, 0.0).rgb;
    c += textureSampleLevel(bsrc, bsamp, in.uv + vec2<f32>(t.x, -t.y), 0.0).rgb;
    c += textureSampleLevel(bsrc, bsamp, in.uv + vec2<f32>(-t.x, t.y), 0.0).rgb;
    return vec4<f32>(c / 16.0, 1.0);
}
