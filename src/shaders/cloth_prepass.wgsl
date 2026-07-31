#import snow::char_uniforms::CharUniforms
#import snow::char_skin::sampleCloth

// The garments' contribution to the camera-space depth prepass.

@group(0) @binding(0) var<uniform> uniforms: CharUniforms;
@group(1) @binding(0) var charTex: texture_2d<f32>;

struct VertexInput {
    @location(0) surface: vec3f,
    @location(1) uv: vec2f,
    @location(2) aux: vec2f,
}

struct Varyings {
    @builtin(position) clip: vec4f,
    @location(0) viewZ: f32,
}

@vertex
fn vertexMain(input: VertexInput) -> Varyings {
    let panel = uniforms.panels[i32(input.surface.z)];
    let sample = sampleCloth(
        charTex, i32(panel.x), i32(panel.y), i32(panel.z),
        input.surface.x, input.surface.y
    );
    var out: Varyings;
    out.clip = uniforms.viewProjection * vec4f(sample.pos, 1.0);
    out.viewZ = out.clip.w;
    return out;
}

@fragment
fn fragmentMain(input: Varyings) -> @location(0) vec4f {
    return vec4f(input.viewZ, 0.0, 0.0, 1.0);
}
