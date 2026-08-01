use crate::settings::Settings;
use crate::systems::Spray;
use crate::systems::character::Character;
use crate::systems::spray;
use crate::systems::terrain;
use crate::systems::terrain::Heightfield;
use rand::Rng;

/// Spine capacity.
pub const SPINE_MAX: usize = 96;

/// Metres of travel between committed samples.
const SPINE_STEP: f32 = 0.30;

/// Seconds a thrown wall of snow stays up.
const LIFE: f32 = 0.88;

/// How far ahead of the player the bow sits, in metres.
const BOW_LEAD: f32 = 0.55;

/// Peak wall height at a full-speed hard carve, in metres.
const MAX_HEIGHT: f32 = 2.4;

/// Mesh lattice.
pub const WAKE_COLS: usize = 128;
pub const WAKE_ROWS: usize = 18;

/// The snow-surf wake: the wave, the plume, and the bow.
pub struct Wake {
    position: [[f32; 3]; SPINE_MAX],
    /// The rider's right vector in the horizontal plane, per sample.
    right: [[f32; 2]; SPINE_MAX],
    /// Odometer reading when the sample was laid, in metres.
    travel: [f32; SPINE_MAX],
    /// Clock reading when the sample was laid, in seconds.
    laid: [f32; SPINE_MAX],
    /// Wave strength captured at lay time.
    strength: [f32; SPINE_MAX],
    /// Signed carve captured at lay time.
    carve: [f32; SPINE_MAX],

    /// Newest sample.
    head: usize,
    pub count: usize,
    odometer: f32,
    pub clock: f32,
    active: bool,
    plume_owed: f32,
    drift_owed: f32,

    /// Per-column resolved amplitude, so the plume can be emitted from the crest the
    /// mesh is actually drawing rather than from a second estimate.
    amplitude: [[f32; 2]; SPINE_MAX],
    distance: [f32; SPINE_MAX],
    /// Ring index of each column, newest first.
    column: [usize; SPINE_MAX],

    /// Three rows per sample, in the layout the shape library reads.
    texels: Vec<f32>,
    /// The largest amplitude anywhere on the wave, in metres.
    pub peak: f32,
}

impl Default for Wake {
    fn default() -> Self {
        Self {
            position: [[0.0; 3]; SPINE_MAX],
            right: [[1.0, 0.0]; SPINE_MAX],
            travel: [0.0; SPINE_MAX],
            laid: [0.0; SPINE_MAX],
            strength: [0.0; SPINE_MAX],
            carve: [0.0; SPINE_MAX],
            head: 0,
            count: 0,
            odometer: 0.0,
            clock: 0.0,
            active: false,
            plume_owed: 0.0,
            drift_owed: 0.0,
            amplitude: [[0.0; 2]; SPINE_MAX],
            distance: [0.0; SPINE_MAX],
            column: [0; SPINE_MAX],
            texels: vec![0.0; SPINE_MAX * 3 * 4],
            peak: 0.0,
        }
    }
}

/// True when there is a wave worth drawing.
pub fn visible(wake: &Wake) -> bool {
    wake.count >= 2 && wake.peak > 0.01
}

pub fn texels(wake: &Wake) -> &[f32] {
    &wake.texels
}

/// Advances the spine, resolves the wave and stages the plume.
pub fn update(
    wake: &mut Wake,
    delta_time: f32,
    settings: &Settings,
    character: &Character,
    heightfield: &Heightfield,
    spray: &mut Spray,
) {
    wake.clock += delta_time;
    wake.odometer += (character.velocity.x.hypot(character.velocity.z)) * delta_time;

    // The rider cuts only while the feet are on the snow. The spine survives a
    // quarter second, long enough for a jump to land back into the wake it
    // left.
    let active = !character.airborne && character.surf > 0.06 && character.speed > 1.6;
    if active {
        if !wake.active {
            maybe_restart(wake);
        }
        write_head(wake, character, heightfield);
        wake.active = true;
    } else {
        wake.active = false;
    }

    retire(wake);
    resolve(wake, settings);
    plume(wake, delta_time, settings, character, spray);
}

/// A new run starts a new spine rather than continuing the last one.
fn maybe_restart(wake: &mut Wake) {
    if wake.count == 0 {
        return;
    }
    if wake.clock - wake.laid[wake.head] > 0.25 {
        wake.count = 0;
    }
}

