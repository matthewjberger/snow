#import snow::char_uniforms::CharUniforms
#import snow::char_skin::sampleCloth
#import snow::fabric::{FabricInput, shadeFabric}

// The garments: surface reconstructed from the simulated node grid.

@group(0) @binding(0) var<uniform> uniforms: CharUniforms;

@group(1) @binding(0) var charTex: texture_2d<f32>;
@group(1) @binding(1) var skyLUT: texture_2d<f32>;
@group(1) @binding(2) var skySamp: sampler;
@group(1) @binding(3) var cascade0: texture_2d<f32>;
@group(1) @binding(4) var cascade1: texture_2d<f32>;
@group(1) @binding(5) var cascade2: texture_2d<f32>;
@group(1) @binding(6) var cascadeSamp: sampler;

struct VertexInput {
    // Surface coordinates across and down the panel, then the panel index.
    @location(0) surface: vec3f,
    // Weave coordinates, in metres of surface.
    @location(1) uv: vec2f,
    // (material slot, baked occlusion)
    @location(2) aux: vec2f,
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
    let panel = uniforms.panels[i32(input.surface.z)];
    let sample = sampleCloth(
        charTex, i32(panel.x), i32(panel.y), i32(panel.z),
        input.surface.x, input.surface.y
    );

    var out: Varyings;
    out.world = sample.pos;
    out.normal = sample.nrm;
    out.uv = input.uv;
    out.aux = input.aux;
    out.viewDist = distance(sample.pos, uniforms.camera.xyz);
    out.clip = uniforms.viewProjection * vec4f(sample.pos, 1.0);
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
