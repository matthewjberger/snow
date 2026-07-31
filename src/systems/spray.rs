use crate::settings;
use crate::settings::Settings;

/// Pool size.
pub const SPRAY_CAPACITY: usize = 5120;

/// Terminal fall speed of a snow grain, in metres a second.
const TERMINAL: f32 = 1.9;

/// A pooled, processor-simulated, billboarded particle field.
pub struct Spray {
    position: Vec<[f32; 3]>,
    velocity: Vec<[f32; 3]>,
    age: Vec<f32>,
    life: Vec<f32>,
    size: Vec<f32>,
    seed: Vec<f32>,
    /// Zero for a powder puff, one for a heavy clod.
    kind: Vec<f32>,
    /// Linear drag coefficient, per second.
    drag: Vec<f32>,

    /// Index of the next slot to try.
    next: usize,
    pub live: usize,
    time: f32,

    /// Two rows per particle: position with the radius in w, then the aged fraction,
    /// the seed, the kind and the opacity.
    texels: Vec<f32>,
}

impl Default for Spray {
    fn default() -> Self {
        Self {
            position: vec![[0.0; 3]; SPRAY_CAPACITY],
            velocity: vec![[0.0; 3]; SPRAY_CAPACITY],
            age: vec![1.0; SPRAY_CAPACITY],
            life: vec![0.0; SPRAY_CAPACITY],
            size: vec![0.0; SPRAY_CAPACITY],
            seed: vec![0.0; SPRAY_CAPACITY],
            kind: vec![0.0; SPRAY_CAPACITY],
            drag: vec![0.0; SPRAY_CAPACITY],
            next: 0,
            live: 0,
            time: 0.0,
            texels: vec![0.0; SPRAY_CAPACITY * 2 * 4],
        }
    }
}

/// Emits one grain, entirely in world space.
pub fn emit(
    spray: &mut Spray,
    position: [f32; 3],
    velocity: [f32; 3],
    size: f32,
    life: f32,
    kind: f32,
    drag: Option<f32>,
) {
    let mut slot = spray.next;
    for attempt in 0..SPRAY_CAPACITY {
        if spray.age[slot] >= spray.life[slot] {
            break;
        }
        slot = (slot + 1) % SPRAY_CAPACITY;
        if attempt == SPRAY_CAPACITY - 1 {
            return;
        }
    }
    spray.next = (slot + 1) % SPRAY_CAPACITY;

    spray.position[slot] = position;
    spray.velocity[slot] = velocity;
    spray.age[slot] = 0.0;
    spray.life[slot] = life;
    spray.size[slot] = size;
    spray.kind[slot] = kind;
    spray.drag[slot] = drag.unwrap_or(if kind > 0.5 { 1.1 } else { 5.2 });
    spray.seed[slot] =
        (slot as f32 * 0.618_033 + position[0] * 0.137 + position[2] * 0.311).rem_euclid(1.0);
}

pub fn update(
    spray: &mut Spray,
    delta_time: f32,
    settings: &Settings,
    height_at: impl Fn(f32, f32) -> f32,
) {
    spray.time += delta_time;
    let step = delta_time.min(1.0 / 30.0);

    let angle = settings::wind_angle(settings);
    let wind = [
        angle.sin() * 2.4 * settings.wind_strength,
        angle.cos() * 2.4 * settings.wind_strength,
    ];

    let mut live = 0;
    for slot in 0..SPRAY_CAPACITY {
        let first = slot * 4;
        let second = (SPRAY_CAPACITY + slot) * 4;

        if spray.age[slot] >= spray.life[slot] {
            spray.texels[first + 3] = 0.0;
            spray.texels[second + 3] = 0.0;
            continue;
        }

        spray.age[slot] += step;
        let aged = spray.age[slot] / spray.life[slot];

        let drag = spray.drag[slot];
        let blend = (drag * step).min(1.0);
        let vertical = spray.velocity[slot][1];
        spray.velocity[slot][0] += (wind[0] - spray.velocity[slot][0]) * blend;
        spray.velocity[slot][2] += (wind[1] - spray.velocity[slot][2]) * blend;
        spray.velocity[slot][1] = vertical + (-9.81 - drag * (vertical + TERMINAL)) * step;

        for axis in 0..3 {
            spray.position[slot][axis] += spray.velocity[slot][axis] * step;
        }

        let ground = height_at(spray.position[slot][0], spray.position[slot][2]);
        if spray.position[slot][1] < ground {
            spray.position[slot][1] = ground;
            spray.velocity[slot][0] *= 0.2;
            spray.velocity[slot][1] = 0.0;
            spray.velocity[slot][2] *= 0.2;
            spray.age[slot] += step * 2.5;
        }

        let growth = if spray.kind[slot] > 0.5 {
            1.0
        } else {
            1.0 + aged * 1.3
        };
        let opacity = (aged * 8.0).min(1.0) * (1.0 - aged) * (1.0 - aged);

        spray.texels[first] = spray.position[slot][0];
        spray.texels[first + 1] = spray.position[slot][1];
        spray.texels[first + 2] = spray.position[slot][2];
        spray.texels[first + 3] = spray.size[slot] * growth;
        spray.texels[second] = aged;
        spray.texels[second + 1] = spray.seed[slot];
        spray.texels[second + 2] = spray.kind[slot];
        spray.texels[second + 3] = opacity;
        live += 1;
    }
    spray.live = live;
}

/// This frame's particle state, two rows of the data texture.
pub fn texels(spray: &Spray) -> &[f32] {
    &spray.texels
}
