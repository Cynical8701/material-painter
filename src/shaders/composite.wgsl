// Layer stack compositing. One pass per visible layer, ping-ponging between
// two accumulation sets: reads the stack below, blends this layer on top.
// Fill layers and smart masks are evaluated procedurally here from the baked
// mesh maps, so they cost no texture memory.

struct LayerU {
    fill_color: vec4<f32>,  // sRGB rgb
    fill_rmh: vec4<f32>,    // roughness, metallic, height
    ch: vec4<f32>,          // per-channel enable (and allocated, for paint layers)
    p0: vec4<f32>,          // opacity, blend mode, kind (0 paint, 1 fill), has paint mask
    var0: vec4<f32>,        // color variation, variation scale, roughness variation, height variation
    var1: vec4<f32>,        // variation seed, per-part variation, projection (engine::Projection order), image channel bits
    bmin: vec4<f32>,
    bsize: vec4<f32>,
    gw: array<vec4<f32>, 5>, // smart mask input weights (see engine::GEN_INPUTS)
    gc: vec4<f32>,          // custom map inputs: index among the baked custom maps (-1 none)
    gcw: vec4<f32>,         // custom map inputs: weights
    gp0: vec4<f32>,         // level, contrast, scale, noise
    gp1: vec4<f32>,         // invert, seed, has generator, edge width
    gid: vec4<f32>,         // ID color, tolerance
    gx: vec4<f32>,          // baked edge max distance, unused, tri-planar sharpness
    img: vec4<f32>,         // image tiling, rotation (degrees), offset x, offset y
};

@group(0) @binding(0) var<uniform> u: LayerU;
@group(0) @binding(1) var prev_color: texture_2d<f32>;
@group(0) @binding(2) var prev_rough: texture_2d<f32>;
@group(0) @binding(3) var prev_metal: texture_2d<f32>;
@group(0) @binding(4) var prev_height: texture_2d<f32>;
@group(0) @binding(5) var l_color: texture_2d<f32>;
@group(0) @binding(6) var l_rough: texture_2d<f32>;
@group(0) @binding(7) var l_metal: texture_2d<f32>;
@group(0) @binding(8) var l_height: texture_2d<f32>;
@group(0) @binding(9) var l_mask: texture_2d<f32>;
@group(0) @binding(10) var pos_map: texture_2d<f32>;
@group(0) @binding(11) var nrm_map: texture_2d<f32>;
@group(0) @binding(12) var map_a: texture_2d<f32>;  // AO, curvature, cavity, thickness
@group(0) @binding(13) var map_b: texture_2d<f32>;  // sky, edge dist, crease dist, part random
@group(0) @binding(14) var map_c: texture_2d<f32>;  // bent normal, part gradient
@group(0) @binding(15) var custom_maps: texture_2d_array<f32>;  // custom map i: channel i % 4 of layer i / 4
@group(0) @binding(16) var map_e: texture_2d<f32>;  // ID color
@group(0) @binding(17) var samp: sampler;
@group(0) @binding(18) var rsamp: sampler;  // tiling, mipmapped: fill images

fn blend3(mode: i32, a: vec3<f32>, b: vec3<f32>) -> vec3<f32> {
    switch mode {
        case 1: { return a * b; }
        case 2: { return 1.0 - (1.0 - a) * (1.0 - b); }
        case 3: { return select(1.0 - 2.0 * (1.0 - a) * (1.0 - b), 2.0 * a * b, a < vec3<f32>(0.5)); }
        case 4: { return a + b; }
        case 5: { return a - b; }
        case 6: { return min(a, b); }
        case 7: { return max(a, b); }
        default: { return b; }
    }
}

fn blend1(mode: i32, a: f32, b: f32) -> f32 {
    return blend3(mode, vec3<f32>(a), vec3<f32>(b)).x;
}

fn weight(i: i32) -> f32 {
    var w = u.gw;
    return w[i / 4][i % 4];
}

fn rot2(q: vec2<f32>, deg: f32) -> vec2<f32> {
    let a = radians(deg);
    let c = cos(a);
    let s = sin(a);
    return vec2<f32>(q.x * c - q.y * s, q.x * s + q.y * c);
}

