use crate::ecs::{VORTEX, Vortex};
use crate::systems::deform;
use crate::systems::deform::Brush;
use crate::systems::spell::bending::{bell, smooth01, transport};
use crate::systems::spell::lights;
use crate::systems::spell::water;
use crate::systems::spell::water::{PROFILE_TUBE, WaterBody};
use crate::systems::spell::{self, Cast};
use crate::systems::spray;
use crate::systems::terrain;
use nightshade::prelude::*;
use rand::Rng;

/// How many helices. Three reads as a spiral; two reads as a double helix.
const HELICES: usize = 3;

/// Spine samples per helix. This is the tightest curve anything here draws.
const COLS: usize = 64;

/// Seconds of full-strength spin, before the ease out.
const HOLD: f32 = 3.0;
const RAMP: f32 = 0.55;
const FADE: f32 = 1.1;

/// Height of the column, in metres.
const TOP: f32 = 4.8;

/// Turns each helix makes from the ground to the top.
const TURNS: f32 = 1.35;

/// Casting raises the column, or restarts the one already turning.
pub fn cast(world: &mut World, cast: &mut Cast) {
    let standing = spell::live::<Vortex>(world, VORTEX);
    let mut vortex = standing.map(|(_, vortex)| vortex).unwrap_or_default();
    begin(&mut vortex, cast.water);
    spell::commit(world, standing.map(|(entity, _)| entity), vortex);
}

/// Runs the column, despawning it once it has spun down.
pub fn update(world: &mut World, cast: &mut Cast, character: [f32; 2]) {
    let Some((entity, mut vortex)) = spell::live::<Vortex>(world, VORTEX) else {
        return;
    };
    if tick(&mut vortex, cast, character) {
        world.set(entity, vortex);
    } else {
        release(&mut vortex, cast.water);
        world.despawn_recursive(entity);
    }
}

/// Drops the column, for the settings toggle.
pub fn cancel_all(world: &mut World, water: &mut WaterBody) {
    let Some((entity, mut vortex)) = spell::live::<Vortex>(world, VORTEX) else {
        return;
    };
    release(&mut vortex, water);
    world.despawn_recursive(entity);
}

fn begin(vortex: &mut Vortex, water: &mut WaterBody) {
    for strand in &mut vortex.strands {
        if strand.is_none() {
            *strand = water::acquire(water);
        }
    }
    vortex.time = 0.0;
    vortex.ring = 0.9;
    vortex.strip_owed = 0.0;
    vortex.grain_owed = 0.0;
}

/// Advances the column, reporting whether it is still turning.
fn tick(vortex: &mut Vortex, cast: &mut Cast, character: [f32; 2]) -> bool {
    vortex.time += cast.delta_time;
    if vortex.time >= RAMP + HOLD + FADE {
        return false;
    }

    // The column follows the player. It is their vortex, and walking out of
    // it would be the single most effect-like thing it could do.
    vortex.centre = character;

    let envelope =
        smooth01(vortex.time / RAMP) * (1.0 - smooth01((vortex.time - RAMP - HOLD) / FADE));
    // Spins up and keeps spinning: the rotation does not ease out with the
    // envelope, so the last frame is a fading column that is still turning
    // rather than one that is winding down.
    vortex.spin += cast.delta_time * (5.2 + 2.4 * envelope);

    helices(vortex, cast, envelope);
    strip(vortex, cast, envelope);
    grains(vortex, cast, envelope);

    let ground = terrain::height_at(cast.heightfield, vortex.centre[0], vortex.centre[1]);
    lights::add(
        cast.lights,
        [vortex.centre[0], ground + 1.3, vortex.centre[1]],
        9.0,
        [0.46, 0.74, 1.0],
        9.0 * envelope,
    );
    true
}

/// The radius of the column at a height up it.
///
/// Wide at the bottom where it is picking snow up, narrower and faster at the
/// top. Not a cone: the waist is what makes it read as a vortex rather than
/// as a party hat.
fn radius_at(height: f32) -> f32 {
    (2.55 - 1.15 * height) * (0.78 + 0.34 * bell((height * 1.2).clamp(0.0, 1.0)))
}

