#define_import_path snow::char_skin

// The character's shared vertex-side transform library.

// Skins a point by one bone.
fn skinPoint1(tex: texture_2d<f32>, bone: i32, p: vec3f) -> vec3f {
    let c0 = textureLoad(tex, vec2i(bone, 0), 0);
    let c1 = textureLoad(tex, vec2i(bone, 1), 0);
    let c2 = textureLoad(tex, vec2i(bone, 2), 0);
    let c3 = textureLoad(tex, vec2i(bone, 3), 0);
    return c0.xyz * p.x + c1.xyz * p.y + c2.xyz * p.z + c3.xyz;
}

// Skins a direction by one bone, ignoring the translation.
fn skinDir1(tex: texture_2d<f32>, bone: i32, d: vec3f) -> vec3f {
    let c0 = textureLoad(tex, vec2i(bone, 0), 0);
    let c1 = textureLoad(tex, vec2i(bone, 1), 0);
    let c2 = textureLoad(tex, vec2i(bone, 2), 0);
    return c0.xyz * d.x + c1.xyz * d.y + c2.xyz * d.z;
}

// Two-influence linear blend skinning.
fn skinPoint(tex: texture_2d<f32>, index: vec4f, weight: vec4f, p: vec3f) -> vec3f {
    var r = skinPoint1(tex, i32(index.x), p) * weight.x;
    if (weight.y > 0.0001) { r += skinPoint1(tex, i32(index.y), p) * weight.y; }
    return r / max(1e-4, weight.x + weight.y);
}

fn skinNormal(tex: texture_2d<f32>, index: vec4f, weight: vec4f, n: vec3f) -> vec3f {
    var r = skinDir1(tex, i32(index.x), n) * weight.x;
    if (weight.y > 0.0001) { r += skinDir1(tex, i32(index.y), n) * weight.y; }
    return normalize(r);
}

// ------------------------------------------------------------- cloth sampling

// One simulated node.
fn clothNode(
    tex: texture_2d<f32>,
    rowBase: i32,
    cols: i32,
    rows: i32,
    i: i32,
    j: i32
) -> vec3f {
    let column = (i % cols + cols) % cols;
    let row = clamp(j, 0, rows - 1);
    return textureLoad(tex, vec2i(column, rowBase + row), 0).xyz;
}

fn crBasis(t: f32) -> vec4f {
    let t2 = t * t;
    let t3 = t2 * t;
    return vec4f(
        0.5 * (-t3 + 2.0 * t2 - t),
        0.5 * (3.0 * t3 - 5.0 * t2 + 2.0),
        0.5 * (-3.0 * t3 + 4.0 * t2 + t),
        0.5 * (t3 - t2)
    );
}

fn crDeriv(t: f32) -> vec4f {
    let t2 = t * t;
    return vec4f(
        0.5 * (-3.0 * t2 + 4.0 * t - 1.0),
        0.5 * (9.0 * t2 - 10.0 * t),
        0.5 * (-9.0 * t2 + 8.0 * t + 1.0),
        0.5 * (3.0 * t2 - 2.0 * t)
    );
}

struct ClothSample {
    pos: vec3f,
    nrm: vec3f,
    tanU: vec3f,
}

// Reconstructs a smooth garment surface from its simulated grid.
fn sampleCloth(
    tex: texture_2d<f32>,
    rowBase: i32,
    cols: i32,
    rows: i32,
    u: f32,
    v: f32
) -> ClothSample {
    let gu = u * f32(cols);
    let gv = v * f32(rows - 1);
    let fu = floor(gu);
    let fv = floor(gv);
    let i0 = i32(fu) - 1;
    let j0 = i32(fv) - 1;

    let wu = crBasis(gu - fu);
    let du = crDeriv(gu - fu);
    let wv = crBasis(gv - fv);
    let dv = crDeriv(gv - fv);

    var p = vec3f(0.0);
    var pu = vec3f(0.0);
    var pv = vec3f(0.0);

    for (var j = 0; j < 4; j++) {
        var rowP = vec3f(0.0);
        var rowD = vec3f(0.0);
        for (var i = 0; i < 4; i++) {
            let q = clothNode(tex, rowBase, cols, rows, i0 + i, j0 + j);
            rowP += q * wu[i];
            rowD += q * du[i];
        }
        p += rowP * wv[j];
        pu += rowD * wv[j];
        pv += rowP * dv[j];
    }

    var out: ClothSample;
    out.pos = p;
    // Ordered so the result points away from the body: u runs anticlockwise around the
    // tube and v runs down it.
    out.nrm = normalize(cross(pv, pu));
    out.tanU = normalize(pu);
    return out;
}
