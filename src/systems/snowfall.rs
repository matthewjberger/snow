//! Weather: the snow that is still falling.
//!
//! A fixed population in a box that follows the player. Flakes wrap when they
//! leave the box, so the field is always around the camera and costs the same
//! wherever the player walks. They occupy a reserved tail of the effects data
//! texture, which keeps the whole field to one upload and one draw.

use crate::settings;
use crate::settings::Settings;
use crate::systems::spray::{SNOWFALL_COUNT, SPRAY_CAPACITY, Spray};
use nalgebra_glm::Vec3;
use rand::Rng;

/// Half width of the box the flakes live in, in metres.
const HALF_EXTENT: f32 = 34.0;

/// Height of the box, in metres. Tall enough to keep the ceiling out of frame at
/// a normal camera pitch, so flakes come into view already falling.
const CEILING: f32 = 26.0;

/// How far below the camera the box goes before a flake wraps back to the top.
const FLOOR: f32 = 6.0;

/// One drifting flake.
pub struct Snowfall {
    position: Vec<Vec3>,
    /// Fall speed, in metres a second. Varied per flake so the field breaks up
    /// as it descends.
    fall: Vec<f32>,
    size: Vec<f32>,
    seed: Vec<f32>,
    /// Phase of the flake's own sway, so no two swing together.
    phase: Vec<f32>,
    /// Metres a second of sway, across the wind.
    sway: Vec<f32>,
    time: f32,
    seeded: bool,
}

impl Default for Snowfall {
    fn default() -> Self {
        Self {
            position: vec![Vec3::zeros(); SNOWFALL_COUNT],
            fall: vec![0.0; SNOWFALL_COUNT],
            size: vec![0.0; SNOWFALL_COUNT],
            seed: vec![0.0; SNOWFALL_COUNT],
            phase: vec![0.0; SNOWFALL_COUNT],
            sway: vec![0.0; SNOWFALL_COUNT],
            time: 0.0,
            seeded: false,
        }
    }
}

/// Scatters the flakes through the box for the first time.
fn seed(snowfall: &mut Snowfall, focus: Vec3) {
    let mut random = rand::rng();
    for index in 0..SNOWFALL_COUNT {
        snowfall.position[index] = Vec3::new(
            focus.x + random.random_range(-HALF_EXTENT..HALF_EXTENT),
            focus.y - FLOOR + random.random_range(0.0..CEILING + FLOOR),
            focus.z + random.random_range(-HALF_EXTENT..HALF_EXTENT),
        );
        // Big flakes fall faster and read as nearer, which is most of what gives
        // the field its depth.
        let scale = random.random_range(0.0..1.0_f32);
        snowfall.size[index] = 0.012 + scale * 0.030;
        snowfall.fall[index] = 0.55 + scale * 1.05;
        snowfall.sway[index] = (1.0 - scale) * random.random_range(0.15..0.55);
        snowfall.phase[index] = random.random_range(0.0..std::f32::consts::TAU);
        snowfall.seed[index] = random.random_range(0.0..1.0);
    }
    snowfall.seeded = true;
}

/// Drifts every flake and writes the reserved tail of the particle texture.
pub fn update(
    snowfall: &mut Snowfall,
    spray: &mut Spray,
    delta_time: f32,
    settings: &Settings,
    focus: Vec3,
) {
    if !snowfall.seeded {
        seed(snowfall, focus);
    }
    let step = delta_time.min(1.0 / 30.0);
    snowfall.time += step;

    let angle = settings::wind_angle(settings);
    let strength = settings.wind_strength * settings.snowfall_wind;
    let wind = Vec3::new(angle.sin() * strength, 0.0, angle.cos() * strength);

    // The slider hides the tail of the population, holding the cost flat and
    // leaving every remaining flake where it was.
    let shown = ((SNOWFALL_COUNT as f32) * settings.snowfall.clamp(0.0, 1.0)) as usize;

    for index in 0..SNOWFALL_COUNT {
        let swing = (snowfall.time * 0.9 + snowfall.phase[index]).sin() * snowfall.sway[index];
        let across = Vec3::new(angle.cos(), 0.0, -angle.sin()) * swing;

        snowfall.position[index] += (wind + across) * step;
        snowfall.position[index].y -= snowfall.fall[index] * step;

        // Wrapping keeps the box centred on the player, and only touches flakes
        // that have already left it.
        let offset = snowfall.position[index] - focus;
        if offset.x > HALF_EXTENT {
            snowfall.position[index].x -= HALF_EXTENT * 2.0;
        } else if offset.x < -HALF_EXTENT {
            snowfall.position[index].x += HALF_EXTENT * 2.0;
        }
        if offset.z > HALF_EXTENT {
            snowfall.position[index].z -= HALF_EXTENT * 2.0;
        } else if offset.z < -HALF_EXTENT {
            snowfall.position[index].z += HALF_EXTENT * 2.0;
        }
        if offset.y < -FLOOR {
            snowfall.position[index].y += CEILING + FLOOR;
        } else if offset.y > CEILING {
            snowfall.position[index].y -= CEILING + FLOOR;
        }

        let slot = SPRAY_CAPACITY + index;
        let first = slot * 4;
        let second = (SPRAY_CAPACITY + SNOWFALL_COUNT + slot) * 4;
        let texels = crate::systems::spray::texels_mut(spray);

        if index >= shown {
            texels[first + 3] = 0.0;
            texels[second + 3] = 0.0;
            continue;
        }

        // Fade the last couple of metres at the ceiling and the floor, so the
        // wrap happens behind a dissolve.
        let height = snowfall.position[index].y - focus.y;
        let entering = ((CEILING - height) / 3.0).clamp(0.0, 1.0);
        let leaving = ((height + FLOOR) / 2.0).clamp(0.0, 1.0);

        texels[first] = snowfall.position[index].x;
        texels[first + 1] = snowfall.position[index].y;
        texels[first + 2] = snowfall.position[index].z;
        texels[first + 3] = snowfall.size[index];
        texels[second] = 0.0;
        texels[second + 1] = snowfall.seed[index];
        texels[second + 2] = 0.0;
        texels[second + 3] = 0.85 * entering * leaving;
    }
}
