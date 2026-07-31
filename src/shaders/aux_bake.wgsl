
// Derives everything the snow material needs to know about the macro landform that is
// not the height itself, by differentiating the baked height texture rather than the
// analytic function.

struct AuxBakeUniforms {
    // (texelWorld, invHeightRes, 0, 0)
    params: vec4f,
}

@group(0) @binding(0) var<uniform> uniforms: AuxBakeUniforms;
@group(0) @binding(1) var heightTex: texture_2d<f32>;
@group(0) @binding(2) var heightSampler: sampler;

@fragment
fn fragmentMain(input: FullscreenVertex) -> @location(0) vec4f {
    let uv = input.uv;
    let t = uniforms.params.y;
    let d = uniforms.params.x;

    let hL = textureSample(heightTex, heightSampler, uv - vec2f(t, 0.0));
    let hR = textureSample(heightTex, heightSampler, uv + vec2f(t, 0.0));
    let hD = textureSample(heightTex, heightSampler, uv - vec2f(0.0, t));
    let hU = textureSample(heightTex, heightSampler, uv + vec2f(0.0, t));
    let hC = textureSample(heightTex, heightSampler, uv);

    // Central difference: second-order accurate, and symmetric so flat ground produces
    // exactly zero slope instead of a bias.
    let dHdx = (hR.x - hL.x) / (2.0 * d);
    let dHdz = (hU.x - hD.x) / (2.0 * d);

    // --- exposure ---------------------------------------------------------- Wide-
    // stencil Laplacian: positive on convex crests (which the wind scours and packs
    // into sastrugi), negative in concave hollows (where loose drift collects).
    let w = t * 6.0;
    let wd = d * 6.0;
    let lL = textureSample(heightTex, heightSampler, uv - vec2f(w, 0.0)).x;
    let lR = textureSample(heightTex, heightSampler, uv + vec2f(w, 0.0)).x;
    let lD = textureSample(heightTex, heightSampler, uv - vec2f(0.0, w)).x;
    let lU = textureSample(heightTex, heightSampler, uv + vec2f(0.0, w)).x;
    let lap = (lL + lR + lD + lU - 4.0 * hC.x) / (wd * wd);

    // Negated so crests come out positive.
    let exposure = clamp(0.5 - lap * 2.2, 0.0, 1.0);

    return vec4f(dHdx, dHdz, hC.y, exposure);
}
