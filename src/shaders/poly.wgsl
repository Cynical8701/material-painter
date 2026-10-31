// Polygon fill: selected triangles drawn at their UVs into the stroke mask
// with full coverage. Island borders arrive with extra padding strips.

@vertex
fn vs_poly(@location(0) uv: vec2<f32>) -> @builtin(position) vec4<f32> {
    return vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0);
}

@fragment
fn fs_poly() -> @location(0) vec4<f32> {
    return vec4<f32>(1.0, 0.0, 0.0, 1.0);
}