/// Writes, or rewrites, the live bow sample, and commits it once it has moved a
/// full step.
fn write_head(wake: &mut Wake, character: &Character, heightfield: &Heightfield) {
    let head = wake.head;
    let (sin_facing, cos_facing) = character.facing.sin_cos();
    let bow = [
        character.position.x + sin_facing * BOW_LEAD,
        character.position.z + cos_facing * BOW_LEAD,
    ];

    wake.position[head] = [
        bow[0],
        terrain::height_at(heightfield, bow[0], bow[1]),
        bow[1],
    ];
    wake.right[head] = [cos_facing, -sin_facing];
    wake.travel[head] = wake.odometer;
    wake.laid[head] = wake.clock;
    wake.strength[head] = character.surf * ((character.speed - 2.2) / 9.0).clamp(0.0, 1.0);
    wake.carve[head] = character.carve;

    if wake.count == 0 {
        wake.count = 1;
        return;
    }

    let previous = (head + SPINE_MAX - 1) % SPINE_MAX;
    let dx = wake.position[head][0] - wake.position[previous][0];
    let dz = wake.position[head][2] - wake.position[previous][2];
    if wake.count == 1 || dx * dx + dz * dz >= SPINE_STEP * SPINE_STEP {
        wake.head = (head + 1) % SPINE_MAX;
        if wake.count < SPINE_MAX {
            wake.count += 1;
        }
        let next = wake.head;
        wake.position[next] = wake.position[head];
        wake.right[next] = wake.right[head];
        wake.travel[next] = wake.travel[head];
        wake.laid[next] = wake.laid[head];
        wake.strength[next] = wake.strength[head];
        wake.carve[next] = wake.carve[head];
    }
}

/// Drops samples that have finished collapsing.
fn retire(wake: &mut Wake) {
    while wake.count > 0 {
        let tail = (wake.head + SPINE_MAX + 1 - wake.count) % SPINE_MAX;
        if wake.clock - wake.laid[tail] <= LIFE {
            break;
        }
        wake.count -= 1;
    }
}

/// Resolves every column's amplitude and curl and writes the data texture.
fn resolve(wake: &mut Wake, settings: &Settings) {
    let height_scale = MAX_HEIGHT * settings.wake_height;
    let mut peak = 0.0_f32;

    for step in 0..wake.count {
        let index = (wake.head + SPINE_MAX - step) % SPINE_MAX;
        wake.column[step] = index;

        let distance = wake.odometer - wake.travel[index] + BOW_LEAD;
        let age = ((wake.clock - wake.laid[index]) / LIFE).clamp(0.0, 1.0);

        let shape = 0.34 + 0.66 * smoothstep01((distance - 0.3) / 1.3);
        let envelope = (1.0 - age) * (1.0 - age);
        let base = height_scale * wake.strength[index] * shape * envelope;

        let bias = wake.carve[index].clamp(-1.0, 1.0);

        let left = base * (0.45 + 0.55 * bias).clamp(0.05, 1.0);
        let right = base * (0.45 - 0.55 * bias).clamp(0.05, 1.0);
        let curl_left = (0.42 + 0.58 * bias).clamp(0.26, 1.0);
        let curl_right = (0.42 - 0.58 * bias).clamp(0.26, 1.0);

        peak = peak.max(left).max(right);
        wake.amplitude[step] = [left, right];
        wake.distance[step] = distance;

        let place = step * 4;
        let basis = (SPINE_MAX + step) * 4;
        let shaping = (SPINE_MAX * 2 + step) * 4;
        let point = wake.position[index];
        wake.texels[place..place + 4].copy_from_slice(&[point[0], point[1], point[2], distance]);
        wake.texels[basis..basis + 4].copy_from_slice(&[
            wake.right[index][0],
            wake.right[index][1],
            left,
            right,
        ]);
        wake.texels[shaping..shaping + 4].copy_from_slice(&[curl_left, curl_right, age, 0.0]);
    }

    wake.peak = peak;
}

