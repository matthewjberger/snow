#import snow::snow_uniforms::SnowUniforms
#import snow::terrain_vertex::placeTerrainVertex
#import snow::deform::{deformUV, deformFalloff}

// Depth-prepass program for the terrain.

@group(0) @binding(0) var<uniform> uniforms: SnowUniforms;

@group(1) @binding(0) var heightTex: texture_2d<f32>;
@group(1) @binding(1) var heightSamp: sampler;
@group(1) @binding(2) var auxTex: texture_2d<f32>;
@group(1) @binding(3) var auxSamp: sampler;
@group(1) @binding(4) var deformTex: texture_2d<f32>;
@group(1) @binding(5) var deformSamp: sampler;

struct Varyings {
    @builtin(position) clip: vec4f,
    @location(0) viewZ: f32,
    // 0 matte snow, 1 mirror ice.
    @location(1) mask: f32,
}

@vertex
fn vertexMain(@location(0) packed: vec3f) -> Varyings {
    let placed = placeTerrainVertex(
        heightTex, heightSamp, auxTex, auxSamp, deformTex, deformSamp,
        vec2f(packed.x, packed.z), packed.y,
        uniforms.clipmap, uniforms.field, uniforms.surface,
        uniforms.deform, uniforms.misc.x
    );

    // The ice channel, read straight rather than through the displacement's binomial:
    // this feeds a reflection gate, not a displacement, so smoothing it to the vertex
    // lattice would only soften the edge of a glaze the fragment stage draws hard.
    var mask = 0.0;
    let weight = deformFalloff(placed.world.xz, uniforms.deform.xy, uniforms.deform.z);
    if (weight > 0.001) {
        let s = textureSampleLevel(
            deformTex, deformSamp, deformUV(placed.world.xz, uniforms.deform.z), 0.0
        );
        mask = clamp(s.a, 0.0, 1.0) * weight;
    }

    var out: Varyings;
    out.clip = uniforms.viewProjection * vec4f(placed.world, 1.0);
    out.viewZ = out.clip.w;
    out.mask = mask;
    return out;
}

@fragment
fn fragmentMain(input: Varyings) -> @location(0) vec4f {
    return vec4f(input.viewZ, input.mask, 0.0, 1.0);
}
