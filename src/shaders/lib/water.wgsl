#define_import_path snow::water

#import snow::noise::noise2
#import snow::wake::wakeSection

// The shape of a bent water body: a swept surface along a spine, one strand per
// live spell, placed from a small data texture.
//
// Three rows per strand, with strand s at rows 3s to 3s+2:
//   row 0   position, radius in metres
//   row 1   parallel-transported reference right, twist
//   row 2   distance along in metres, age, foam, flatten

fn waterTexel(tex: texture_2d<f32>, row: i32, column: i32) -> vec4f {
    return textureLoad(tex, vec2i(column, row), 0);
}

/// Interpolated row, Catmull-Rom through the samples.
///
/// Not smoothstep: its derivative is zero at every knot, and a normal differenced
/// out of a radius interpolated that way rings at the sample pitch.
fn waterRow(tex: texture_2d<f32>, row: i32, count: f32, u: f32) -> vec4f {
    let n = max(count, 2.0);
    let f = clamp(u, 0.0, 1.0) * (n - 1.0);
    let first = i32(floor(f));
    let t = f - f32(first);
    let last = i32(n) - 1;

    let p0 = waterTexel(tex, row, max(first - 1, 0));
    let p1 = waterTexel(tex, row, first);
    let p2 = waterTexel(tex, row, min(first + 1, last));
    let p3 = waterTexel(tex, row, min(first + 2, last));

    let t2 = t * t;
    let t3 = t2 * t;
    return 0.5
        * ((2.0 * p1) + (-p0 + p2) * t + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2
            + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t3);
}

fn waterSpine(tex: texture_2d<f32>, base: i32, count: f32, u: f32) -> vec3f {
    return waterRow(tex, base, count, u).xyz;
}

/// Exact derivative of the spine spline.
///
/// Not a finite difference: sampling a fraction of a knot spacing away gives a
/// chord whose error depends on where in the span it starts, so the frame wobbles
/// once per spine sample and scallops the tube at every knot.
fn waterSpineTangent(tex: texture_2d<f32>, base: i32, count: f32, u: f32) -> vec3f {
    let n = max(count, 2.0);
    let f = clamp(u, 0.0, 1.0) * (n - 1.0);
    let first = i32(floor(f));
    let t = f - f32(first);
    let last = i32(n) - 1;

    let p0 = waterTexel(tex, base, max(first - 1, 0)).xyz;
    let p1 = waterTexel(tex, base, first).xyz;
    let p2 = waterTexel(tex, base, min(first + 1, last)).xyz;
    let p3 = waterTexel(tex, base, min(first + 2, last)).xyz;

    let derivative = 0.5
        * ((-p0 + p2) + 2.0 * (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t
            + 3.0 * (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t * t);
    let length = length(derivative);
    return select(vec3f(0.0, 1.0, 0.0), derivative / length, length > 1e-7);
}

/// Surface relief, in cycles per strand rather than per metre.
///
/// The lattice carries a fixed vertex count whatever the strand is doing, so
/// keying the field to world distance puts a long strand past Nyquist and the
/// field beats instead of adding detail. Sampled around a circle rather than
/// along the angle, because a tube is closed and plain noise runs on forever, so
/// feeding the angle in directly creases every tube along its length.
fn waterRelief(u: f32, theta: f32, time: f32) -> f32 {
    let circle = vec2f(cos(theta), sin(theta));
    return noise2(circle * 0.85 + vec2f(u * 4.0 - time * 1.6, u * 2.3)) * 0.60
        + noise2(circle * 1.50 + vec2f(u * 7.5 + 11.3, -u * 5.1 - time * 3.1)) * 0.40;
}

/// The same field for an open section, which has no seam to close.
fn waterReliefOpen(u: f32, v: f32, time: f32) -> f32 {
    return noise2(vec2f(u * 4.0 - time * 1.6, v * 2.60)) * 0.60
        + noise2(vec2f(u * 7.5 - time * 3.1, v * 4.40 + 11.3)) * 0.40;
}

/// A point on the strand surface, with `u` from head to tail and `q` around the
/// section for a tube or up the face for a sheet.
fn waterPoint(
    tex: texture_2d<f32>,
    base: i32,
    count: f32,
    profile: f32,
    u: f32,
    q: f32,
    time: f32
) -> vec3f {
    let place = waterRow(tex, base, count, u);
    let frame = waterRow(tex, base + 1, count, u);
    let shaping = waterRow(tex, base + 2, count, u);

    let spine = place.xyz;
    let radius = place.w;
    let flatten = max(shaping.w, 0.02);
    let tangent = waterSpineTangent(tex, base, count, u);

    // The stored right is transported on the processor, which stops the section
    // spinning as the spine curves. Re-orthogonalised because interpolating two
    // transported frames does not preserve the right angle exactly.
    var right = frame.xyz - tangent * dot(frame.xyz, tangent);
    let rightLength = length(right);
    right = select(
        normalize(cross(tangent, vec3f(0.0, 0.0, 1.0)) + vec3f(1e-5, 0.0, 0.0)),
        right / max(rightLength, 1e-8),
        rightLength > 1e-5
    );
    let up = cross(tangent, right);

    if (profile < 0.5) {
        let theta = q * 6.28318530718 + frame.w;
        // Scaled by the radius, so a thin trailing wisp carries less of the
        // same lumps as a metre-wide column.
        let relief = waterRelief(clamp(u, 0.0, 1.0), theta, time);
        let swollen = radius * (1.0 + relief * 0.22);
        return spine + right * (cos(theta) * swollen) + up * (sin(theta) * swollen * flatten);
    }

    // The wake's own section, so a crescent of slush and a carve's wall of snow
    // are one description at two scales.
    let section = wakeSection(q, frame.w);
    let relief =
        waterReliefOpen(clamp(u, 0.0, 1.0), q * 3.0, time) * 0.13 * smoothstep(0.1, 0.7, q);
    return spine
        + right * ((section.x + relief) * radius)
        + vec3f(0.0, (section.y + relief * 0.5) * radius * flatten, 0.0);
}
