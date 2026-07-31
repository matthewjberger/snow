#define_import_path snow::ridge

#import snow::noise::{fbmd, noised, ridgedd}

// The far-field mountains.

// Tallest a peak can be, in metres.
fn ridgeCeiling(amp: f32) -> f32 {
    return amp * 1.05;
}

// Height of the range at a world XZ, in metres, with its analytic gradient.
fn ridgeField(p: vec2f, amp: f32) -> vec3f {
    // Kilometres.
    let q = p * 0.001;
    let kq = 0.001;

    // ---- the bowl ---------------------------------------------------------- The
    // range is excluded from a seven-kilometre disc centred on the world origin, and
    // the player is confined well inside it, so the field is guaranteed to be empty
    // everywhere the march begins.
    let rad = length(p);
    let bt = clamp((rad - 7000.0) / 6000.0, 0.0, 1.0);
    let bowl = bt * bt * (3.0 - 2.0 * bt);
    if (bowl <= 0.0) { return vec3f(0.0); }
    let dbowl = select(
        vec2f(0.0),
        (p / max(rad, 1.0)) * (6.0 * bt * (1.0 - bt) / 6000.0),
        bt > 0.0 && bt < 1.0
    );

    // ---- where there is a range at all ------------------------------------
    let massif = fbmd(q * 0.10 + vec2f(11.3, 4.7), 2, 2.13, 0.52);
    let mk = 0.10 * kq;
    let t = clamp((massif.x + 0.34) / 0.70, 0.0, 1.0);
    let env = t * t * (3.0 - 2.0 * t);
    // The smoothstep's own derivative, chained through the massif's slope.
    let denv = select(
        vec2f(0.0),
        massif.yz * mk * (6.0 * t * (1.0 - t) / 0.62),
        t > 0.0 && t < 1.0
    );

    // ---- domain warp ------------------------------------------------------- The
    // single largest difference between ridged noise and mountains.
    let w1 = noised(q * 0.26 + vec2f(2.7, 8.1));
    let w2 = noised(q * 0.26 + vec2f(19.4, 3.6));
    let qw = q + vec2f(w1.x, w2.x) * 1.35;

    // ---- the peaks --------------------------------------------------------- Four
    // octaves, not three.
    let r = ridgedd(qw * 0.30, 4, 2.09, 0.50);
    let rk = 0.30 * kq;
    // A second, finer set at a different phase.
    let s = ridgedd(qw * 1.05 + vec2f(31.0, 17.0), 3, 2.11, 0.50);
    let sk = 1.05 * kq;

    let raw = r.x * 0.78 + s.x * 0.22;
    let draw = r.yz * (0.78 * rk) + s.yz * (0.22 * sk);

    // Sharpen the crests and widen the valleys.
    let peaks = raw * raw * raw * 0.55 + raw * 0.45;
    let dpeaks = draw * (3.0 * raw * raw * 0.55 + 0.45);

    // A small floor under the envelope: low foothills in the gaps between massifs
    // rather than absolute nothing, which reads as a cut-out.
    let e = 0.06 + 0.94 * env;
    let h = peaks * e;
    let dh = dpeaks * e + peaks * denv * 0.94;
    return vec3f(
        h * bowl * amp,
        (dh * bowl + h * dbowl) * amp
    );
}

// Earth curvature drop at a horizontal distance, in metres.
fn ridgeDrop(d: f32) -> f32 {
    return d * d / 12742000.0;
}

struct RidgeHit {
    hit: bool,
    // Horizontal metres to the hit.
    dist: f32,
    // World Y of the surface there.
    height: f32,
    normal: vec3f,
    // World XZ of the hit.
    pos: vec2f,
}

// March the range along a view ray.
fn ridgeMarch(camPos: vec3f, dir: vec3f, amp: f32) -> RidgeHit {
    var out: RidgeHit;
    out.hit = false;
    out.dist = 0.0;
    out.height = 0.0;
    out.normal = vec3f(0.0, 1.0, 0.0);
    out.pos = vec2f(0.0);

    let hl = length(dir.xz);
    if (hl < 1e-4) { return out; }

    let step = dir.xz / hl;
    // Metres of rise per metre of ground.
    let slope = dir.y / hl;

    // Where the range starts, set by how large a massif should read rather than by
    // taste: a 1.8 km peak at 9 km subtends about eleven degrees, which is roughly what
    // a real range does across a frame.
    const D_NEAR: f32 = 5500.0;
    const D_FAR: f32 = 45000.0;
    const STEPS: i32 = 18;

    // A ray already above the tallest possible peak and still climbing can never hit.
    let ceiling = ridgeCeiling(amp);
    if (camPos.y + slope * D_NEAR > ceiling && slope >= 0.0) { return out; }

    let growth = pow(D_FAR / D_NEAR, 1.0 / f32(STEPS));

    // Prime the crossing state from a real sample rather than a constant.
    var prevD = D_NEAR;
    var prevGap = camPos.y + slope * D_NEAR
                - (ridgeField(camPos.xz + step * D_NEAR, amp).x - ridgeDrop(D_NEAR));

    if (prevGap < 0.0) {
        // Started inside the near face, which is a legitimate hit at the near distance.
        out.dist = D_NEAR;
        out.pos = camPos.xz + step * D_NEAR;
        let f = ridgeField(out.pos, amp);
        out.height = f.x - ridgeDrop(D_NEAR);
        out.normal = normalize(vec3f(-f.y, 1.0, -f.z));
        out.hit = true;
        return out;
    }

    var d = D_NEAR * growth;

    for (var i = 1; i < STEPS; i++) {
        let p = camPos.xz + step * d;
        let h = ridgeField(p, amp).x - ridgeDrop(d);
        let rayY = camPos.y + slope * d;
        let gap = rayY - h;

        if (gap < 0.0) {
            // Interpolate the crossing rather than accepting the step.
            var t = 0.5;
            if (prevGap - gap > 1e-5) { t = prevGap / (prevGap - gap); }
            out.dist = mix(prevD, d, clamp(t, 0.0, 1.0));
            out.pos = camPos.xz + step * out.dist;

            let f = ridgeField(out.pos, amp);
            out.height = f.x - ridgeDrop(out.dist);
            out.normal = normalize(vec3f(-f.y, 1.0, -f.z));
            out.hit = true;
            return out;
        }

        // Climbed clear of the tallest possible peak: nothing ahead can be hit.
        if (rayY > ceiling && slope > 0.0) { return out; }

        prevGap = gap;
        prevD = d;
        d *= growth;
    }

    return out;
}

// Fraction of the sun reaching a point on the range, marched along the sun /
// direction.
fn ridgeShadow(pos: vec2f, height: f32, sunDir: vec3f, amp: f32) -> f32 {
    let hl = length(sunDir.xz);
    if (hl < 1e-3 || sunDir.y <= 0.0) { return 1.0; }

    let step = sunDir.xz / hl;
    let slope = sunDir.y / hl;

    var d = 420.0;
    for (var i = 0; i < 4; i++) {
        let h = ridgeField(pos + step * d, amp).x;
        if (h > height + slope * d) { return 0.0; }
        d *= 2.6;
    }
    return 1.0;
}
