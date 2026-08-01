#define_import_path snow::shading

#import snow::noise::{PI, hash22}

// The BRDF, the subsurface term, the glints and the shadow filter.

// ---------------------------------------------------------------- microfacet

fn distributionGGX(NdotH: f32, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let d = NdotH * NdotH * (a2 - 1.0) + 1.0;
    return a2 / max(1e-7, PI * d * d);
}

fn visSmithGGXCorrelated(NdotV: f32, NdotL: f32, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let gv = NdotL * sqrt(NdotV * NdotV * (1.0 - a2) + a2);
    let gl = NdotV * sqrt(NdotL * NdotL * (1.0 - a2) + a2);
    return 0.5 / max(1e-7, gv + gl);
}

fn fresnelSchlick(u: f32, f0: vec3f) -> vec3f {
    let f = pow(1.0 - u, 5.0);
    return f0 + (vec3f(1.0) - f0) * f;
}

fn fresnelSchlickRough(u: f32, f0: vec3f, roughness: f32) -> vec3f {
    let f = pow(1.0 - u, 5.0);
    return f0 + (max(vec3f(1.0 - roughness), f0) - f0) * f;
}

// ---------------------------------------------------------------- subsurface

// Wrapped diffuse.
fn wrapDiffuse(NdotL: f32, w: f32) -> f32 {
    let denom = (1.0 + w) * (1.0 + w);
    return max(0.0, (NdotL + w) / denom);
}

// Back-scatter transmission: light that entered the surface, scattered, and / left
// toward the eye.
fn backScatter(N: vec3f, L: vec3f, V: vec3f, distortion: f32, power: f32, thickness: f32) -> f32 {
    let H = normalize(L + N * distortion);
    let vh = pow(clamp(dot(V, -H), 0.0, 1.0), power);
    return vh * thickness;
}

// Combined snow subsurface response for one light.
fn snowSubsurface(
    N: vec3f,
    L: vec3f,
    V: vec3f,
    lightColor: vec3f,
    thickness: f32,
    strength: f32,
    radius: f32
) -> vec3f {
    // Deeper snow scatters longer and comes back bluer, because red is absorbed first
    // over any appreciable path length.
    let shallowTint = vec3f(0.94, 0.965, 1.0);
    let deepTint = vec3f(0.55, 0.72, 1.0);
    let tint = mix(shallowTint, deepTint, clamp(thickness * radius, 0.0, 1.0));

    // Lobe width and amplitude both key off thickness, and both run the opposite way to
    // how they read at first glance: a thin edge (a drift lip, a berm crest, the far
    // wall of a footprint) transmits brightly and over a wide range of angles, because
    // the path through it is short from almost anywhere.
    let back = backScatter(
        N, L, V, 0.28 * radius,
        mix(3.0, 9.0, thickness),
        mix(1.0, 0.30, thickness)
    );

    return lightColor * tint * back * strength;
}

// -------------------------------------------------------------------- glints

// One octave of discrete surface sparkle.
fn glintOctave(
    p: vec2f,
    cell: f32,
    N: vec3f,
    H: vec3f,
    T: vec3f,
    B: vec3f,
    sharpness: f32
) -> f32 {
    let id = floor(p / cell);
    let r = hash22(id);
    let r2 = hash22(id + vec2f(19.73, 7.31));

    // Only a fraction of cells hold a crystal facet oriented to catch anything.
    if (r2.x > 0.62) { return 0.0; }

    let centre = (id + 0.5 + (r - 0.5) * 0.72) * cell;
    let d = length(p - centre) / (cell * 0.17);
    let disc = clamp(1.0 - d * d, 0.0, 1.0);
    if (disc <= 0.0) { return 0.0; }

    // Tilt the facet off the surface normal by a random amount in the tangent plane.
    let ang = r.y * 6.28318530718;
    let tilt = 0.10 + r2.y * 0.26;
    let facet = normalize(N + (T * cos(ang) + B * sin(ang)) * tilt);

    let nh = clamp(dot(facet, H), 0.0, 1.0);
    return disc * pow(nh, sharpness);
}

// Full glint response.
fn snowGlints(
    worldXZ: vec2f,
    N: vec3f,
    V: vec3f,
    L: vec3f,
    pixelFootprint: f32,
    intensity: f32,
    grazeGate: f32
) -> f32 {
    if (intensity <= 0.0) { return 0.0; }

    let H = normalize(V + L);

    // Any stable tangent frame will do, since the facets are random anyway.
    let up = select(vec3f(0.0, 1.0, 0.0), vec3f(1.0, 0.0, 0.0), abs(N.y) > 0.95);
    let T = normalize(cross(up, N));
    let B = cross(N, T);

    // Grazing gate: 1 looking along the surface, 0 looking straight down.
    let NdotV = clamp(dot(N, V), 0.0, 1.0);
    let graze = pow(1.0 - NdotV, mix(1.5, 5.0, grazeGate));

    // The sun must be low relative to the surface too, so a facet has something to bounce
    // toward the eye.
    let NdotL = clamp(dot(N, L), 0.0, 1.0);
    let lightGate = smoothstep(0.02, 0.35, NdotL) * (1.0 - smoothstep(0.55, 0.95, NdotL) * 0.55);

    let gate = graze * lightGate;
    if (gate <= 0.001) { return 0.0; }

    var sum = 0.0;

    let cellA = 0.052;
    let fadeA = smoothstep(cellA * 0.55, cellA * 2.2, pixelFootprint);
    if (fadeA < 1.0) {
        sum += glintOctave(worldXZ, cellA, N, H, T, B, 780.0) * (1.0 - fadeA);
    }

    let cellB = 0.185;
    let fadeB = smoothstep(cellB * 0.55, cellB * 2.2, pixelFootprint);
    if (fadeB < 1.0) {
        sum += glintOctave(worldXZ + vec2f(53.1, 17.9), cellB, N, H, T, B, 1500.0)
            * (1.0 - fadeB) * 1.35;
    }

    return sum * gate * intensity;
}

