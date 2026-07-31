//! The frame's simulation, in the order the pieces depend on each other.
//!
//! One pass, because the order is not incidental: the character moves, the
//! figure poses to where it moved, the cloth hangs off the figure, the contacts
//! come off the feet the figure placed, and the deformation window centres on
//! the character rather than on the camera. Splitting these into separately
//! registered systems would let a reordering change how the snow looks.

use crate::camera::{self, RigTarget};
use crate::ecs::SnowResources;
use crate::input::SnowInput;
use crate::settings::Settings;
use crate::systems::perf;
use crate::systems::{
    Perf, character, cloth, contact, deform, figure, shadows, spray, terrain, wake,
};
use nightshade::prelude::*;

pub fn simulate_system(snow: &mut SnowResources, world: &mut World) {
    let frame_seconds = world.res::<Time>().delta_time.min(0.1);
    let settings = *world.res::<Settings>();
    let delta_time = if settings.freeze_time {
        0.0
    } else {
        frame_seconds
    };
    snow.time += delta_time;

    let input = *world.res::<SnowInput>();

    character::update(
        &mut snow.character,
        delta_time,
        &input,
        &mut snow.rig,
        &snow.heightfield,
    );
    terrain::clamp_to_play_area(&mut snow.character.position);

    let target = RigTarget {
        position: snow.character.position,
        velocity: snow.character.velocity,
        lean: snow.character.lean,
        speed01: snow.character.speed01,
    };
    let heightfield = &snow.heightfield;
    camera::update(&mut snow.rig, delta_time, &input, &target, |x, z| {
        terrain::height_at(heightfield, x, z)
    });

    snow.focus = snow.character.position;

    let SnowResources {
        character,
        figure,
        cloth,
        contact,
        deform,
        heightfield,
        spray,
        wake,
        focus,
        ..
    } = &mut *snow;

    let mut timings: [(&'static str, f32); 6] = [
        ("figure", 0.0),
        ("cloth", 0.0),
        ("contact", 0.0),
        ("spray", 0.0),
        ("deform", 0.0),
        ("wake", 0.0),
    ];

    let mut clock = Instant::now();
    figure::update(figure, delta_time, character, heightfield);
    timings[0].1 = elapsed(&mut clock);

    cloth::update(cloth, delta_time, &settings, figure, character, heightfield);
    timings[1].1 = elapsed(&mut clock);

    contact::update(contact, character, figure, deform, spray);
    timings[2].1 = elapsed(&mut clock);

    spray::update(spray, delta_time, &settings, |x, z| {
        terrain::height_at(heightfield, x, z)
    });
    timings[3].1 = elapsed(&mut clock);

    deform::update(deform, delta_time, focus);
    timings[4].1 = elapsed(&mut clock);

    wake::update(wake, delta_time, &settings, character, heightfield, spray);
    timings[5].1 = elapsed(&mut clock);

    let perf = world.res_mut::<Perf>();
    for (name, milliseconds) in timings {
        perf::mark(perf, name, milliseconds);
    }
}

/// Fits the shadow cascades to what the camera now sees, and moves the engine
/// camera onto the rig.
///
/// Last, because both read a rig the spells have already shaken.
pub fn present_system(snow: &mut SnowResources, world: &mut World) {
    let aspect = {
        let window = world.res::<Window>();
        match window.cached_viewport_size {
            Some((width, height)) if height > 0 => width as f32 / height as f32,
            _ => 16.0 / 9.0,
        }
    };
    let view_projection =
        camera::projection_matrix(&snow.rig, aspect) * camera::view_matrix(&snow.rig);
    let sun_direction = snow.sky.sun_direction;
    shadows::update(&mut snow.shadows, &view_projection, &sun_direction);

    if let Some(entity) = snow.camera_entity
        && let Some(transform) = world.get_mut::<LocalTransform>(entity)
    {
        transform.translation = snow.rig.position;
    }

    let frame_milliseconds = world.res::<Time>().raw_delta_time * 1000.0;
    perf::sample(world.res_mut::<Perf>(), frame_milliseconds);
}

/// Milliseconds since the last call, restarting the clock.
///
/// The engine's re-export rather than the standard library's, because the
/// standard clock has no implementation on the web and panics the moment it is
/// read.
fn elapsed(clock: &mut Instant) -> f32 {
    let taken = clock.elapsed().as_secs_f32() * 1000.0;
    *clock = Instant::now();
    taken
}
