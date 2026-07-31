#define_import_path snow::wake

#import snow::noise::noise2

// The shape of the snow-surf wake.

// Steps in the cross-section integral.
const WAKE_STEPS: i32 = 20;

// Normalises the integral so the crest lands near one, which is what lets the /
// amplitude be a single number in metres rather than a per-curl table.
const WAKE_NORM: f32 = 3.35;

// The section is squashed across rather than scaled uniformly.
const WAKE_LATERAL: f32 = 0.70;

// Cross-section of a breaking wave, in lateral and up, at unit height.
fn wakeSection(q: f32, curl: f32) -> vec2f {
    let start = -0.24;
    let end = 1.65 + curl * 3.30;
    var point = vec2f(0.0, 0.0);
    let step = q / f32(WAKE_STEPS);
    for (var index = 0; index < WAKE_STEPS; index++) {
        let t = (f32(index) + 0.5) * step;
        let angle = start + (end - start) * pow(t, 1.65);
        // The section thins as it climbs, so the lip is fine and the base broad.
        point += vec2f(cos(angle), sin(angle)) * (1.0 - 0.40 * t) * step;
    }
    return vec2f(point.x * WAKE_LATERAL, point.y) * WAKE_NORM;
}

// Per-side scalars at spine parameter `u`, interpolated with a smoothstep / weight
// rather than linearly.
fn wakeScalars(tex: texture_2d<f32>, count: f32, u: f32, side: f32) -> vec4f {
    let n = max(count, 2.0);
    let f = clamp(u, 0.0, 1.0) * (n - 1.0);
    let first = i32(floor(f));
    let second = min(first + 1, i32(n) - 1);
    let blend = smoothstep(0.0, 1.0, f - f32(first));

    let basisA = textureLoad(tex, vec2i(first, 1), 0);
    let basisB = textureLoad(tex, vec2i(second, 1), 0);
    let shapeA = textureLoad(tex, vec2i(first, 2), 0);
    let shapeB = textureLoad(tex, vec2i(second, 2), 0);
    let distA = textureLoad(tex, vec2i(first, 0), 0).w;
    let distB = textureLoad(tex, vec2i(second, 0), 0).w;

    let left = side < 0.0;
    let amplitude = select(mix(basisA.w, basisB.w, blend), mix(basisA.z, basisB.z, blend), left);
    let curl = select(mix(shapeA.y, shapeB.y, blend), mix(shapeA.x, shapeB.x, blend), left);
    return vec4f(
        amplitude,
        curl,
        mix(distA, distB, blend),
        mix(shapeA.z, shapeB.z, blend)
    );
}

// Spine position at `u`, Catmull-Rom through the samples.
fn wakeSpine(tex: texture_2d<f32>, count: f32, u: f32) -> vec3f {
    let n = max(count, 2.0);
    let f = clamp(u, 0.0, 1.0) * (n - 1.0);
    let first = i32(floor(f));
    let t = f - f32(first);
    let last = i32(n) - 1;

    let p0 = textureLoad(tex, vec2i(max(first - 1, 0), 0), 0).xyz;
    let p1 = textureLoad(tex, vec2i(first, 0), 0).xyz;
    let p2 = textureLoad(tex, vec2i(min(first + 1, last), 0), 0).xyz;
    let p3 = textureLoad(tex, vec2i(min(first + 2, last), 0), 0).xyz;

    let t2 = t * t;
    let t3 = t2 * t;
    return 0.5
        * ((2.0 * p1) + (-p0 + p2) * t + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2
            + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t3);
}

// The wake surface.
fn wakePoint(tex: texture_2d<f32>, count: f32, u: f32, q: f32, side: f32, time: f32) -> vec3f {
    let scalars = wakeScalars(tex, count, u, side);
    let spine = wakeSpine(tex, count, u);

    let sample = i32(clamp(u, 0.0, 1.0) * (max(count, 2.0) - 1.0));
    let flat = normalize(textureLoad(tex, vec2i(sample, 1), 0).xy);
    let right = vec3f(flat.x, 0.0, flat.y);
    let forward = vec3f(-flat.y, 0.0, flat.x);

    let section = wakeSection(q, scalars.y);

    // The two walls start close together at the bow and spread behind it, which is what
    // makes the pair read as a bow wave splitting around the board rather than as two
    // unrelated banks.
    let base = 0.24 + 0.44 * smoothstep(0.3, 2.6, scalars.z);

    // Thrown snow curls, so the tangent turns along the section.
    let tangentAngle = -0.24 + (1.89 + scalars.y * 3.30) * pow(q, 1.65);
    let sectionNormal = vec2f(-sin(tangentAngle), cos(tangentAngle));
    let lump = (noise2(vec2f(
        scalars.z * 1.13 + q * 0.9 + side * 17.3,
        q * 1.7 + 5.1 + time * 0.30
    )) * 0.55
        + noise2(vec2f(
            scalars.z * 3.31 - q * 1.7 + side * 31.7 - time * 0.45,
            q * 4.3 + 2.7
        )) * 0.30
        + noise2(vec2f(scalars.z * 8.7 + side * 5.3, q * 9.1 + time * 0.9)) * 0.15)
        * 0.085
        * smoothstep(0.12, 0.72, q);

    let lateral = base + (section.x + sectionNormal.x * WAKE_LATERAL * lump) * scalars.x;

    // Thrown snow lags the thing that threw it, so the lip trails backward along the
    // spine.
    let trail = -q * q * 0.34 * scalars.x;

    // Sunk, because the base has to meet a trench floor and a berm crest that the
    // spine's recorded ground height knows nothing about.
    return spine
        + right * (side * lateral)
        + vec3f(0.0, (section.y + sectionNormal.y * lump) * scalars.x - 0.10, 0.0)
        + forward * trail;
}

// True where the wake has broken up into airborne powder and there is no / surface
// left to draw.
fn wakeEroded(alongDist: f32, q: f32, age01: f32, time: f32) -> bool {
    let threshold = smoothstep(0.84, 1.06, q) * mix(0.34, 0.70, age01)
        + smoothstep(0.68, 1.0, age01) * 0.95;
    if (threshold <= 0.001) { return false; }

    // Thirteen cells across the section rather than four and a half: any coarser across
    // and the noise is effectively one dimensional, so the holes come out as a row of
    // vertical slots, a comb rather than a breakup.
    let p = vec2f(alongDist, q);
    let coarse = noise2(vec2f(
        p.x * 6.9 + p.y * 3.1 + time * 0.9,
        p.y * 13.0 - p.x * 2.2 - time * 0.6
    )) * 0.72 + 0.5;
    let fine = noise2(vec2f(
        p.x * 19.0 - p.y * 9.0 + 31.7 - time * 3.1,
        p.y * 31.0 + p.x * 7.0 + time * 2.3
    )) * 0.72 + 0.5;
    return (coarse * 0.58 + fine * 0.42) < threshold;
}