// A fill image sampled with the layer's projection. The mip level comes from
// how many image texels one output texel covers (`texel_world` is the world
// size of an output texel), since derivatives aren't available here.
fn fill_image(t: texture_2d<f32>, uv: vec2<f32>, p: vec3<f32>, n: vec3<f32>, texel_world: f32) -> vec4<f32> {
    let size = f32(textureDimensions(t).x);
    let tiling = max(u.img.x, 1e-3);
    let mode = i32(u.var1.z + 0.5);
    if (mode == 0) {
        let q = rot2(uv - 0.5, u.img.y) * tiling + 0.5 + u.img.zw;
        let lod = log2(max(tiling * size / f32(textureDimensions(prev_color).x), 1.0));
        return textureSampleLevel(t, rsamp, q, lod);
    }
    if (mode >= 2) {
        // Planar, spherical and cylindrical projections span the mesh's
        // bounds once at tiling 1.
        let ext = max(max(u.bsize.x, u.bsize.y), max(u.bsize.z, 1e-4));
        let d = (p - (u.bmin.xyz + u.bsize.xyz * 0.5)) / ext;
        var c: vec2<f32>;
        switch mode {
            case 2: { c = vec2<f32>(-d.z, -d.y) + 0.5; }
            case 3: { c = vec2<f32>(d.x, d.z) + 0.5; }
            case 4: { c = vec2<f32>(d.x, -d.y) + 0.5; }
            case 5: {
                // Longitude around the vertical axis, latitude from the top.
                let r = max(length(d), 1e-6);
                c = vec2<f32>(atan2(d.x, d.z) / 6.2831853 + 0.5, acos(clamp(d.y / r, -1.0, 1.0)) / 3.1415927);
            }
            default: { c = vec2<f32>(atan2(d.x, d.z) / 6.2831853 + 0.5, 0.5 - d.y); }
        }
        let q = rot2(c - 0.5, u.img.y) * tiling + 0.5 + u.img.zw;
        let lod = log2(max(texel_world * tiling * size / ext, 1.0));
        return textureSampleLevel(t, rsamp, q, lod);
    }
    let lod = log2(max(texel_world * tiling * size, 1.0));
    var w = pow(abs(n), vec3<f32>(max(u.gx.z, 1.0)));
    w /= max(w.x + w.y + w.z, 1e-4);
    let q = p * tiling;
    return textureSampleLevel(t, rsamp, rot2(vec2<f32>(q.z, -q.y), u.img.y) + u.img.zw, lod) * w.x
        + textureSampleLevel(t, rsamp, rot2(q.xz, u.img.y) + u.img.zw, lod) * w.y
        + textureSampleLevel(t, rsamp, rot2(vec2<f32>(q.x, -q.y), u.img.y) + u.img.zw, lod) * w.z;
}

// Wood grain: rings and fibers running along the object's long axis, or
// vertically on faces that look down that axis (the ends of planks).
fn wood_grain(p: vec3<f32>, n: vec3<f32>, scale: f32) -> f32 {
    let size = u.bsize.xyz;
    var ax = 0;
    if (size.y > size.x && size.y >= size.z) {
        ax = 1;
    } else if (size.z > size.x) {
        ax = 2;
    }
    if (abs(n[ax]) > 0.7) {
        ax = select(1, 0, ax == 1);
    }
    let a = p[ax];
    let b = p[(ax + 1) % 3];
    let c = p[(ax + 2) % 3];
    let seed = vec3<f32>(u.gp1.y * 17.31);
    let warp = fbm(vec3<f32>(a * 0.35, b * 1.5, c * 1.5) * scale + seed);
    let r = (b + c) * scale * 2.5 + warp * 5.0;
    let ring = smoothstep(0.6, 1.0, abs(fract(r) * 2.0 - 1.0));
    let fiber = fbm(vec3<f32>(a * 0.5, b * 30.0, c * 30.0) * scale + seed + vec3<f32>(3.7));
    return clamp(ring * 0.7 + smoothstep(0.35, 0.75, fiber) * 0.45, 0.0, 1.0);
}

// Scratches on a plane: some grid cells hold one stroke, slightly bent,
// tapered at both ends, each with its own width and depth. `aa` is the
// texel footprint in q units: strokes thinner than a texel fade instead
// of breaking into stair-steps.
fn scratch_plane(q: vec2<f32>, seed: f32, aa: f32) -> f32 {
    let cell = floor(q);
    var m = 0.0;
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let c = cell + vec2<f32>(f32(x), f32(y));
            let h = hash33(vec3<f32>(c, seed));
            if (h.z < 0.55) {
                continue;
            }
            let h2 = hash33(vec3<f32>(c + vec2<f32>(17.0, 31.0), seed + 5.0));
            let ang = h2.x * 3.14159265;
            let dir = vec2<f32>(cos(ang), sin(ang));
            let side = vec2<f32>(-dir.y, dir.x);
            let half_len = 0.35 + 1.25 * h.x * h.x;
            let rel = q - (c + h.xy);
            let t = dot(rel, dir);
            if (abs(t) > half_len) {
                continue;
            }
            let s = t / half_len;
            // A gentle arc, so strokes aren't ruler-straight.
            let bend = (h2.y - 0.5) * 0.25 * half_len;
            let d = abs(dot(rel, side) - bend * s * s);
            let width = 0.006 + 0.03 * h2.z * h2.z;
            let taper = 1.0 - smoothstep(0.3, 1.0, abs(s));
            let w = width * taper;
            // The falloff spans at least a texel, so a thin diagonal stroke
            // always reaches a texel center (no dashes); thin strokes are
            // fainter instead of thinner than a texel.
            let edge = max(aa, 1e-4);
            let line = clamp(1.0 - d / (w + edge), 0.0, 1.0);
            let strength = 0.3 + 0.7 * fract(h2.x * 13.7 + h.y * 7.1);
            m = max(m, line * line * strength * min(w / edge + 0.35, 1.0));
        }
    }
    return m;
}