fn helices(vortex: &mut Vortex, cast: &mut Cast, envelope: f32) {
    let ground = terrain::height_at(cast.heightfield, vortex.centre[0], vortex.centre[1]);

    for helix in 0..HELICES {
        let Some(strand) = vortex.strands[helix] else {
            continue;
        };
        let phase = (helix as f32 / HELICES as f32) * std::f32::consts::TAU;

        let mut previous = [0.0_f32; 3];
        let mut right = [0.0, 1.0, 0.0];
        let mut tangent = [1.0, 0.0, 0.0];
        let mut distance = 0.0_f32;

        for column in 0..COLS {
            let u = column as f32 / (COLS - 1) as f32;
            // Column zero is the top of the helix, the leading edge of the
            // lift, so the parameter runs downward like every other strand.
            let height = 1.0 - u;
            let angle = phase + vortex.spin + height * TURNS * std::f32::consts::TAU;
            let radius = radius_at(height);

            let point = [
                vortex.centre[0] + angle.cos() * radius,
                ground + TOP * height * envelope + 0.05,
                vortex.centre[1] + angle.sin() * radius,
            ];

            if column > 0 {
                let mut next = [
                    point[0] - previous[0],
                    point[1] - previous[1],
                    point[2] - previous[2],
                ];
                let length = (next[0] * next[0] + next[1] * next[1] + next[2] * next[2])
                    .sqrt()
                    .max(1e-4);
                distance += length;
                next = [next[0] / length, next[1] / length, next[2] / length];
                right = transport(right, tangent, next);
                tangent = next;
            }

            // Both ends taper to nothing: the top because the snow is
            // dispersing, the bottom because it is still on the ground. Thin,
            // because the helices give the column a readable shape rather
            // than being the column, and a fat ribbon takes the reading away
            // from the grains. Monotonic with one slow modulation and nothing
            // else: several terms keyed to distance reach the sample limit
            // and pinch the tube shut wherever their zeros line up.
            let taper = bell(u * 0.92 + 0.04);
            let tube = 0.125
                * taper
                * envelope
                * (0.78 + 0.34 * (u * 3.4 + cast.time * 2.2 + helix as f32).sin());

            // The section roll carries no distance term. A roll that advances
            // along the spine spirals everything keyed to the section angle,
            // including the relief, so the surface comes out cut with a screw
            // thread. The ribbon wants that, because its section is an
            // ellipse and the twist is the point; a round section gains
            // nothing from it but the artefact.
            water::column(
                cast.water,
                strand,
                column,
                point,
                tube,
                right,
                cast.time * 0.7 + helix as f32 * 2.1,
                distance,
                u,
                0.22 + 0.3 * (1.0 - height),
                1.0,
            );
            previous = point;
        }

        // Almost entirely opaque, because this is lifted snow rather than
        // water. The transparency that is left lets the far side of the
        // column show through the near side, which is most of what makes it
        // read as a rotating volume.
        water::set_params(
            cast.water,
            strand,
            PROFILE_TUBE,
            0.88,
            (envelope * 1.3).clamp(0.0, 1.0),
            COLS,
        );
    }
}

