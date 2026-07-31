#define_import_path snow::fullscreen

struct FullscreenVertex {
    @builtin(position) clip: vec4f,
    @location(0) uv: vec2f,
}

// Covers the target with one oversized triangle.
fn fullscreenTriangle(index: u32) -> FullscreenVertex {
    let uv = vec2f(f32((index << 1u) & 2u), f32(index & 2u));
    var out: FullscreenVertex;
    out.uv = uv;
    out.clip = vec4f(uv * vec2f(2.0, -2.0) + vec2f(-1.0, 1.0), 0.0, 1.0);
    return out;
}
