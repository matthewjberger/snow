use crate::ecs::{SWEEP, Sweep};
use crate::systems::deform;
use crate::systems::deform::{Brush, Deformation};
use crate::systems::spell::bending::{bell, smooth01};
use crate::systems::spell::lights;
use crate::systems::spell::water::{self, PROFILE_SHEET, WaterBody};
use crate::systems::spell::{self, Cast};
use crate::systems::spray;
use crate::systems::terrain;
use nightshade::prelude::*;
use rand::Rng;

/// Spine samples across the crescent.
const COLS: usize = 48;

/// Curvature radius of the crescent, in metres, and fixed.
///
/// Not the distance travelled. Using that makes the wave an arc of a circle
/// centred on the caster, so ten metres out the crescent is twenty metres wide: a
/// ridge in the terrain rather than something thrown. A wave front has a
/// curvature of its own that has nothing to do with how far it has run, so the
/// arc keeps its shape and translates, and only its span opens up.
const CURVE: f32 = 5.5;

/// Half-angle of the arc at cast and at full spread, in radians.
const ARC_NEAR: f32 = 0.52;
const ARC_FAR: f32 = 0.96;

/// Seconds from cast to fully collapsed.
const LIFE: f32 = 2.4;

/// Peak crest height at the centre of the arc, in metres.
///
/// Taller than the character, on the same reasoning as the surf wake: at this
/// framing a crest the height of the relief the terrain already has reads as a
/// dune rather than as thrown mass.
const PEAK: f32 = 2.15;

/// Casting spawns a crescent, or restarts the one already running.
pub fn cast(world: &mut World, cast: &mut Cast, feet: [f32; 2], aim: [f32; 2]) {
    let standing = spell::live::<Sweep>(world, SWEEP);
    let mut sweep = standing.map(|(_, sweep)| sweep).unwrap_or_default();

    if sweep.strand.is_none() {
        sweep.strand = water::acquire(cast.water);
    }
    if sweep.strand.is_none() {
        return;
    }
    begin(&mut sweep, feet, aim);
    spell::commit(world, standing.map(|(entity, _)| entity), sweep);
}

/// Runs the crescent, despawning it once it has collapsed.
pub fn update(world: &mut World, cast: &mut Cast) {
    let Some((entity, mut sweep)) = spell::live::<Sweep>(world, SWEEP) else {
        return;
    };
    if tick(&mut sweep, cast) {
        world.set(entity, sweep);
    } else {
        release(&mut sweep, cast.water);
        world.despawn_recursive(entity);
    }
}

/// Drops the crescent, for the settings toggle.
pub fn cancel_all(world: &mut World, water: &mut WaterBody) {
    let Some((entity, mut sweep)) = spell::live::<Sweep>(world, SWEEP) else {
        return;
    };
    release(&mut sweep, water);
    world.despawn_recursive(entity);
}

fn begin(sweep: &mut Sweep, feet: [f32; 2], aim: [f32; 2]) {
    let length = (aim[0] * aim[0] + aim[1] * aim[1]).sqrt().max(1e-6);
    sweep.direction = [aim[0] / length, aim[1] / length];
    // Born a little ahead of the feet, so the player is never inside it.
    sweep.origin = [
        feet[0] + sweep.direction[0] * 1.1,
        feet[1] + sweep.direction[1] * 1.1,
    ];
    sweep.time = 0.0;
    sweep.reach = 1.4;
    sweep.brush_owed = 0.0;
    sweep.spray_owed = 0.0;
}