// Scratches projected from the three axes and blended by the normal, so
// every face gets clean strokes: a set of bold ones and finer hairlines.
fn scratches(p: vec3<f32>, n: vec3<f32>, scale: f32, texel_world: f32) -> f32 {
    var w = pow(abs(n), vec3<f32>(4.0));
    w /= max(w.x + w.y + w.z, 1e-4);
    let k = scale * 5.0;
    let q = p * k;
    let seed = u.gp1.y * 3.7;
    let aa = texel_world * k;
    var m = 0.0;
    for (var i = 0; i < 3; i++) {
        if (w[i] < 0.01) {
            continue;
        }
        let a = select(select(q.xy, q.xz, i == 1), q.yz, i == 0);
        let bold = scratch_plane(a, seed + f32(i), aa);
        let fine = scratch_plane(a * 2.7 + vec2<f32>(3.1, 7.7), seed + f32(i) + 11.0, aa * 2.7) * 0.55;
        m += w[i] * max(bold, fine);
    }
    return m;
}

// Grunge for breaking up mask edges: broad blotches with crisp, gritty
// detail, so worn and dirty areas end in irregular borders instead of
// soft gradients.
fn grunge(q: vec3<f32>) -> f32 {
    let broad = fbm(q);
    let grit = fbm(q * 5.3 + vec3<f32>(4.1, 1.3, 7.9));
    let speck = vnoise(q * 23.0 + vec3<f32>(9.4, 2.2, 5.6));
    return broad * 0.6 + grit * 0.3 + speck * 0.1;
}

// World size of one output texel at `c` (on the surface point `p`).
fn texel_size(c: vec2<i32>, p: vec3<f32>) -> f32 {
    let right = textureLoad(pos_map, c + vec2<i32>(1, 0), 0);
    return select(2.0 / f32(textureDimensions(pos_map).x), distance(right.xyz, p), right.a > 0.5);
}

