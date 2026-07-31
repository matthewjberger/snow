#define_import_path snow::shadow_lookup

#import snow::shading::pcssShadow

// The receiving half of the cascaded shadow maps.

struct ShadowParams {
    matrices: array<mat4x4f, 3>,
    // Per cascade: (depth range in metres, ortho width in metres, 0, 0).
    cascade: array<vec4f, 3>,
    // Far distance of each cascade in metres; w repeats the last.
    splits: vec4f,
    // (one shadow texel in UV, softness, depth bias in metres, 0)
    filterParams: vec4f,
    // Points toward the sun; w unused.
    sunDir: vec4f,
}

// Project into one cascade and run the soft-shadow filter.
fn sampleCascadeTex(
    tex: texture_2d<f32>,
    samp: sampler,
    shadow: ShadowParams,
    m: mat4x4f,
    params: vec4f,
    world: vec3f,
    geoN: vec3f,
    noiseRot: f32
) -> f32 {
    let depthRange = params.x;
    let orthoWidth = params.y;
    let shadowTexel = shadow.filterParams.x;
    let texelWorld = orthoWidth * shadowTexel;

    // ---- the light's own basis -------------------------------------------
    // Reconstructed here rather than passed in, so it cannot drift out of sync with the
    // matrix.
    let lf = -shadow.sunDir.xyz;
    let lr = normalize(cross(vec3f(0.0, 1.0, 0.0), lf));
    let lu = cross(lf, lr);

    // Surface normal in that basis.
    let nl = vec3f(dot(geoN, lr), dot(geoN, lu), dot(geoN, lf));
    let nz = select(min(nl.z, -1e-3), max(nl.z, 1e-3), nl.z >= 0.0);
    let grad = clamp(vec2f(-nl.x / nz, -nl.y / nz), vec2f(-6.0), vec2f(6.0));

    // Metres of light-space travel per unit UV.
    let planeNdcPerUV = vec2f(grad.x, grad.y) * orthoWidth / depthRange;

    // ---- normal-offset bias ---------------------------------------------- Move the
    // receiver off the surface by a texel's worth before projecting, scaled by how
    // obliquely the light meets it.
    let sinL = sqrt(clamp(1.0 - nl.z * nl.z, 0.0, 1.0));
    let biased = world + geoN * (texelWorld * 1.5 * max(sinL, 0.2));

    let clip = m * vec4f(biased, 1.0);
    let ndc = clip.xyz / clip.w;
    if (any(abs(ndc.xy) > vec2f(1.0)) || ndc.z < 0.0 || ndc.z > 1.0) { return 1.0; }

    let uv = vec2f(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);

    return pcssShadow(
        tex, samp, uv, ndc.z, shadowTexel,
        depthRange, orthoWidth, shadow.filterParams.y, noiseRot, shadow.filterParams.z, planeNdcPerUV
    );
}

// Pick a cascade and evaluate the soft shadow, cross-fading over the last 12% / of
// each slice so the filter width never visibly steps.
fn sunShadow(
    cascade0: texture_2d<f32>,
    cascade0Samp: sampler,
    cascade1: texture_2d<f32>,
    cascade1Samp: sampler,
    cascade2: texture_2d<f32>,
    cascade2Samp: sampler,
    shadow: ShadowParams,
    world: vec3f,
    geoN: vec3f,
    viewDist: f32,
    noiseRot: f32
) -> f32 {
    let sp = shadow.splits;

    if (viewDist >= sp.z) { return 1.0; }

    if (viewDist < sp.x) {
        let s = sampleCascadeTex(cascade0, cascade0Samp, shadow, shadow.matrices[0],
                                 shadow.cascade[0], world, geoN, noiseRot);
        let blendStart = sp.x * 0.88;
        if (viewDist <= blendStart) { return s; }
        let s2 = sampleCascadeTex(cascade1, cascade1Samp, shadow, shadow.matrices[1],
                                  shadow.cascade[1], world, geoN, noiseRot);
        return mix(s, s2, clamp((viewDist - blendStart) / (sp.x - blendStart), 0.0, 1.0));
    }

    if (viewDist < sp.y) {
        let s = sampleCascadeTex(cascade1, cascade1Samp, shadow, shadow.matrices[1],
                                 shadow.cascade[1], world, geoN, noiseRot);
        let blendStart = sp.y * 0.88;
        if (viewDist <= blendStart) { return s; }
        let s2 = sampleCascadeTex(cascade2, cascade2Samp, shadow, shadow.matrices[2],
                                  shadow.cascade[2], world, geoN, noiseRot);
        return mix(s, s2, clamp((viewDist - blendStart) / (sp.y - blendStart), 0.0, 1.0));
    }

    let s = sampleCascadeTex(cascade2, cascade2Samp, shadow, shadow.matrices[2],
                             shadow.cascade[2], world, geoN, noiseRot);
    // Fade the last cascade out at its far edge rather than cutting to lit.
    return mix(s, 1.0, smoothstep(sp.z * 0.85, sp.z, viewDist));
}
