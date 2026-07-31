use crate::ecs::{CRYSTALLIZE, Crystallize};
use crate::systems::deform;
use crate::systems::deform::Brush;
use crate::systems::spell::bending::smooth01;
use crate::systems::spell::crystals;
use crate::systems::spell::lights;
use crate::systems::spell::{self, Cast};
use crate::systems::spray;
use crate::systems::terrain;
use nightshade::prelude::*;
use rand::Rng;

/// Seconds the whole cast takes to finish planting.
const PLANT_TIME: f32 = 0.85;

/// Crystals in one formation.
const COUNT: usize = 34;

/// Seconds the formation stands at full size before sublimating.
const STAND: f32 = 34.0;

/// Casting starts a formation, or restarts the one already growing.
pub fn cast(world: &mut World, cast: &mut Cast, target: [f32; 3]) {
    let standing = spell::live::<Crystallize>(world, CRYSTALLIZE);
    let mut crystallize = standing.map(|(_, state)| state).unwrap_or_default();
    begin(&mut crystallize, cast, target);
    spell::commit(world, standing.map(|(entity, _)| entity), crystallize);
}

/// Plants the formation, despawning the cast once the last prism is in.
///
/// The prisms outlive the cast: they are their own entities on their own clock,
/// which is why this despawns rather than lingering to hold them.
pub fn update(world: &mut World, cast: &mut Cast) {
    let Some((entity, mut crystallize)) = spell::live::<Crystallize>(world, CRYSTALLIZE) else {
        return;
    };
    if tick(world, &mut crystallize, cast) {
        world.set(entity, crystallize);
    } else {
        world.despawn_recursive(entity);
    }
}

/// Drops the cast, for the settings toggle. The prisms are cleared separately.
pub fn cancel_all(world: &mut World) {
    let Some((entity, _)) = spell::live::<Crystallize>(world, CRYSTALLIZE) else {
        return;
    };
    world.despawn_recursive(entity);
}

fn begin(crystallize: &mut Crystallize, cast: &mut Cast, target: [f32; 3]) {
    crystallize.centre = target;
    crystallize.time = 0.0;
    crystallize.planted = 0;
    let mut random = rand::rng();
    crystallize.seed = random.random_range(0.0..1000.0);

    // The glaze goes down immediately, under where the formation will be, so
    // the ground has already changed material by the time the first prism is
    // tall enough to see. Doing it as the crystals land leaves a beat where
    // ice is standing on ordinary snow.
    deform::brush(
        cast.deform,
        &Brush {
            x: target[0],
            z: target[2],
            radius: 1.55,
            depth: 0.10,
            berm: 0.16,
            compression: 0.85,
            ice: 1.0,
            yaw: random.random_range(0.0..std::f32::consts::PI),
            elongation: 1.2,
            edge: 0.85,
        },
    );
    for _ in 0..3 {
        let angle = random.random_range(0.0..std::f32::consts::TAU);
        let distance = random.random_range(1.1..2.4);
        deform::brush(
            cast.deform,
            &Brush {
                x: target[0] + angle.cos() * distance,
                z: target[2] + angle.sin() * distance,
                radius: random.random_range(0.55..1.05),
                depth: 0.04,
                berm: 0.10,
                compression: 0.5,
                ice: 0.75,
                yaw: angle,
                elongation: 1.5,
                edge: 1.0,
            },
        );
    }
}

