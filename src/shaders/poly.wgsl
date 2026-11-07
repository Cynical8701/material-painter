// Polygon fill: selected triangles drawn at their UVs into the stroke mask
// with full coverage. Island borders arrive with extra padding strips, drawn
// in a second pass that only fills texels outside every UV island (the
// position map's alpha is 1 inside one), so padding never spills onto a
// neighboring island in a tight UV layout.

@group(0) @binding(0) var pos_map: texture_2d<f32>;

@vertex
fn vs_poly(@location(0) uv: vec2<f32>) -> @builtin(position) vec4<f32> {
    return vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0);
}

@fragment
fn fs_poly() -> @location(0) vec4<f32> {
    return vec4<f32>(1.0, 0.0, 0.0, 1.0);
}

@fragment
fn fs_poly_pad(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    if (textureLoad(pos_map, vec2<i32>(at.xy), 0).a > 0.9) {
        discard;
    }
    return vec4<f32>(1.0, 0.0, 0.0, 1.0);
}
