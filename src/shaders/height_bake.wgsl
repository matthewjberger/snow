#import snow::terrain::{terrainMacro, rockField}

// Bakes the macro landform (broad dunes, medium drifts and rock outcrops) into a two-
// channel float texture covering the whole playable field.

struct HeightBakeUniforms {
    // (origin.x, origin.z, worldSize, windAngle)
    world: vec4f,
    // (heightAmp, 0, 0, 0)
    params: vec4f,
}

@group(0) @binding(0) var<uniform> uniforms: HeightBakeUniforms;

@fragment
fn fragmentMain(input: FullscreenVertex) -> @location(0) vec4f {
    let p = uniforms.world.xy + input.uv * uniforms.world.z;

    var h = terrainMacro(p, uniforms.world.w, uniforms.params.x);

    // Rock displaces snow upward; snow then re-accumulates on the flatter faces, which
    // the snow material resolves from the mask in the aux bake.
    let rock = rockField(p, uniforms.world.w);
    h += rock.x;

    return vec4f(h, rock.y, 0.0, 1.0);
}
