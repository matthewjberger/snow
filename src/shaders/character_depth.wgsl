#import snow::char_uniforms::CharUniforms
#import snow::char_skin::skinPoint

// The body's shadow-cascade program.

@group(0) @binding(0) var<uniform> uniforms: CharUniforms;
@group(1) @binding(0) var charTex: texture_2d<f32>;

struct VertexInput {
    @location(0) position: vec3f,
    @location(1) normal: vec3f,
    @location(2) uv: vec2f,
    @location(3) aux: vec2f,
    @location(4) boneIndex: vec4f,
    @location(5) boneWeight: vec4f,
}

struct Varyings {
    @builtin(position) clip: vec4f,
    @location(0) depth: f32,
}

@vertex
fn vertexMain(input: VertexInput) -> Varyings {
    let world = skinPoint(charTex, input.boneIndex, input.boneWeight, input.position);
    var out: Varyings;
    out.clip = uniforms.viewProjection * vec4f(world, 1.0);
    out.depth = out.clip.z / out.clip.w;
    return out;
}

@fragment
fn fragmentMain(input: Varyings) -> @location(0) vec4f {
    return vec4f(input.depth, 0.0, 0.0, 1.0);
}