/// Spray off the lip.
fn plume(
    wake: &mut Wake,
    delta_time: f32,
    settings: &Settings,
    character: &Character,
    spray: &mut Spray,
) {
    if wake.count < 3 || character.surf < 0.15 || character.speed < 3.0 {
        wake.plume_owed = 0.0;
        // Reset alongside the plume, or a pause banks metres of drift and
        // dumps the whole backlog on the frame the next run starts.
        wake.drift_owed = 0.0;
        return;
    }

    let travelled = character.speed * delta_time;
    wake.plume_owed += travelled;
    wake.drift_owed += travelled;
    let mut random = rand::rng();

    let per_metre = 88.0 * settings.wake_spray;
    let mut count = (wake.plume_owed * per_metre) as usize;
    if count > 0 {
        wake.plume_owed -= count as f32 / per_metre;
        count = count.min(150);

        // Inclusive, because a spine at the minimum length makes this zero
        // wide and an exclusive range with no values in it is a panic.
        let span = (wake.count - 1).min(15) as f32;

        for _ in 0..count {
            let along = random.random_range(0.0..=span);
            let Some(sample) = interpolate(wake, along) else {
                continue;
            };

            let total = sample.amplitude[0] + sample.amplitude[1];
            if total < 0.12 {
                continue;
            }
            let side = if random.random_range(0.0..total) < sample.amplitude[0] {
                -1.0
            } else {
                1.0
            };
            let amplitude = if side < 0.0 {
                sample.amplitude[0]
            } else {
                sample.amplitude[1]
            };
            if amplitude < 0.10 {
                continue;
            }

            let forward = [-sample.right[1], sample.right[0]];

            let base = 0.24 + 0.44 * smoothstep01((sample.distance - 0.3) / 2.3);
            let lateral = base + random.random_range(0.35..0.90) * amplitude;
            let point = [
                sample.position[0] + sample.right[0] * side * lateral,
                sample.position[1]
                    + (0.30 + 0.82 * random.random_range(0.0..1.0_f32).sqrt()) * amplitude,
                sample.position[2] + sample.right[1] * side * lateral,
            ];

            if random.random_range(0.0..1.0_f32) < 0.72 {
                spray::emit(
                    spray,
                    point,
                    [
                        sample.right[0] * side * random.random_range(0.4..1.5)
                            + character.velocity.x * 0.16,
                        random.random_range(0.9..2.7),
                        sample.right[1] * side * random.random_range(0.4..1.5)
                            + character.velocity.z * 0.16,
                    ],
                    random.random_range(0.055..0.140),
                    random.random_range(0.34..0.74),
                    0.0,
                    Some(4.5),
                );
                continue;
            }

            let out = random.random_range(1.2..3.8);
            let back = random.random_range(0.4..2.6);
            let clod = random.random_range(0.0..1.0_f32) < 0.18;
            spray::emit(
                spray,
                point,
                [
                    sample.right[0] * side * out - forward[0] * back + character.velocity.x * 0.30,
                    random.random_range(1.6..5.0) + amplitude * 1.5,
                    sample.right[1] * side * out - forward[1] * back + character.velocity.z * 0.30,
                ],
                if clod {
                    random.random_range(0.020..0.042)
                } else {
                    random.random_range(0.045..0.100)
                },
                if clod {
                    random.random_range(0.7..1.2)
                } else {
                    random.random_range(0.9..2.2)
                },
                if clod { 1.0 } else { 0.0 },
                Some(if clod {
                    0.7
                } else {
                    random.random_range(1.0..1.8)
                }),
            );
        }
    }

    let drift_per_metre = 7.0 * settings.wake_spray;
    let mut drift = (wake.drift_owed * drift_per_metre) as usize;
    if drift == 0 {
        return;
    }
    wake.drift_owed -= drift as f32 / drift_per_metre;
    drift = drift.min(14);
    // Inclusive: at the shortest spine that reaches here this is zero wide,
    // and an exclusive range with no values in it is a panic rather than a
    // degenerate draw.
    let span = wake.count.saturating_sub(3).min(22) as f32;

    for _ in 0..drift {
        let Some(sample) = interpolate(wake, 2.0 + random.random_range(0.0..=span)) else {
            continue;
        };
        let lateral = random.random_range(-0.8..0.8);
        spray::emit(
            spray,
            [
                sample.position[0] + sample.right[0] * lateral,
                sample.position[1] + random.random_range(0.08..0.43),
                sample.position[2] + sample.right[1] * lateral,
            ],
            [
                random.random_range(-0.55..0.55),
                random.random_range(0.25..1.15),
                random.random_range(-0.55..0.55),
            ],
            random.random_range(0.026..0.062),
            random.random_range(1.5..3.1),
            0.0,
            Some(4.5),
        );
    }
}

/// The spine between two columns, at a fractional column index.
fn interpolate(wake: &Wake, along: f32) -> Option<Sample> {
    let first = along as usize;
    if first >= wake.count {
        return None;
    }
    let second = (first + 1).min(wake.count - 1);
    let blend = along - first as f32;

    let a = wake.column[first];
    let b = wake.column[second];
    let lerp = |from: f32, to: f32| from + (to - from) * blend;

    Some(Sample {
        position: [
            lerp(wake.position[a][0], wake.position[b][0]),
            lerp(wake.position[a][1], wake.position[b][1]),
            lerp(wake.position[a][2], wake.position[b][2]),
        ],
        right: [
            lerp(wake.right[a][0], wake.right[b][0]),
            lerp(wake.right[a][1], wake.right[b][1]),
        ],
        amplitude: [
            lerp(wake.amplitude[first][0], wake.amplitude[second][0]),
            lerp(wake.amplitude[first][1], wake.amplitude[second][1]),
        ],
        distance: lerp(wake.distance[first], wake.distance[second]),
    })
}

/// One interpolated point on the spine, as the plume sees it.
struct Sample {
    position: [f32; 3],
    right: [f32; 2],
    amplitude: [f32; 2],
    distance: f32,
}

/// Hermite smoothstep on an already normalised parameter.
fn smoothstep01(t: f32) -> f32 {
    let x = t.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}