// ------------------------------------------------------------------- shadows

// Poisson-ish disc, precomputed.
const POISSON: array<vec2f, 12> = array<vec2f, 12>(
    vec2f(-0.326, -0.406), vec2f(-0.840, -0.074), vec2f(-0.696,  0.457),
    vec2f(-0.203,  0.621), vec2f( 0.962, -0.195), vec2f( 0.473, -0.480),
    vec2f( 0.519,  0.767), vec2f( 0.185, -0.893), vec2f( 0.507,  0.064),
    vec2f( 0.896,  0.412), vec2f(-0.322, -0.933), vec2f(-0.792, -0.598)
);

// Percentage-closer soft shadows, worked entirely in world units.
fn pcssShadow(
    shadowMap: texture_2d<f32>,
    shadowSamp: sampler,
    uv: vec2f,
    receiverDepth: f32,
    texelSize: f32,
    depthRange: f32,
    orthoWidth: f32,
    softness: f32,
    noiseRot: f32,
    biasWorld: f32,
    planeNdcPerUV: vec2f
) -> f32 {
    let bias = biasWorld / depthRange;

    // ---- receiver-plane depth bias --------------------------------------- Both loops
    // below sample the map away from the shading point, and then have to decide whether
    // what they found is an occluder.
    let planeAt = planeNdcPerUV;

    // Widest penumbra we will ever produce, in UV, and also the search radius, since an
    // occluder further away than this cannot soften anything more.
    let maxPenumbraUV = min(24.0 * texelSize, 1.8 / orthoWidth);

    let cs = cos(noiseRot);
    let sn = sin(noiseRot);
    let rot = mat2x2f(cs, -sn, sn, cs);

    // ---- blocker search --------------------------------------------------
    // Accumulates how far in front of the plane each blocker sits, rather than its raw
    // depth, so the penumbra estimate inherits the same correction.
    var blockerDepthSum = 0.0;
    var blockerCount = 0.0;

    for (var i = 0; i < 8; i++) {
        let off = rot * POISSON[i] * maxPenumbraUV;
        let s = clamp(uv + off, vec2f(0.0), vec2f(1.0));
        let d = textureSampleLevel(shadowMap, shadowSamp, s, 0.0).r;
        let cmp = receiverDepth + dot(off, planeAt) - bias;
        if (d < cmp) {
            blockerDepthSum += cmp - d;
            blockerCount += 1.0;
        }
    }

    // The receiver is clear: fully lit, and the filter is skipped.
    if (blockerCount < 0.5) { return 1.0; }

    // ---- penumbra estimate ------------------------------------------------ Similar
    // triangles, in metres.
    let blockerDist = (blockerDepthSum / blockerCount) * depthRange;
    let penumbraWorld = blockerDist * 0.0093 * softness;
    let filterR = clamp(penumbraWorld / orthoWidth, texelSize, maxPenumbraUV);

    // ---- filter -----------------------------------------------------------
    var lit = 0.0;
    for (var i = 0; i < 12; i++) {
        let off = rot * POISSON[i] * filterR;
        let s = clamp(uv + off, vec2f(0.0), vec2f(1.0));
        let d = textureSampleLevel(shadowMap, shadowSamp, s, 0.0).r;
        let cmp = receiverDepth + dot(off, planeAt) - bias;
        lit += select(1.0, 0.0, d < cmp);
    }

    return lit / 12.0;
}

// ---------------------------------------------------------------- SH ambient

// Irradiance from 9 spherical-harmonic coefficients, the standard / Ramamoorthi and
// Hanrahan convolution.
fn shIrradiance(n: vec3f, sh: array<vec4f, 9>) -> vec3f {
    let c1 = 0.429043;
    let c2 = 0.511664;
    let c3 = 0.743125;
    let c4 = 0.886227;
    let c5 = 0.247708;

    return
        sh[0].rgb * c4
        + sh[1].rgb * 2.0 * c2 * n.y
        + sh[2].rgb * 2.0 * c2 * n.z
        + sh[3].rgb * 2.0 * c2 * n.x
        + sh[4].rgb * 2.0 * c1 * n.x * n.y
        + sh[5].rgb * 2.0 * c1 * n.y * n.z
        + sh[6].rgb * (c3 * n.z * n.z - c5)
        + sh[7].rgb * 2.0 * c1 * n.x * n.z
        + sh[8].rgb * c1 * (n.x * n.x - n.y * n.y);
}

// ------------------------------------------------------------------- helpers

// Reoriented normal mapping: blends a detail normal onto a base normal without / the
// base's tilt being lost, unlike a naive add-and-normalise.
fn blendNormalRNM(base: vec3f, detail: vec3f) -> vec3f {
    let t = base + vec3f(0.0, 0.0, 1.0);
    let u = detail * vec3f(-1.0, -1.0, 1.0);
    return normalize(t * dot(t, u) - u * t.z);
}

// Build a world normal from a heightfield gradient.
fn normalFromGradient(d: vec2f) -> vec3f {
    return normalize(vec3f(-d.x, 1.0, -d.y));
}

// Luminance, Rec.
fn luma(c: vec3f) -> f32 {
    return dot(c, vec3f(0.2126, 0.7152, 0.0722));
}
