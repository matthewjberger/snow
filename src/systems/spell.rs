pub mod bending;
pub mod bloom;
pub mod crystallize;
pub mod crystals;
pub mod lights;
pub mod ribbon;
pub mod sweep;
pub mod vortex;
pub mod water;

pub use lights::SpellLights;
pub use water::WaterBody;

use crate::ecs::SnowResources;
use crate::input::SnowInput;
use crate::settings::Settings;
use crate::systems::deform::Deformation;
use crate::systems::terrain::Heightfield;
use crate::systems::{Perf, Spray, perf};
use nightshade::prelude::nightshade_ecs::dynamic::Component;
use nightshade::prelude::*;

/// Everything a spell writes into while it runs.
///
/// One borrow of each, taken once for the whole dispatch, because every spell
/// touches the same four things and threading them individually through five
/// update signatures is how two of them end up disagreeing about which frame the
/// brushes belong to.
pub struct Cast<'a> {
    pub delta_time: f32,
    pub spray_scale: f32,
    pub time: f32,
    /// The rig's basis. The player points with the camera and the figure turns
    /// to follow, so aim comes off the rig rather than off the character.
    pub aim: [f32; 3],
    pub right: [f32; 3],
    pub up: [f32; 3],
    /// The casting hand, in world space.
    pub hand: [f32; 3],
    pub heightfield: &'a Heightfield,
    pub water: &'a mut WaterBody,
    pub lights: &'a mut SpellLights,
    pub deform: &'a mut Deformation,
    pub spray: &'a mut Spray,
    /// Shake a spell asked for, applied to the rig once the dispatch is done.
    pub trauma: f32,
}

/// The spell of one kind that is up, if any.
///
/// Each kind is single instance: a recast restarts it rather than stacking one
/// on top of itself, and the pool of water strands is eight, so five spells that
/// each stacked freely would starve each other of surface.
pub fn live<T: Component + Copy>(world: &World, mask: u64) -> Option<(Entity, T)> {
    world.ecs.worlds[GAME]
        .query_entities(mask)
        .next()
        .and_then(|entity| world.get::<T>(entity).map(|state| (entity, *state)))
}

/// Writes a cast back, spawning it if this is the frame it started on.
pub fn commit<T: Component + Copy>(world: &mut World, entity: Option<Entity>, state: T) {
    match entity {
        Some(entity) => world.set(entity, state),
        None => {
            world.spawn_with((state,));
        }
    }
}

/// Casts what was pressed, then runs every spell that is up.
///
/// The light pool is cleared before they run and read after, or a spell that
/// ended last frame keeps lighting the snow. The brushes are written here rather
/// than after the terrain, so the simulation pass sees this frame's marks.
///
/// A spell is an entity: casting spawns one and the spell's own system despawns
/// it when it has finished, which is why nothing here carries an active flag.
pub fn cast_system(snow: &mut SnowResources, world: &mut World) {
    let settings = world.res::<Settings>();
    let delta_time = if settings.freeze_time {
        0.0
    } else {
        world.res::<Time>().delta_time.min(0.1)
    };
    snow.lights.scale = settings.spell_light;
    let spray_scale = settings.spell_spray;
    let enabled = settings.show_spells;
    let pressed = world.res::<SnowInput>().spell_pressed;
    let held_ribbon = world.res::<SnowInput>().spell_held_2;

    let mut clock = Instant::now();
    lights::begin(&mut snow.lights);

    let eye = [
        snow.rig.position.x,
        snow.rig.position.y,
        snow.rig.position.z,
    ];
    let feet = [snow.character.position.x, snow.character.position.z];
    let hand = crate::systems::figure::hand_position(&snow.figure, 1);

    let SnowResources {
        rig,
        heightfield,
        water,
        lights,
        deform,
        spray,
        time,
        ..
    } = snow;

    let mut cast = Cast {
        delta_time,
        spray_scale,
        time: *time,
        aim: [rig.forward.x, rig.forward.y, rig.forward.z],
        right: [rig.right.x, rig.right.y, rig.right.z],
        up: [rig.up.x, rig.up.y, rig.up.z],
        hand,
        heightfield,
        water,
        lights,
        deform,
        spray,
        trauma: 0.0,
    };

    if !enabled {
        sweep::cancel_all(world, cast.water);
        ribbon::cancel_all(world, cast.water);
        bloom::cancel_all(world, cast.water);
        vortex::cancel_all(world, cast.water);
        crystallize::cancel_all(world);
        crystals::clear(world);
        water::end_frame(cast.water, delta_time);
        mark(world, &mut clock);
        return;
    }

    let aim = cast.aim;
    match pressed {
        // Flat, because the crescent runs along the ground and a camera pointed
        // at the sky must not launch it into the air.
        1 => {
            sweep::cast(world, &mut cast, feet, [aim[0], aim[2]]);
            cast.trauma = cast.trauma.max(0.12);
        }
        // Placed where the player is looking, so what the spell hits is what is
        // under the centre of the screen. Capped well short of what the terrain
        // could answer for: across a dune field the first surface a ray meets is
        // often on the next ridge, and a bloom over there is an effect the
        // player has to squint at.
        3 => {
            let target = bending::aim_point(cast.heightfield, eye, aim, 22.0, 13.0);
            bloom::cast(world, &mut cast, target);
        }
        4 => {
            let target = bending::aim_point(cast.heightfield, eye, aim, 22.0, 13.0);
            crystallize::cast(world, &mut cast, target);
        }
        5 => {
            vortex::cast(world, &mut cast);
            cast.trauma = cast.trauma.max(0.10);
        }
        _ => {}
    }

    // The ribbon is a hold, so it is polled rather than edge triggered.
    ribbon::hold(world, &mut cast, held_ribbon);

    sweep::update(world, &mut cast);
    ribbon::update(world, &mut cast);
    bloom::update(world, &mut cast);
    vortex::update(world, &mut cast, feet);
    crystallize::update(world, &mut cast);

    let trauma = cast.trauma;
    water::end_frame(cast.water, delta_time);
    crystals::age(world, delta_time);

    if trauma > 0.0 {
        crate::camera::add_trauma(rig, trauma);
    }

    mark(world, &mut clock);
}

/// Files this frame's spell cost under the same clock the other systems use, so
/// the overlay lists it in the order the frame actually ran.
fn mark(world: &mut World, clock: &mut Instant) {
    let milliseconds = clock.elapsed().as_secs_f32() * 1000.0;
    perf::mark(world.res_mut::<Perf>(), "spells", milliseconds);
}