// Smart mask: weighted sum of mesh-map inputs, noise breakup, then levels.
fn generator(p: vec3<f32>, n: vec3<f32>, uv: vec2<f32>, texel_world: f32) -> f32 {
    let A = textureSampleLevel(map_a, samp, uv, 0.0);
    let B = textureSampleLevel(map_b, samp, uv, 0.0);
    let C = textureSampleLevel(map_c, samp, uv, 0.0);
    let E = textureSampleLevel(map_e, samp, uv, 0.0);
    let scale = u.gp0.z;
    let q = p * scale + vec3<f32>(u.gp1.y * 17.31);
    let curv = (A.g - 0.5) * 2.0;
    let width = max(u.gp1.w, 1e-4);
    let emax = u.gx.x;

    // Thin parts (rods, rims, belts) are rounded all the way around, so edge
    // and convexity inputs would wear them completely; damp them there.
    let thick = mix(0.15, 1.0, smoothstep(0.15, 0.75, A.a));
    var inp: array<f32, 19>;
    inp[0] = (1.0 - smoothstep(0.0, width, B.g * emax)) * thick;
    inp[1] = max(curv, 0.0) * thick;
    inp[2] = max(-curv, 0.0);
    inp[3] = 1.0 - A.r;
    inp[4] = 1.0 - A.b;
    inp[5] = B.r;
    inp[6] = max(n.y, 0.0);
    inp[7] = 1.0 - A.a;
    inp[8] = C.a;
    inp[9] = (p.y - u.bmin.y) / max(u.bsize.y, 1e-4);
    inp[10] = B.a;
    inp[11] = 1.0 - smoothstep(0.0, width, B.b * emax);
    inp[12] = 0.0;
    if (weight(12) != 0.0) {
        let s = fbm(vec3<f32>(p.x * scale * 7.0, p.y * scale * 0.45, p.z * scale * 7.0) + vec3<f32>(u.gp1.y));
        inp[12] = smoothstep(0.45, 0.8, s);
    }
    inp[13] = 0.0;
    if (weight(13) != 0.0) {
        inp[13] = fbm(q);
    }
    let tol = max(u.gid.w, 1e-3);
    inp[14] = 1.0 - smoothstep(tol * 0.5, tol, distance(E.rgb, u.gid.rgb));
    inp[15] = max(-n.y, 0.0);
    inp[16] = 0.0;
    if (weight(16) != 0.0) {
        inp[16] = wood_grain(p, n, scale);
    }
    inp[17] = 0.0;
    if (weight(17) != 0.0) {
        inp[17] = cellular(q * 4.0);
    }
    inp[18] = 0.0;
    if (weight(18) != 0.0) {
        inp[18] = scratches(p, n, scale, texel_world);
    }

    var v = 0.0;
    for (var i = 0; i < 19; i++) {
        v += weight(i) * inp[i];
    }
    for (var k = 0; k < 4; k++) {
        let ci = i32(u.gc[k]);
        if (ci >= 0) {
            let t = textureSampleLevel(custom_maps, samp, uv, ci / 4, 0.0);
            v += u.gcw[k] * t[ci % 4];
        }
    }
    let noise_amt = u.gp0.w;
    if (noise_amt != 0.0) {
        // Fine grit breaks the mask's border up; a broad, slow variation on
        // top moves it in and out along long edges, so wear and grime don't
        // follow the mesh maps' contours as an even band.
        let broad = fbm(q * 0.35 + vec3<f32>(11.7, 3.1, 5.9)) - 0.5;
        v += ((grunge(q * 2.0) - 0.5) * 1.3 + broad * 2.6) * noise_amt;
    }
    let level = u.gp0.x;
    let contrast = max(u.gp0.y, 0.001);
    var m = smoothstep(level - contrast, level + contrast, v);
    if (u.gp1.x > 0.5) {
        m = 1.0 - m;
    }
    return m;
}

struct AccumOut {
    @location(0) color: vec4<f32>,
    @location(1) rough: vec4<f32>,
    @location(2) metal: vec4<f32>,
    @location(3) height: vec4<f32>,
};

// The layer's mask before opacity: the painted mask (or, with mask filters,
// the filtered mask) times the smart mask.
fn layer_mask(c: vec2<i32>, p: vec3<f32>, n: vec3<f32>, uv: vec2<f32>) -> f32 {
    var mask = 1.0;
    if (u.p0.w > 0.5) {
        mask *= textureLoad(l_mask, c, 0).r;
    }
    if (u.gp1.z > 0.5) {
        mask *= generator(p, n, uv, texel_size(c, p));
    }
    return mask;
}

// The layer's mask alone, as the first step of its mask filters.
@fragment
fn fs_mask_source(in: FsOut) -> @location(0) vec4<f32> {
    let c = vec2<i32>(in.pos.xy);
    let uv = in.pos.xy / vec2<f32>(textureDimensions(pos_map));
    let p = textureLoad(pos_map, c, 0).xyz;
    let n = normalize(textureLoad(nrm_map, c, 0).xyz * 2.0 - 1.0);
    return vec4<f32>(layer_mask(c, p, n, uv), 0.0, 0.0, 1.0);
}

