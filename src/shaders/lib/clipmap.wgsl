#define_import_path snow::clipmap

// Vertex placement and displacement for the nested-ring terrain.

// Bicubic B-spline height fetch, via the four-bilinear-tap trick.
fn sampleHeightBicubic(tex: texture_2d<f32>, samp: sampler, uv: vec2f, res: f32) -> f32 {
    let coord = uv * res - 0.5;
    let base = floor(coord);
    let f = coord - base;

    let f2 = f * f;
    let f3 = f2 * f;
    let w0 = (1.0 - 3.0 * f + 3.0 * f2 - f3) / 6.0;
    let w1 = (4.0 - 6.0 * f2 + 3.0 * f3) / 6.0;
    let w2 = (1.0 + 3.0 * f + 3.0 * f2 - 3.0 * f3) / 6.0;
    let w3 = f3 / 6.0;

    let s0 = w0 + w1;
    let s1 = w2 + w3;
    let o0 = (base + 0.5 - 1.0 + w1 / s0) / res;
    let o1 = (base + 0.5 + 1.0 + w3 / s1) / res;

    let t00 = textureSampleLevel(tex, samp, vec2f(o0.x, o0.y), 0.0).r;
    let t10 = textureSampleLevel(tex, samp, vec2f(o1.x, o0.y), 0.0).r;
    let t01 = textureSampleLevel(tex, samp, vec2f(o0.x, o1.y), 0.0).r;
    let t11 = textureSampleLevel(tex, samp, vec2f(o1.x, o1.y), 0.0).r;

    return mix(mix(t00, t10, s1.x), mix(t01, t11, s1.x), s1.y);
}

// World XZ to height-texture UV.
fn worldToHeightUV(p: vec2f, origin: vec2f, size: f32) -> vec2f {
    return (p - origin) / size;
}

struct ClipmapVertex {
    worldXZ: vec2f,
    // This vertex's effective sample spacing, post-morph.
    spacing: f32,
    morph: f32,
}

// Place a clipmap vertex in world space.
fn placeClipmapVertex(
    grid: vec2f,
    level: f32,
    camXZ: vec2f,
    baseSpacing: f32,
    gridHalfN: f32
) -> ClipmapVertex {
    let spacing = baseSpacing * exp2(level);

    // Snap the ring origin to twice this level's spacing.
    let snap = spacing * 2.0;
    let origin = floor(camXZ / snap) * snap;

    var local = grid * spacing;

    // ---- morph toward the coarser lattice -------------------------------- Chebyshev
    // distance, because the rings are square.
    let extent = gridHalfN * spacing;
    let cheb = max(abs(local.x), abs(local.y)) / extent;

    // Completes at 0.86, comfortably before the overlap band where this ring and the
    // next coarser one both draw.
    let morph = clamp((cheb - 0.70) / 0.16, 0.0, 1.0);

    // The coarse lattice is every second vertex of this one.
    let coarseGrid = floor(grid * 0.5) * 2.0;
    let coarseLocal = coarseGrid * spacing;
    local = mix(local, coarseLocal, morph);

    var out: ClipmapVertex;
    out.worldXZ = origin + local;
    out.spacing = spacing * (1.0 + morph);
    out.morph = morph;
    return out;
}