/// Advances one crescent, reporting whether it is still standing.
fn tick(sweep: &mut Sweep, cast: &mut Cast) -> bool {
    let delta_time = cast.delta_time;
    let heightfield = cast.heightfield;
    let Some(strand) = sweep.strand else {
        return false;
    };

    sweep.time += delta_time;
    let life = sweep.time / LIFE;
    if life >= 1.0 {
        return false;
    }

    // Speed decays, because the wave is launched rather than driven, which is
    // what makes it read as something thrown.
    let speed = 11.5 * (-sweep.time * 1.15).exp() + 1.2;
    let travelled = speed * delta_time;
    sweep.reach += travelled;

    // Rise fast, hold, fall. The fall is quadratic to exactly zero so the last
    // frame of the wave is flat rather than a step.
    let rise = smooth01(sweep.time / 0.26);
    let fall = 1.0 - ((life - 0.55) / 0.45).clamp(0.0, 1.0);
    let envelope = rise * fall * fall;

    // A wave spreads as it runs: the arc opens and the crest thins, so the
    // same mass covers more ground.
    let spread = ((sweep.reach - 1.4) / 14.0).clamp(0.0, 1.0);
    let arc = ARC_NEAR + (ARC_FAR - ARC_NEAR) * spread;
    let height = PEAK * envelope / (1.0 + spread * 0.45);

    let centre = circle_centre(sweep, sweep.reach);
    let mut middle = [0.0_f32; 3];

    for column in 0..COLS {
        let u = column as f32 / (COLS - 1) as f32;
        let radial = radial(sweep, u, arc);
        let x = centre[0] + radial[0] * CURVE;
        let z = centre[1] + radial[1] * CURVE;
        // Sunk, so the base of the wall meets the trench floor it is cutting
        // rather than floating on the undisturbed surface.
        let y = terrain::height_at(heightfield, x, z) - 0.13;

        // Horns taper to nothing. The bell is on the parameter rather than on
        // the angle, so the two ends close symmetrically however wide the arc
        // has opened, and the sheet degenerates onto its own spine there.
        let amplitude = height * bell(u);
        // The crest curls harder in the middle, where the mass is, and is
        // pushed most of the way to the section integral's plunging limit: a
        // bank lying on a dune field is indistinguishable from the dune field.
        let curl = 0.48 + 0.47 * bell(u) * (0.45 + 0.55 * rise);
        let foam = 0.30 + 0.45 * bell(u);

        water::column(
            cast.water,
            strand,
            column,
            [x, y, z],
            amplitude,
            [radial[0], 0.0, radial[1]],
            curl,
            sweep.reach + u * 2.0,
            life,
            foam,
            1.0,
        );

        if column == COLS / 2 {
            middle = [x, y, z];
        }
    }

    // Mostly water, with enough entrained snow to be opaque at the crest.
    // Much below this it reads as a glass sculpture, and much above it the
    // water disappears and it is a snow berm that happens to be moving.
    water::set_params(
        cast.water,
        strand,
        PROFILE_SHEET,
        0.48,
        (envelope * 1.4).clamp(0.0, 1.0),
        COLS,
    );

    // Low, so it grazes the channel it is cutting rather than lighting it
    // from above.
    lights::add(
        cast.lights,
        [middle[0], middle[1] + height * 0.55, middle[2]],
        9.5,
        [0.42, 0.74, 1.0],
        13.0 * envelope,
    );

    plough(sweep, travelled, envelope, cast.deform);
    throw_spray(sweep, travelled, envelope, height, cast);
    true
}

/// The circle centre, one curvature radius behind the leading point.
fn circle_centre(sweep: &Sweep, reach: f32) -> [f32; 2] {
    [
        sweep.origin[0] + sweep.direction[0] * (reach - CURVE),
        sweep.origin[1] + sweep.direction[1] * (reach - CURVE),
    ]
}

/// The outward radial at a parameter along the arc: the direction the section
/// faces, and the direction that end of the crescent is running.
fn radial(sweep: &Sweep, u: f32, arc: f32) -> [f32; 2] {
    let angle = (u - 0.5) * 2.0 * arc;
    let (sine, cosine) = angle.sin_cos();
    let across = [sweep.direction[1], -sweep.direction[0]];
    [
        sweep.direction[0] * cosine + across[0] * sine,
        sweep.direction[1] * cosine + across[1] * sine,
    ]
}

