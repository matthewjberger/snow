use crate::systems::character::Character;
use crate::systems::deform;
use crate::systems::deform::{Brush, Deformation};
use crate::systems::figure::Figure;
use crate::systems::spray;
use crate::systems::spray::Spray;
use rand::Rng;

/// Boot geometry, in metres.
const BOOT_WIDTH: f32 = 0.10;
const BOOT_ELONGATION: f32 = 1.7;

/// Surf groove geometry, in metres.
const SURF_WIDTH: f32 = 0.30;
const SURF_ELONGATION: f32 = 2.6;

/// Where the character meets the snow.
#[derive(Default)]
pub struct Contact {
    previous_x: f32,
    previous_z: f32,
}

pub fn update(
    contact: &mut Contact,
    character: &Character,
    figure: &Figure,
    field: &mut Deformation,
    spray: &mut Spray,
) {
    let dx = character.position.x - contact.previous_x;
    let dz = character.position.z - contact.previous_z;
    let moved = (dx * dx + dz * dz).sqrt();
    contact.previous_x = character.position.x;
    contact.previous_z = character.position.z;

    if character.airborne {
        return;
    }
    if character.landed {
        land(character, field, spray);
    }

    if character.surf > 0.02 {
        surf(character, field, moved);
    }
    if character.surf < 0.98 {
        walk(character, field, moved);
    }

    for foot in 0..2 {
        if !figure.touchdown[foot] || !character.stepping {
            continue;
        }
        let plant = figure.plant[foot];
        let impact = (0.35 + character.speed / 5.4).min(1.3);
        deform::brush(
            field,
            &Brush {
                x: plant[0],
                z: plant[2],
                radius: BOOT_WIDTH,
                depth: 0.17 + 0.14 * impact,
                berm: 0.10 + 0.08 * impact,
                compression: 0.9,
                ice: 0.0,
                yaw: character.facing,
                elongation: BOOT_ELONGATION,
                edge: 1.0,
            },
        );
        kick(character, plant, impact, spray);
    }
}

/// Both boots at once, which is what a landing is.
///
/// Deeper and wider than a footfall, centred under the body, with the snow going
/// up: a drop punches a hole and throws a collar around it, where a stride
/// shears snow forward.
fn land(character: &Character, field: &mut Deformation, spray: &mut Spray) {
    let impact = character.landing_impact;
    let right_x = character.facing.cos();
    let right_z = -character.facing.sin();

    for foot in 0..2 {
        let side = if foot == 0 { -0.105 } else { 0.105 };
        deform::brush(
            field,
            &Brush {
                x: character.position.x + right_x * side,
                z: character.position.z + right_z * side,
                radius: BOOT_WIDTH * 1.45,
                depth: 0.24 + 0.30 * impact,
                berm: 0.16 + 0.20 * impact,
                compression: 1.0,
                ice: 0.0,
                yaw: character.facing,
                elongation: BOOT_ELONGATION * 0.8,
                edge: 0.9,
            },
        );
    }

    let mut random = rand::rng();
    let count = (70.0 * impact) as usize;
    for _ in 0..count {
        let angle = random.random_range(0.0..std::f32::consts::TAU);
        let radius = random.random_range(0.0..0.42);
        let outward = random.random_range(0.8..3.4) * impact;
        spray::emit(
            spray,
            [
                character.position.x + angle.cos() * radius,
                character.position.y + random.random_range(0.0..0.12),
                character.position.z + angle.sin() * radius,
            ],
            [
                angle.cos() * outward + character.velocity.x * 0.3,
                random.random_range(1.4..4.2) * impact,
                angle.sin() * outward + character.velocity.z * 0.3,
            ],
            random.random_range(0.010..0.030),
            random.random_range(0.5..1.2),
            if random.random_range(0.0..1.0_f32) < 0.25 {
                1.0
            } else {
                0.0
            },
            Some(3.2),
        );
    }
}

