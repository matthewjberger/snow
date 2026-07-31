#define_import_path snow::crystal

#import snow::noise::{hash21, hash22}

// The shape of a grown ice formation: a six-sided tapered prism with a point on
// it, and deliberately nothing more. The read comes from the cluster, from the
// light through it, and from the fact that it grows.
//
// Three rows, one column per crystal:
//   row 0   position, height in metres
//   row 1   growth axis, base radius in metres
//   row 2   growth, seed

/// Vertices per crystal: two rings of six plus an apex.
const CRYSTAL_RING: i32 = 6;
const CRYSTAL_VERTS: i32 = 13;

/// Local position of a vertex, in the crystal's own frame with the growth axis
/// up.
///
/// The seed breaks the hexagon: each of the six radial directions gets its own
/// length, so no two crystals in a cluster share a silhouette and none of them is
/// a regular hexagon, which reads as manufactured immediately.
fn crystalLocal(v: i32, height: f32, radius: f32, seed: f32) -> vec3f {
    if (v >= CRYSTAL_VERTS - 1) {
        // Apex, nudged off the axis so the point is not perfectly centred.
        let jitter = hash22(vec2f(seed, 7.31)) - 0.5;
        return vec3f(jitter.x * radius * 0.5, height, jitter.y * radius * 0.5);
    }

    let ring = v / CRYSTAL_RING;
    let k = v - ring * CRYSTAL_RING;
    let angle = f32(k) * 1.04719755 + seed * 6.2831853;
    let wobble = 0.72 + 0.56 * hash21(vec2f(f32(k) + seed * 31.0, seed * 17.0));

    let r = select(radius * wobble, radius * wobble * 0.68, ring == 1);
    let y = select(0.0, height * 0.58, ring == 1);
    return vec3f(cos(angle) * r, y, sin(angle) * r);
}

/// World position of a vertex of one crystal.
fn crystalPoint(tex: texture_2d<f32>, index: i32, v: i32) -> vec3f {
    let place = textureLoad(tex, vec2i(index, 0), 0);
    let frame = textureLoad(tex, vec2i(index, 1), 0);
    let state = textureLoad(tex, vec2i(index, 2), 0);

    let growth = clamp(state.x, 0.0, 1.0);
    // Height leads and girth follows. A crystal that scales uniformly reads as a
    // model being interpolated in; one that spears up and then thickens reads as
    // ice forming, because that is what ice does.
    let tall = growth * growth * (3.0 - 2.0 * growth);
    let fat = smoothstep(0.25, 1.0, growth);
    let height = place.w * tall;
    let radius = frame.w * (0.22 + 0.78 * fat);

    let local = crystalLocal(v, height, radius, state.y);

    // Any stable perpendicular will do, since the shape is already randomised
    // about the axis by the seed.
    let axis = normalize(select(frame.xyz, vec3f(0.0, 1.0, 0.0), dot(frame.xyz, frame.xyz) < 1e-6));
    let reference = select(vec3f(0.0, 0.0, 1.0), vec3f(1.0, 0.0, 0.0), abs(axis.y) < 0.9);
    let across = normalize(cross(reference, axis));
    let along = cross(axis, across);

    return place.xyz + across * local.x + axis * local.y + along * local.z;
}
