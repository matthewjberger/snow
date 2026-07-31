#import snow::snow_uniforms::SnowUniforms
#import snow::terrain_vertex::placeTerrainVertex

// Shadow-pass program for the terrain.

@group(0) @binding(0) var<uniform> uniforms: SnowUniforms;

@group(1) @binding(0) var heightTex: texture_2d<f32>;
@group(1) @binding(1) var heightSamp: sampler;
@group(1) @binding(2) var auxTex: texture_2d<f32>;
@group(1) @binding(3) var auxSamp: sampler;
@group(1) @binding(4) var deformTex: texture_2d<f32>;
@group(1) @binding(5) var deformSamp: sampler;

struct Varyings {
    @builtin(position) clip: vec4f,
    @location(0) depth: f32,
}

@vertex
fn vertexMain(@location(0) packed: vec3f) -> Varyings {
    let placed = placeTerrainVertex(
        heightTex, heightSamp, auxTex, auxSamp, deformTex, deformSamp,
        vec2f(packed.x, packed.z), packed.y,
        uniforms.clipmap, uniforms.field, uniforms.surface,
        uniforms.deform, uniforms.misc.x
    );

    var out: Varyings;
    out.clip = uniforms.viewProjection * vec4f(placed.world, 1.0);
    out.depth = out.clip.z / out.clip.w;
    return out;
}

@fragment
fn fragmentMain(input: Varyings) -> @location(0) vec4f {
    return vec4f(input.depth, 0.0, 0.0, 1.0);
}