@fragment
fn fs_layer(in: FsOut) -> AccumOut {
    let c = vec2<i32>(in.pos.xy);
    let dims = vec2<f32>(textureDimensions(prev_color));
    let uv = in.pos.xy / dims;

    let pc = textureLoad(prev_color, c, 0).rgb;
    let pr = textureLoad(prev_rough, c, 0).r;
    let pm = textureLoad(prev_metal, c, 0).r;
    let ph = textureLoad(prev_height, c, 0).r;

    let p = textureLoad(pos_map, c, 0).xyz;
    let n = normalize(textureLoad(nrm_map, c, 0).xyz * 2.0 - 1.0);

    var sc = u.fill_color.rgb;
    var sr = u.fill_rmh.x;
    var sm = u.fill_rmh.y;
    var sh = u.fill_rmh.z;
    var ac = u.ch.x;
    var ar = u.ch.y;
    var am = u.ch.z;
    var ah = u.ch.w;

    if (u.p0.z < 0.5) {
        // Paint layer: per-channel (value, alpha).
        if (u.ch.x > 0.5) {
            let t = textureLoad(l_color, c, 0);
            sc = t.rgb;
            ac = t.a;
        }
        if (u.ch.y > 0.5) {
            let t = textureLoad(l_rough, c, 0).rg;
            sr = t.x;
            ar = t.y;
        }
        if (u.ch.z > 0.5) {
            let t = textureLoad(l_metal, c, 0).rg;
            sm = t.x;
            am = t.y;
        }
        if (u.ch.w > 0.5) {
            let t = textureLoad(l_height, c, 0).rg;
            sh = t.x;
            ah = t.y;
        }
    } else {
        // Fill layer: images replace their channels' values, then procedural
        // and per-part variation apply on top.
        let bits = u32(u.var1.w);
        if (bits != 0u) {
            let texel_world = texel_size(c, p);
            if ((bits & 1u) != 0u) {
                let img = fill_image(l_color, uv, p, n, texel_world);
                sc = select(img.rgb, img.rrr, (bits & 16u) != 0u);
            }
            if ((bits & 2u) != 0u) {
                sr = fill_image(l_rough, uv, p, n, texel_world).r;
            }
            if ((bits & 4u) != 0u) {
                sm = fill_image(l_metal, uv, p, n, texel_world).r;
            }
            if ((bits & 8u) != 0u) {
                // Mid grey is the surface; lighter is raised. The layer's
                // height value scales it (0.5: as is; negative inverts).
                sh = (fill_image(l_height, uv, p, n, texel_world).r - 0.5) * 2.0 * u.fill_rmh.z;
            }
        }
        let vs = u.var0.y;
        let seed = vec3<f32>(u.var1.x * 13.7);
        if (u.var0.x != 0.0) {
            let n1 = fbm(p * vs + seed);
            sc = sc * (1.0 + (n1 - 0.5) * 2.0 * u.var0.x);
        }
        if (u.var0.z != 0.0) {
            let n2 = fbm(p * vs * 1.7 + seed + vec3<f32>(5.2));
            sr = sr + (n2 - 0.5) * u.var0.z;
        }
        if (u.var0.w != 0.0) {
            let n3 = fbm(p * vs * 3.1 + seed + vec3<f32>(11.9));
            sh = sh + (n3 - 0.5) * u.var0.w;
        }
        if (u.var1.y != 0.0) {
            let pr_rand = textureSampleLevel(map_b, samp, uv, 0.0).a;
            sc = sc * (1.0 + (pr_rand - 0.5) * 2.0 * u.var1.y);
            sr = sr + (fract(pr_rand * 7.31) - 0.5) * u.var1.y * 0.6;
        }
        sc = clamp(sc, vec3<f32>(0.0), vec3<f32>(1.0));
        sr = clamp(sr, 0.0, 1.0);
    }

    let mask = u.p0.x * layer_mask(c, p, n, uv);

    let mode = i32(u.p0.y);
    var out: AccumOut;
    out.color = vec4<f32>(clamp(mix(pc, blend3(mode, pc, sc), ac * mask), vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
    out.rough = vec4<f32>(clamp(mix(pr, blend1(mode, pr, sr), ar * mask), 0.0, 1.0));
    out.metal = vec4<f32>(clamp(mix(pm, blend1(mode, pm, sm), am * mask), 0.0, 1.0));
    out.height = vec4<f32>(mix(ph, blend1(mode, ph, sh), ah * mask));
    return out;
}

// Ends a faded or masked folder: the stack with the folder's layers on it
// (prev_*) is faded over the stack below the folder (in the layer slots, l_*)
// by the folder's opacity and mask, like an isolated group.
@fragment
fn fs_folder_merge(in: FsOut) -> AccumOut {
    let c = vec2<i32>(in.pos.xy);
    let uv = in.pos.xy / vec2<f32>(textureDimensions(prev_color));
    var m = u.p0.x;
    if (u.p0.w > 0.5) {
        m *= textureLoad(l_mask, c, 0).r;
    }
    if (u.gp1.z > 0.5) {
        let p = textureLoad(pos_map, c, 0).xyz;
        let n = normalize(textureLoad(nrm_map, c, 0).xyz * 2.0 - 1.0);
        m *= generator(p, n, uv, texel_size(c, p));
    }
    var out: AccumOut;
    out.color = vec4<f32>(mix(textureLoad(l_color, c, 0).rgb, textureLoad(prev_color, c, 0).rgb, m), 1.0);
    out.rough = vec4<f32>(mix(textureLoad(l_rough, c, 0).r, textureLoad(prev_rough, c, 0).r, m));
    out.metal = vec4<f32>(mix(textureLoad(l_metal, c, 0).r, textureLoad(prev_metal, c, 0).r, m));
    out.height = vec4<f32>(mix(textureLoad(l_height, c, 0).r, textureLoad(prev_height, c, 0).r, m));
    return out;
}