/// Advances the formation, reporting whether it is still planting.
fn tick(world: &mut World, crystallize: &mut Crystallize, cast: &mut Cast) -> bool {
    crystallize.time += cast.delta_time;

    // Spread over most of a second rather than all at once, so the formation
    // grows outward from the centre instead of appearing on one frame.
    let want = COUNT.min(((crystallize.time / PLANT_TIME) * COUNT as f32).ceil() as usize);
    while crystallize.planted < want {
        plant_one(world, crystallize, crystallize.planted, cast);
        crystallize.planted += 1;
    }

    // Bright and tight while it is forming, then a low ember that lasts as
    // long as the formation does. Ice does not emit, but snow around a
    // cluster of refracting prisms under a low sun genuinely picks up caustic
    // light, and a little of it is what stops the formation looking pasted on.
    let forming = 1.0 - smooth01((crystallize.time - PLANT_TIME) / 0.9);
    let ember = 0.10 + 0.06 * (crystallize.time * 1.7).sin();
    lights::add(
        cast.lights,
        [
            crystallize.centre[0],
            crystallize.centre[1] + 0.55,
            crystallize.centre[2],
        ],
        7.5,
        [0.52, 0.80, 1.0],
        (0.35 + 12.0 * forming) * (1.0 + ember),
    );

    if crystallize.time < PLANT_TIME + 0.4 {
        frost(crystallize, cast);
    }

    // The spell is done once the last prism is in; the crystals age on their
    // own clock from there.
    crystallize.time <= PLANT_TIME + 1.6
}

/// One prism, on the spiral.
///
/// The golden angle is doing real work: it is the one rotation that never
/// repeats a radial line, so no two crystals line up with each other however
/// many there are. Any rational fraction of a turn gives visible spokes.
fn plant_one(world: &mut World, crystallize: &Crystallize, index: usize, cast: &mut Cast) {
    let mut random = rand::rng();
    let along = index as f32 / (COUNT - 1) as f32;
    let angle = index as f32 * 2.399_963_2 + crystallize.seed;
    let radius = 0.18 + along.sqrt() * 2.05;

    let x = crystallize.centre[0] + angle.cos() * radius + random.random_range(-0.08..0.08);
    let z = crystallize.centre[2] + angle.sin() * radius + random.random_range(-0.08..0.08);
    let y = terrain::height_at(cast.heightfield, x, z) - 0.06;

    // Tall in the middle, low at the edges, with enough scatter that the
    // envelope is not a readable cone. The centre crystals are chest height
    // on the character deliberately: a knee-height cluster is something the
    // player walks past, and scale is the cheapest drama there is.
    let scale = (1.0 - along * 0.58) * random.random_range(0.6..1.4);
    let height = 1.75 * scale;
    let girth = 0.15 * scale * random.random_range(0.7..1.4);

    // Leaning outward, more so further out, the way a real cluster grows
    // toward the space it has.
    let tilt = 0.10 + along * 0.42 * random.random_range(0.6..1.4);
    crystals::plant(
        world,
        [x, y, z],
        [
            angle.cos() * tilt + random.random_range(-0.06..0.06),
            1.0,
            angle.sin() * tilt + random.random_range(-0.06..0.06),
        ],
        height,
        girth,
        random.random_range(0.45..1.0),
        STAND + random.random_range(0.0..8.0),
    );

    // A little snow pushed aside where each one broke the surface.
    if index.is_multiple_of(2) {
        deform::brush(
            cast.deform,
            &Brush {
                x,
                z,
                radius: girth * 3.2,
                depth: 0.05,
                berm: 0.09,
                compression: 0.4,
                ice: 0.9,
                yaw: angle,
                elongation: 1.2,
                edge: 1.0,
            },
        );
    }
}

/// Frost thrown off as the ice breaks the surface.
fn frost(crystallize: &Crystallize, cast: &mut Cast) {
    let count = (60.0 * cast.spray_scale * cast.delta_time) as usize;
    let mut random = rand::rng();
    for _ in 0..count {
        let angle = random.random_range(0.0..std::f32::consts::TAU);
        let radius = random.random_range(0.0..1.8);
        spray::emit(
            cast.spray,
            [
                crystallize.centre[0] + angle.cos() * radius,
                crystallize.centre[1] + 0.05 + random.random_range(0.0..0.5),
                crystallize.centre[2] + angle.sin() * radius,
            ],
            [
                angle.cos() * random.random_range(0.6..2.0),
                random.random_range(0.9..3.3),
                angle.sin() * random.random_range(0.6..2.0),
            ],
            random.random_range(0.012..0.032),
            random.random_range(0.7..1.6),
            if random.random_range(0.0..1.0_f32) < 0.4 {
                1.0
            } else {
                0.0
            },
            Some(2.4),
        );
    }
}