/// The channel and its berms, written per metre travelled rather than per
/// second, so the trench has the same depth at any speed or frame rate.
fn plough(sweep: &mut Sweep, travelled: f32, envelope: f32, deform: &mut Deformation) {
    if envelope < 0.05 {
        return;
    }
    sweep.brush_owed += travelled;
    // One rank of brushes every quarter metre of advance. Denser just re-cuts
    // the same trench; sparser leaves it scalloped.
    if sweep.brush_owed < 0.25 {
        return;
    }
    let weight = sweep.brush_owed.min(0.7);
    sweep.brush_owed = 0.0;

    let spread = ((sweep.reach - 1.4) / 14.0).clamp(0.0, 1.0);
    let arc = ARC_NEAR + (ARC_FAR - ARC_NEAR) * spread;
    const RANKS: usize = 13;

    // Slightly behind the crest: the channel is what the wave has already
    // passed over, not what it is about to.
    let centre = circle_centre(sweep, sweep.reach - 0.5);

    for rank in 0..RANKS {
        let u = rank as f32 / (RANKS - 1) as f32;
        let across = bell(u);
        if across < 0.06 {
            continue;
        }
        let radial = radial(sweep, u, arc);
        let scale = weight * envelope * across;

        deform::brush(
            deform,
            &Brush {
                x: centre[0] + radial[0] * CURVE,
                z: centre[1] + radial[1] * CURVE,
                radius: 0.34,
                depth: 0.95 * scale,
                berm: 0.62 * scale,
                compression: 0.55 * scale,
                ice: 0.16 * scale,
                // The long axis runs along the arc, so the trench is continuous
                // rather than a row of round pits.
                yaw: radial[1].atan2(-radial[0]),
                elongation: 2.2,
                edge: 0.9,
            },
        );
    }
}

/// Spray off the crest, thrown outward and back over the top.
fn throw_spray(sweep: &mut Sweep, travelled: f32, envelope: f32, height: f32, cast: &mut Cast) {
    if envelope < 0.08 {
        return;
    }
    let per_metre = 120.0 * cast.spray_scale;
    let Cast {
        heightfield, spray, ..
    } = cast;
    let heightfield = *heightfield;
    sweep.spray_owed += travelled;
    let mut count = (sweep.spray_owed * per_metre) as usize;
    if count == 0 {
        return;
    }
    sweep.spray_owed -= count as f32 / per_metre;
    count = count.min(150);

    let spread = ((sweep.reach - 1.4) / 14.0).clamp(0.0, 1.0);
    let arc = ARC_NEAR + (ARC_FAR - ARC_NEAR) * spread;
    let centre = circle_centre(sweep, sweep.reach);
    let mut random = rand::rng();

    for _ in 0..count {
        let u = random.random_range(0.0..1.0_f32);
        let across = bell(u);
        if across < 0.12 {
            continue;
        }
        let radial = radial(sweep, u, arc);
        let amplitude = height * across;
        let distance = CURVE + random.random_range(-0.2..0.4);
        let x = centre[0] + radial[0] * distance;
        let z = centre[1] + radial[1] * distance;
        let y = terrain::height_at(heightfield, x, z) + amplitude * random.random_range(0.55..1.15);

        let out = random.random_range(1.4..4.6);
        let clod = random.random_range(0.0..1.0_f32) < 0.2;
        spray::emit(
            spray,
            [x, y, z],
            [
                radial[0] * out + random.random_range(-0.7..0.7),
                random.random_range(1.5..4.7) + amplitude * 1.6,
                radial[1] * out + random.random_range(-0.7..0.7),
            ],
            if clod {
                random.random_range(0.022..0.046)
            } else {
                random.random_range(0.050..0.125)
            },
            if clod {
                random.random_range(0.6..1.1)
            } else {
                random.random_range(0.55..1.25)
            },
            if clod { 1.0 } else { 0.0 },
            Some(if clod {
                0.8
            } else {
                random.random_range(1.6..3.0)
            }),
        );
    }
}

/// Hands the strand back, so the pool does not leak a slot per cast.
fn release(sweep: &mut Sweep, water: &mut WaterBody) {
    if let Some(strand) = sweep.strand.take() {
        water::release(water, strand);
    }
}