/// Strips the ground, then gives it back.
///
/// The ring grows outward while the spell holds and retreats while it fades,
/// so the snow comes back from the outside in, which is what settling snow
/// does: the outermost material was lifted the least far.
fn strip(vortex: &mut Vortex, cast: &mut Cast, envelope: f32) {
    let holding = vortex.time < RAMP + HOLD;
    vortex.ring = if holding {
        (vortex.ring + cast.delta_time * 0.85).min(3.1)
    } else {
        (vortex.ring - cast.delta_time * 2.2).max(0.9)
    };

    vortex.strip_owed += cast.delta_time;
    if vortex.strip_owed < 1.0 / 45.0 {
        return;
    }
    let weight = vortex.strip_owed.min(0.05);
    vortex.strip_owed = 0.0;

    const RANKS: usize = 9;
    let mut random = rand::rng();
    for rank in 0..RANKS {
        // Rotating with the column, so the ring is scoured rather than
        // stamped: a fixed set of angles leaves nine radial scars.
        let angle = (rank as f32 / RANKS as f32) * std::f32::consts::TAU + vortex.spin * 0.6;
        let radius = vortex.ring * random.random_range(0.82..1.12);

        // Holding takes snow away, with no berm, because the mass is in the
        // air rather than piled at the rim. Fading puts it back as a negative
        // depression plus a little loose berm, because what lands is broken
        // snow sitting proud of what it fell on.
        let brush = if holding {
            Brush {
                depth: 0.95 * weight * envelope,
                berm: 0.05 * weight * envelope,
                compression: 0.30 * weight * envelope,
                ..Brush::default()
            }
        } else {
            Brush {
                depth: -1.7 * weight,
                berm: 0.85 * weight,
                compression: -0.6 * weight,
                ..Brush::default()
            }
        };

        deform::brush(
            cast.deform,
            &Brush {
                x: vortex.centre[0] + angle.cos() * radius,
                z: vortex.centre[1] + angle.sin() * radius,
                radius: 0.55,
                ice: 0.0,
                yaw: angle + std::f32::consts::FRAC_PI_2,
                elongation: 1.9,
                edge: 1.0,
                ..brush
            },
        );
    }
}

/// The airborne grains, emitted on one of the helices with that helix's own
/// tangential velocity, and short lived enough that a straight-line
/// integration never visibly departs from the curve it was launched along.
///
/// Nothing in the particle system knows this is a vortex: the swirl is
/// entirely in where and how the grains are born.
fn grains(vortex: &mut Vortex, cast: &mut Cast, envelope: f32) {
    if envelope < 0.05 {
        return;
    }
    let rate = 2600.0 * cast.spray_scale * envelope;
    vortex.grain_owed += cast.delta_time * rate;
    let mut count = vortex.grain_owed as usize;
    if count == 0 {
        return;
    }
    vortex.grain_owed -= count as f32;
    count = count.min(260);

    let ground = terrain::height_at(cast.heightfield, vortex.centre[0], vortex.centre[1]);
    let mut random = rand::rng();

    for _ in 0..count {
        // Weighted toward the bottom, where the snow is being picked up.
        let height = random.random_range(0.0..1.0_f32) * random.random_range(0.0..1.0_f32);
        let helix = random.random_range(0..HELICES);
        let phase = (helix as f32 / HELICES as f32) * std::f32::consts::TAU;
        let angle = phase
            + vortex.spin
            + height * TURNS * std::f32::consts::TAU
            + random.random_range(-0.45..0.45);
        let radius = radius_at(height) * random.random_range(0.85..1.20);
        let (sine, cosine) = angle.sin_cos();

        // Tangential: perpendicular to the radius, in the direction of spin.
        let speed = 7.5 - 2.6 * height;
        spray::emit(
            cast.spray,
            [
                vortex.centre[0] + cosine * radius,
                ground + TOP * height * envelope + 0.06 + random.random_range(0.0..0.2),
                vortex.centre[1] + sine * radius,
            ],
            [
                -sine * speed + cosine * random.random_range(-0.72..0.48),
                random.random_range(1.4..4.8) + (1.0 - height) * 2.5,
                cosine * speed + sine * random.random_range(-0.72..0.48),
            ],
            random.random_range(0.028..0.090),
            random.random_range(0.30..0.56),
            0.0,
            // Low drag and a short life, so it holds the launch velocity for
            // the whole of it, which is what keeps it on the spiral.
            Some(0.9),
        );
    }
}

/// Hands every strand back, so the pool does not leak three slots per cast.
fn release(vortex: &mut Vortex, water: &mut WaterBody) {
    for strand in &mut vortex.strands {
        if let Some(index) = strand.take() {
            water::release(water, index);
        }
    }
}
