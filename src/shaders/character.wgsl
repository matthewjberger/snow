#import snow::char_uniforms::CharUniforms
#import snow::char_skin::{skinPoint, skinNormal}
#import snow::fabric::{FabricInput, shadeFabric}

// The body: linear blend skinning straight out of the transform texture, shaded by the
// shared fabric material.

@group(0) @binding(0) var<uniform> uniforms: CharUniforms;

@group(1) @binding(0) var charTex: texture_2d<f32>;
@group(1) @binding(1) var skyLUT: texture_2d<f32>;
@group(1) @binding(2) var skySamp: sampler;
@group(1) @binding(3) var cascade0: texture_2d<f32>;
@group(1) @binding(4) var cascade1: texture_2d<f32>;
@group(1) @binding(5) var cascade2: texture_2d<f32>;
@group(1) @binding(6) var cascadeSamp: sampler;

struct VertexInput {
    // Bind-pose world position.
    @location(0) position: vec3f,
    // Bind-pose world normal.
    @location(1) normal: vec3f,
    // Weave coordinates, in metres of surface.
    @location(2) uv: vec2f,
    // (material slot, baked occlusion)
    @location(3) aux: vec2f,
    @location(4) boneIndex: vec4f,
    @location(5) boneWeight: vec4f,
}

struct Varyings {
    @builtin(position) clip: vec4f,
    @location(0) world: vec3f,
    @location(1) normal: vec3f,
    @location(2) uv: vec2f,
    @location(3) aux: vec2f,
    @location(4) viewDist: f32,
}

@vertex
fn vertexMain(input: VertexInput) -> Varyings {
    let world = skinPoint(charTex, input.boneIndex, input.boneWeight, input.position);
    let normal = skinNormal(charTex, input.boneIndex, input.boneWeight, input.normal);

    var out: Varyings;
    out.world = world;
    out.normal = normal;
    out.uv = input.uv;
    out.aux = input.aux;
    out.viewDist = distance(world, uniforms.camera.xyz);
    out.clip = uniforms.viewProjection * vec4f(world, 1.0);
    return out;
}

@fragment
fn fragmentMain(input: Varyings) -> @location(0) vec4f {
    let color = shadeFabric(
        uniforms, skyLUT, skySamp, cascade0, cascade1, cascade2, cascadeSamp,
        FabricInput(input.world, input.normal, input.uv, input.aux, input.viewDist, input.clip.xy)
    );
    return vec4f(color, 1.0);
}