/// Snow thrown by a boot landing.
fn kick(character: &Character, plant: [f32; 3], impact: f32, spray: &mut Spray) {
    if character.speed < 0.4 {
        return;
    }
    let mut random = rand::rng();
    let forward = [character.facing.sin(), character.facing.cos()];

    let count = 6 + (impact * 14.0) as usize;
    for _ in 0..count {
        let across = random.random_range(-0.45..0.45);
        let along = random.random_range(-0.45..0.45);
        let up = random.random_range(0.9..2.8);
        let back = 0.5 + random.random_range(0.0..1.6) * impact;
        let clod = if random.random_range(0.0..1.0_f32) < 0.22 {
            1.0
        } else {
            0.0
        };

        spray::emit(
            spray,
            [
                plant[0] + across * 0.09,
                plant[1] + 0.03 + random.random_range(0.0..0.05),
                plant[2] + along * 0.09,
            ],
            [
                -forward[0] * back + across * 1.3 + character.velocity.x * 0.25,
                up * if clod > 0.5 { 1.25 } else { 1.0 },
                -forward[1] * back + along * 1.3 + character.velocity.z * 0.25,
            ],
            if clod > 0.5 {
                random.random_range(0.014..0.026)
            } else {
                random.random_range(0.020..0.050)
            },
            if clod > 0.5 {
                random.random_range(0.55..0.90)
            } else {
                random.random_range(0.55..1.15)
            },
            clod,
            None,
        );
    }
}

/// Walking scuff.
fn walk(character: &Character, field: &mut Deformation, moved: f32) {
    if character.speed < 0.25 {
        return;
    }

    let weight = 1.0 - character.surf;
    let step = moved.min(0.35);
    deform::brush(
        field,
        &Brush {
            x: character.position.x,
            z: character.position.z,
            radius: 0.22,
            depth: 0.20 * step * weight,
            berm: 0.22 * step * weight,
            compression: 0.8 * step * weight,
            ice: 0.0,
            yaw: character.facing,
            elongation: 1.5,
            edge: 0.85,
        },
    );
}

/// The surf wake: the groove the board cuts, and one berm on each side weighted by
/// the carve, so the outside of a turn throws a much heavier wall of snow than the
/// inside.
fn surf(character: &Character, field: &mut Deformation, moved: f32) {
    let speed_weight = (character.speed / 6.0).min(1.0);
    if speed_weight < 0.05 {
        return;
    }

    let step = moved.min(0.6) * character.surf * speed_weight;
    if step <= 0.0 {
        return;
    }

    let fast = ((character.speed - 6.0).max(0.0) / 12.0).min(1.0);

    let yaw = character.facing;
    let right_x = yaw.cos();
    let right_z = -yaw.sin();

    let lean = character.carve;
    deform::brush(
        field,
        &Brush {
            x: character.position.x + right_x * lean * 0.12,
            z: character.position.z + right_z * lean * 0.12,
            radius: SURF_WIDTH * (1.0 + 0.35 * fast),
            depth: 1.20 * step,
            berm: 0.30 * step,
            compression: 4.0 * step,
            ice: 0.0,
            yaw,
            elongation: SURF_ELONGATION,
            edge: 0.55,
        },
    );

    let outside = lean.abs().min(1.0);
    let left_weight = 0.5 + lean * 0.5;
    let right_weight = 0.5 - lean * 0.5;

    let offset = SURF_WIDTH * (1.5 + 0.5 * fast);
    let thrown = 0.75 * step * (0.55 + 0.9 * outside) * (1.0 + 0.5 * fast);

    deform::brush(
        field,
        &Brush {
            x: character.position.x - right_x * offset,
            z: character.position.z - right_z * offset,
            radius: SURF_WIDTH * 0.95,
            berm: thrown * left_weight * 2.0,
            yaw,
            elongation: SURF_ELONGATION * 0.8,
            ..Default::default()
        },
    );
    deform::brush(
        field,
        &Brush {
            x: character.position.x + right_x * offset,
            z: character.position.z + right_z * offset,
            radius: SURF_WIDTH * 0.95,
            berm: thrown * right_weight * 2.0,
            yaw,
            elongation: SURF_ELONGATION * 0.8,
            ..Default::default()
        },
    );
}
