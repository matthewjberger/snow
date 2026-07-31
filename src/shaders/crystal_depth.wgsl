#import snow::snow_uniforms::SnowUniforms
#import snow::crystal::crystalPoint

// The formations' contribution to the shadow cascades, from the same shape the
// beauty pass draws: a formation whose shadow is a different shape from the
// formation is worse than no shadow at all.

@group(0) @binding(0) var<uniform> uniforms: SnowUniforms;
@group(1) @binding(0) var crystalTex: texture_2d<f32>;

struct VertexInput {
    @location(0) lattice: vec3f,
}

struct Varyings {
    @builtin(position) clip: vec4f,
    @location(0) depth: f32,
}

@vertex
fn vertexMain(input: VertexInput) -> Varyings {
    let point = crystalPoint(crystalTex, i32(input.lattice.x), i32(input.lattice.y));
    var out: Varyings;
    out.clip = uniforms.viewProjection * vec4f(point, 1.0);
    out.depth = out.clip.z / out.clip.w;
    return out;
}

@fragment
fn fragmentMain(input: Varyings) -> @location(0) vec4f {
    return vec4f(input.depth, 0.0, 0.0, 1.0);
}
