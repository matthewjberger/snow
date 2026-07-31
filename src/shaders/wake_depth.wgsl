#import snow::snow_uniforms::SnowUniforms
#import snow::wake::{wakePoint, wakeScalars, wakeEroded}

// The wake's contribution to the shadow cascades.

@group(0) @binding(0) var<uniform> uniforms: SnowUniforms;
@group(1) @binding(0) var wakeTex: texture_2d<f32>;

struct VertexInput {
    @location(0) lattice: vec3f,
}

struct Varyings {
    @builtin(position) clip: vec4f,
    @location(0) depth: f32,
    @location(1) q: f32,
    @location(2) along: f32,
    @location(3) age: f32,
}

@vertex
fn vertexMain(input: VertexInput) -> Varyings {
    let count = uniforms.wake.x;
    let time = uniforms.wake.w;
    let side = input.lattice.z;
    let u = input.lattice.x / max(uniforms.wake.y - 1.0, 1.0);
    let q = input.lattice.y / max(uniforms.wake.z - 1.0, 1.0);

    let point = wakePoint(wakeTex, count, u, q, side, time);
    let scalars = wakeScalars(wakeTex, count, u, side);

    var out: Varyings;
    out.clip = uniforms.viewProjection * vec4f(point, 1.0);
    out.depth = out.clip.z / out.clip.w;
    out.q = q;
    out.along = scalars.z;
    out.age = scalars.w;
    return out;
}

@fragment
fn fragmentMain(input: Varyings) -> @location(0) vec4f {
    if (wakeEroded(input.along, input.q, input.age, uniforms.wake.w)) { discard; }
    return vec4f(input.depth, 0.0, 0.0, 1.0);
}
