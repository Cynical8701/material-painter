// Environment prefiltering: a box mip chain of the source HDRI, then one
// radiance mip per roughness, GGX-convolved with filtered importance sampling
// (each sample reads the source mip matching its lobe footprint).

struct P {
    size: vec4<f32>,   // output width, height, roughness, source width
    extra: vec4<f32>,  // source mip count, sample count
};

@group(0) @binding(0) var<uniform> p: P;
@group(0) @binding(1) var src: texture_2d<f32>;
@group(0) @binding(2) var src_samp: sampler;
@group(0) @binding(3) var dst: texture_storage_2d<rgba16float, write>;

@compute @workgroup_size(8, 8)
fn cs_downsample(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (f32(gid.x) >= p.size.x || f32(gid.y) >= p.size.y) {
        return;
    }
    let s = vec2<i32>(gid.xy) * 2;
    let c = textureLoad(src, s, 0) + textureLoad(src, s + vec2<i32>(1, 0), 0)
        + textureLoad(src, s + vec2<i32>(0, 1), 0) + textureLoad(src, s + vec2<i32>(1, 1), 0);
    textureStore(dst, vec2<i32>(gid.xy), c * 0.25);
}

fn radical_inverse(i: u32) -> f32 {
    var b = (i << 16u) | (i >> 16u);
    b = ((b & 0x55555555u) << 1u) | ((b & 0xAAAAAAAAu) >> 1u);
    b = ((b & 0x33333333u) << 2u) | ((b & 0xCCCCCCCCu) >> 2u);
    b = ((b & 0x0F0F0F0Fu) << 4u) | ((b & 0xF0F0F0F0u) >> 4u);
    b = ((b & 0x00FF00FFu) << 8u) | ((b & 0xFF00FF00u) >> 8u);
    return f32(b) * 2.3283064365386963e-10;
}

@compute @workgroup_size(8, 8)
fn cs_prefilter(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (f32(gid.x) >= p.size.x || f32(gid.y) >= p.size.y) {
        return;
    }
    let uv = (vec2<f32>(gid.xy) + 0.5) / p.size.xy;
    let rough = p.size.z;
    if (rough <= 0.0) {
        textureStore(dst, vec2<i32>(gid.xy), vec4<f32>(textureSampleLevel(src, src_samp, uv, 0.0).rgb, 1.0));
        return;
    }
    // Reflection lobe around n, with the usual n = v = r assumption.
    let n = env_uv_to_dir(uv);
    let up = select(vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(0.0, 1.0, 0.0), abs(n.y) < 0.999);
    let t = normalize(cross(up, n));
    let b = cross(n, t);
    let a = rough * rough;
    let a2 = a * a;
    let count = u32(p.extra.y);
    let src_w = p.size.w;
    let texel_sa = 4.0 * 3.14159265 / (src_w * src_w * 0.5);
    let max_lod = p.extra.x - 1.0;
    var acc = vec3<f32>(0.0);
    var weight = 0.0;
    for (var i = 0u; i < count; i++) {
        let xi = vec2<f32>((f32(i) + 0.5) / f32(count), radical_inverse(i));
        let phi = 6.28318531 * xi.y;
        let ct = sqrt((1.0 - xi.x) / (1.0 + (a2 - 1.0) * xi.x));
        let st = sqrt(max(1.0 - ct * ct, 0.0));
        let h = t * (st * cos(phi)) + b * (st * sin(phi)) + n * ct;
        let l = 2.0 * dot(n, h) * h - n;
        let nl = dot(n, l);
        if (nl > 0.0) {
            let d = (ct * ct * (a2 - 1.0) + 1.0);
            let ggx = a2 / (3.14159265 * d * d);
            // pdf of l is D * (n.h) / (4 v.h); with n = v, that is D / 4.
            let pdf = ggx * 0.25;
            let sample_sa = 1.0 / (f32(count) * pdf + 1e-6);
            let lod = clamp(0.5 * log2(sample_sa / texel_sa) + 1.0, 0.0, max_lod);
            acc += textureSampleLevel(src, src_samp, env_dir_to_uv(l), lod).rgb * nl;
            weight += nl;
        }
    }
    textureStore(dst, vec2<i32>(gid.xy), vec4<f32>(acc / max(weight, 1e-6), 1.0));
}
