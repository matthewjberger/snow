use crate::systems::terrain;
use crate::systems::terrain::Heightfield;

/// A bell that is zero at both ends and one in the middle, with zero slope
/// everywhere it matters.
///
/// Every amplitude envelope in the spells uses it. A linear ramp leaves a visible
/// corner at both ends of every arc, and the grammar here is no instant spawns
/// and no instant despawns.
pub fn bell(t: f32) -> f32 {
    let sine = (std::f32::consts::PI * t.clamp(0.0, 1.0)).sin();
    sine * sine
}

/// Hermite smoothstep on an already normalised parameter.
pub fn smooth01(t: f32) -> f32 {
    let x = t.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// Rotates a reference vector by the minimal rotation taking one tangent to the
/// next, which is what keeps a swept section from spinning as its spine curves.
///
/// A Frenet frame is undefined wherever the spine is momentarily straight and
/// flips through half a turn at every inflection, which a figure eight has two of
/// by definition. An up-referenced frame is degenerate wherever the spine passes
/// through vertical, which a ribbon thrown overhead does constantly. Parallel
/// transport has no degeneracy at all.
pub fn transport(right: [f32; 3], from: [f32; 3], to: [f32; 3]) -> [f32; 3] {
    let mut axis = [
        from[1] * to[2] - from[2] * to[1],
        from[2] * to[0] - from[0] * to[2],
        from[0] * to[1] - from[1] * to[0],
    ];
    let sine = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();

    // Parallel, or antiparallel. Antiparallel cannot happen between adjacent
    // samples of a spine laid every few centimetres, so this is the straight line
    // case and the frame carries over.
    if sine < 1e-7 {
        return right;
    }
    axis = [axis[0] / sine, axis[1] / sine, axis[2] / sine];

    let cosine = from[0] * to[0] + from[1] * to[1] + from[2] * to[2];
    let angle = sine.atan2(cosine);
    let (turn_sine, turn_cosine) = angle.sin_cos();

    let projection = axis[0] * right[0] + axis[1] * right[1] + axis[2] * right[2];
    let cross = [
        axis[1] * right[2] - axis[2] * right[1],
        axis[2] * right[0] - axis[0] * right[2],
        axis[0] * right[1] - axis[1] * right[0],
    ];

    let mut turned = [0.0_f32; 3];
    for component in 0..3 {
        turned[component] = right[component] * turn_cosine
            + cross[component] * turn_sine
            + axis[component] * projection * (1.0 - turn_cosine);
    }
    let length = (turned[0] * turned[0] + turned[1] * turned[1] + turned[2] * turned[2])
        .sqrt()
        .max(1e-6);
    [turned[0] / length, turned[1] / length, turned[2] / length]
}

/// Where a ray meets the snow, as a distance along it, or nothing for a miss.
///
/// A coarse march then a bisection refine against the height mirror. The march
/// does not stop at the first sample below the surface if the ray starts below
/// it: a targeting ray fired from inside a drift should come out of the drift and
/// hit the far side rather than report a hit at the origin.
pub fn ground_ray(
    heightfield: &Heightfield,
    origin: [f32; 3],
    direction: [f32; 3],
    max_distance: f32,
) -> Option<f32> {
    const STEP: f32 = 0.6;
    let above_at = |distance: f32| {
        origin[1] + direction[1] * distance
            - terrain::height_at(
                heightfield,
                origin[0] + direction[0] * distance,
                origin[2] + direction[2] * distance,
            )
    };

    let mut previous = 0.0_f32;
    let mut travelled = STEP;
    while travelled <= max_distance {
        if above_at(travelled) <= 0.0 {
            let mut low = previous;
            let mut high = travelled;
            for _ in 0..8 {
                let middle = (low + high) * 0.5;
                if above_at(middle) > 0.0 {
                    low = middle;
                } else {
                    high = middle;
                }
            }
            return Some((low + high) * 0.5);
        }
        previous = travelled;
        travelled += STEP;
    }
    None
}

/// Where the player is aiming, on the ground.
///
/// Falls back to a fixed distance ahead when the ray misses, because looking at
/// the sky and pressing a key has to do something, and putting the effect a sane
/// distance in front of the player is the only answer that is never surprising.
pub fn aim_point(
    heightfield: &Heightfield,
    origin: [f32; 3],
    direction: [f32; 3],
    max_distance: f32,
    fallback: f32,
) -> [f32; 3] {
    if let Some(distance) = ground_ray(heightfield, origin, direction, max_distance) {
        return [
            origin[0] + direction[0] * distance,
            origin[1] + direction[1] * distance,
            origin[2] + direction[2] * distance,
        ];
    }
    let flat = (direction[0] * direction[0] + direction[2] * direction[2])
        .sqrt()
        .max(1e-6);
    let x = origin[0] + (direction[0] / flat) * fallback;
    let z = origin[2] + (direction[2] / flat) * fallback;
    [x, terrain::height_at(heightfield, x, z), z]
}
