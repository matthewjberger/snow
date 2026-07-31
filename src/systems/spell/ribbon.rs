use crate::ecs::{RIBBON, Ribbon};
use crate::math::exp_damp;
use crate::systems::deform;
use crate::systems::deform::Brush;
use crate::systems::spell::bending::{smooth01, transport};
use crate::systems::spell::water::{self, PROFILE_TUBE, STRAND_COLS, WaterBody};
use crate::systems::spell::{self, Cast};
use crate::systems::spray;
use crate::systems::terrain;
use nightshade::prelude::*;
use rand::Rng;

/// Live spine samples, capped by the strand table's width.
const SAMPLES: usize = 46;

/// Metres of tip travel between committed samples.
const STEP: f32 = 0.20;

/// Seconds a sample survives once the body has been thrown.
const TAIL_LIFE: f32 = 1.25;

/// Speed the thrown head builds to, in metres a second.
///
/// Tuned for the camera rather than the physics: the throw goes away from the
/// viewer, so a body flying flat out foreshortens to a dot within half a second.
/// This is plainly fast and stays broadside long enough to be watched.
const THROW_SPEED: f32 = 21.0;

/// How fast the head turns onto the aim after release, per second.
///
/// Deliberately unhurried. Snapping the velocity onto the aim makes the body a
/// straight line immediately, and a straight line pointing at the horizon is the
/// least legible thing this spell could do.
const THROW_STEER: f32 = 5.5;

/// Tube radius at the fat part of the body, in metres.
const RADIUS: f32 = 0.205;

/// How much wider the section is than it is thick.
///
/// A body of bent water is not a hose. It is a ribbon: flattened, twisting as it
/// goes, catching the light on the broad face and vanishing to an edge when it
/// turns side on. A circular section presents the same silhouette from every
/// direction, which is what makes it read as a cylinder.
const SECTION_ASPECT: f32 = 1.55;

/// The ribbon is a hold rather than a press, so this is polled every frame.
///
/// Holding while a thrown body is still flying takes it back rather than
/// starting a second one: the spine it has is the spine it keeps, which is what
/// makes catching your own throw read as catching it.
pub fn hold(world: &mut World, cast: &mut Cast, held: bool) {
    let standing = spell::live::<Ribbon>(world, RIBBON);

    match (held, standing) {
        (true, None) => {
            let mut ribbon = Ribbon {
                held: true,
                strand: water::acquire(cast.water),
                tip: cast.hand,
                ..Default::default()
            };
            if ribbon.strand.is_none() {
                return;
            }
            ribbon.blend = 0.0;
            world.spawn_with((ribbon,));
        }
        (true, Some((entity, mut ribbon))) => {
            if !ribbon.held {
                ribbon.held = true;
                world.set(entity, ribbon);
            }
        }
        (false, Some((entity, mut ribbon))) => {
            if ribbon.held {
                throw(&mut ribbon, cast);
                world.set(entity, ribbon);
            }
        }
        (false, None) => {}
    }
}

/// Runs the body, despawning it once it has spent itself.
pub fn update(world: &mut World, cast: &mut Cast) {
    let Some((entity, mut ribbon)) = spell::live::<Ribbon>(world, RIBBON) else {
        return;
    };
    if tick(&mut ribbon, cast) {
        world.set(entity, ribbon);
    } else {
        release(&mut ribbon, cast.water);
        world.despawn_recursive(entity);
    }
}

/// Drops the body, for the settings toggle or a lost pointer capture, neither
/// of which is the player throwing anything.
pub fn cancel_all(world: &mut World, water: &mut WaterBody) {
    let Some((entity, mut ribbon)) = spell::live::<Ribbon>(world, RIBBON) else {
        return;
    };
    release(&mut ribbon, water);
    world.despawn_recursive(entity);
}

/// Called on the frame the key comes up: the body is thrown, not dropped.
///
/// The throw is not a translation of the mesh. The head keeps being a moving
/// point with a velocity and the spine keeps recording where it has been; all
/// that changes is what drives the head. That is the difference between
/// throwing the ribbon and the ribbon being thrown, because the water arcs
/// onto the target and the bend it had when you let go is still in the tail
/// on its way out.
fn throw(ribbon: &mut Ribbon, cast: &mut Cast) {
    ribbon.held = false;
    ribbon.thrown = true;
    ribbon.splashed = false;
    ribbon.throw_time = 0.0;

    // Slightly above the aim: a thrown body has to arc, and starting it dead
    // flat means it only ever falls.
    let mut aim = [cast.aim[0], cast.aim[1] + 0.18, cast.aim[2]];
    let length = (aim[0] * aim[0] + aim[1] * aim[1] + aim[2] * aim[2])
        .sqrt()
        .max(1e-6);
    aim = [aim[0] / length, aim[1] / length, aim[2] / length];
    ribbon.throw_aim = aim;

    burst(ribbon, cast);
}

/// A shear of droplets off the whole body at the moment of release.
fn burst(ribbon: &Ribbon, cast: &mut Cast) {
    if ribbon.count < 3 {
        return;
    }
    let total = (70.0 * cast.spray_scale) as usize;
    let mut random = rand::rng();
    for _ in 0..total {
        let step = 1 + random.random_range(0..ribbon.count - 2);
        let sample = ribbon.position[(ribbon.head + SAMPLES * 2 - step) % SAMPLES];
        let thrust = random.random_range(4.0..13.0);
        spray::emit(
            cast.spray,
            [
                sample[0] + random.random_range(-0.15..0.15),
                sample[1] + random.random_range(-0.15..0.15),
                sample[2] + random.random_range(-0.15..0.15),
            ],
            [
                ribbon.throw_aim[0] * thrust + random.random_range(-1.2..1.2),
                ribbon.throw_aim[1] * thrust + random.random_range(0.8..2.8),
                ribbon.throw_aim[2] * thrust + random.random_range(-1.2..1.2),
            ],
            random.random_range(0.020..0.060),
            random.random_range(0.6..1.5),
            1.0,
            Some(0.7),
        );
    }
}

/// Advances the body, reporting whether it is still there.
fn tick(ribbon: &mut Ribbon, cast: &mut Cast) -> bool {
    let Some(strand) = ribbon.strand else {
        return false;
    };
    let delta_time = cast.delta_time;

    // A thrown body does not thin out while it is still flying: it is all
    // still there, travelling. It only gives out once it has spent itself.
    let want = if ribbon.held {
        1.0
    } else if ribbon.thrown {
        (1.0 - (ribbon.throw_time - 1.5) / 1.0).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let rate = if ribbon.held { 5.5 } else { 3.4 };
    ribbon.blend = exp_damp(ribbon.blend, want, rate, delta_time);

    if ribbon.held {
        drive_tip(ribbon, cast);
    } else {
        retire(ribbon, cast);
    }

    if !ribbon.held && (ribbon.count < 3 || ribbon.blend < 0.02) {
        return false;
    }

    write_strand(ribbon, strand, cast);
    score(ribbon, cast);
    shed(ribbon, cast);
    true
}

/// A critically damped spring toward a target that is itself moving on a slow
/// figure of eight.
///
/// The spring is what makes the water heavy: at these rates the tip overshoots
/// a fast camera swing and comes back, which is what a mass on the end of an
/// arc does and exactly what a direct assignment would throw away.
fn drive_tip(ribbon: &mut Ribbon, cast: &mut Cast) {
    let step = cast.delta_time.min(1.0 / 60.0);
    ribbon.phase += cast.delta_time * 2.55;

    // A two to one Lissajous in the plane the camera is looking through, so
    // it always reads as a figure of eight rather than as a shape seen edge
    // on. Two extra harmonics, both incommensurate with the fundamental and
    // with each other, because a pure ratio closes on itself every cycle and
    // the ribbon lies on top of its own previous pass.
    let across = ribbon.phase.sin() * 1.70 + (ribbon.phase * 0.41 + 1.7).sin() * 0.44;
    let vertical =
        (ribbon.phase * 2.0 + 0.4).sin() * 0.92 + (ribbon.phase * 0.73 + 0.2).sin() * 0.26;

    // Reach out along the aim, then swing. The pattern sits high enough that
    // the bottom lobe only occasionally reaches the snow: scoring on every
    // pass turns a trace into a furrow, and a ribbon permanently in contact
    // stops reading as something held in the air.
    const REACH: f32 = 2.5;
    let mut target = [0.0_f32; 3];
    for (axis, component) in target.iter_mut().enumerate() {
        *component = cast.hand[axis]
            + cast.aim[axis] * REACH
            + cast.right[axis] * across
            + cast.up[axis] * vertical;
    }
    target[1] += 0.34;

    // Stiff and close to critical. Slacker and the tip spends each cycle
    // catching up in a straight line and then turning hard at the ends, which
    // squares off the loops: momentum should round the path, not corner it.
    const SPRING: f32 = 210.0;
    let damping = 2.0 * SPRING.sqrt() * 0.92;
    for (axis, want) in target.iter().enumerate() {
        ribbon.velocity[axis] +=
            (SPRING * (want - ribbon.tip[axis]) - damping * ribbon.velocity[axis]) * step;
        ribbon.tip[axis] += ribbon.velocity[axis] * step;
    }

    // Never let the tip bore into the ground; it skims it instead, which is
    // where the scoring comes from.
    let ground = terrain::height_at(cast.heightfield, ribbon.tip[0], ribbon.tip[2]) + 0.10;
    if ribbon.tip[1] < ground {
        ribbon.tip[1] = ground;
        if ribbon.velocity[1] < 0.0 {
            ribbon.velocity[1] *= -0.25;
        }
    }

    commit(ribbon);
}

/// Appends the tip to the spine once it has moved a full step.
fn commit(ribbon: &mut Ribbon) {
    if ribbon.count == 0 {
        push(ribbon);
        return;
    }
    let newest = ribbon.position[ribbon.head];
    let delta = [
        ribbon.tip[0] - newest[0],
        ribbon.tip[1] - newest[1],
        ribbon.tip[2] - newest[2],
    ];
    if delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2] >= STEP * STEP {
        push(ribbon);
    }
}

fn push(ribbon: &mut Ribbon) {
    ribbon.head = (ribbon.head + 1) % SAMPLES;
    if ribbon.count < SAMPLES {
        ribbon.count += 1;
    }
    ribbon.position[ribbon.head] = ribbon.tip;
    ribbon.speed[ribbon.head] = (ribbon.velocity[0] * ribbon.velocity[0]
        + ribbon.velocity[1] * ribbon.velocity[1]
        + ribbon.velocity[2] * ribbon.velocity[2])
        .sqrt();
}

/// After release: flies the head and drains the tail behind it.
///
/// The head is integrated exactly as it was while held, so the moment of
/// release is continuous in both position and velocity and there is nothing
/// to ease. What changes is the force on it.
fn retire(ribbon: &mut Ribbon, cast: &mut Cast) {
    let delta_time = cast.delta_time;
    if ribbon.thrown && ribbon.count > 0 {
        ribbon.throw_time += delta_time;
        let step = delta_time.min(1.0 / 60.0);

        // The velocity direction turns toward the aim rather than being
        // replaced by it, so the head curves out of whatever part of the
        // figure of eight it was in.
        let blend = 1.0 - (-THROW_STEER * step).exp();
        let speed = (ribbon.velocity[0] * ribbon.velocity[0]
            + ribbon.velocity[1] * ribbon.velocity[1]
            + ribbon.velocity[2] * ribbon.velocity[2])
            .sqrt();
        for axis in 0..3 {
            ribbon.velocity[axis] +=
                (ribbon.throw_aim[axis] * speed - ribbon.velocity[axis]) * blend;
        }

        // Thrust for the first third of a second, then quadratic drag takes
        // over and it coasts. Accelerating rather than starting at speed is
        // what makes it read as being sent.
        let thrust = 62.0 * (-ribbon.throw_time * 3.0).exp();
        for axis in 0..3 {
            ribbon.velocity[axis] += ribbon.throw_aim[axis] * thrust * step;
        }
        ribbon.velocity[1] -= 9.81 * step;

        let speed = (ribbon.velocity[0] * ribbon.velocity[0]
            + ribbon.velocity[1] * ribbon.velocity[1]
            + ribbon.velocity[2] * ribbon.velocity[2])
            .sqrt();
        if speed > 0.001 {
            let drag = ((0.55 + speed * speed * 0.0016) * step).min(1.0);
            for axis in 0..3 {
                ribbon.velocity[axis] -= ribbon.velocity[axis] * drag;
            }
        }
        if speed > THROW_SPEED {
            let scale = THROW_SPEED / speed;
            for axis in 0..3 {
                ribbon.velocity[axis] *= scale;
            }
        }

        for axis in 0..3 {
            ribbon.tip[axis] += ribbon.velocity[axis] * step;
        }

        // A thrown body of water that meets the ground does not keep going.
        // Clamping the head to the surface and carrying on makes a released
        // ribbon slither across the snow like a snake, which is the one
        // reading it must not have. It bursts instead.
        let ground = terrain::height_at(cast.heightfield, ribbon.tip[0], ribbon.tip[2]) + 0.05;
        if !ribbon.splashed && ribbon.tip[1] < ground {
            ribbon.tip[1] = ground;
            splash(ribbon, cast);
        }

        if ribbon.splashed {
            ribbon.velocity = [0.0; 3];
        } else {
            commit(ribbon);
        }
    }

    // The tail drains from behind. While the head is still flying this only
    // holds the body to a fixed length; once it slows, the drain outruns it.
    // The rate climbs with time so the spell always terminates.
    ribbon.retire_owed += delta_time;
    let rate = if ribbon.splashed {
        7.0
    } else {
        1.0 + ribbon.throw_time * 0.9
    };
    let per_sample = TAIL_LIFE / SAMPLES as f32 / rate;
    while ribbon.retire_owed >= per_sample && ribbon.count > 0 {
        ribbon.retire_owed -= per_sample;
        ribbon.count -= 1;
    }
}

/// The body meets the ground: a fan of droplets, a mark, and a hard
/// acceleration of the drain so the rest visibly pours into the impact.
///
/// The fan is deliberately wide and low. A vertical burst reads as an
/// explosion, and water hitting a surface at a shallow angle mostly goes
/// sideways: the ring of it skating outward is the thing that says liquid.
fn splash(ribbon: &mut Ribbon, cast: &mut Cast) {
    ribbon.splashed = true;
    let [x, y, z] = ribbon.tip;

    // The incoming direction carries into the fan, so a shallow throw sprays
    // forward and a steep one sprays evenly.
    let speed = (ribbon.velocity[0] * ribbon.velocity[0]
        + ribbon.velocity[1] * ribbon.velocity[1]
        + ribbon.velocity[2] * ribbon.velocity[2])
        .sqrt()
        .max(1e-6);
    let incoming = [ribbon.velocity[0] / speed, ribbon.velocity[2] / speed];
    let steep = (ribbon.velocity[1].abs() / speed).min(1.0);

    let total = ((280.0 + 190.0 * (1.0 - steep)) * cast.spray_scale) as usize;
    let mut random = rand::rng();
    for _ in 0..total {
        let angle = random.random_range(0.0..std::f32::consts::TAU);
        let (sine, cosine) = angle.sin_cos();
        // Biased downrange: the water keeps most of its momentum, and the
        // tall part of a splash is the minority of it.
        let out = random.random_range(1.8..7.3) * (0.45 + 0.85 * (1.0 - steep));
        let drop = random.random_range(0.0..1.0_f32) < 0.55;
        spray::emit(
            cast.spray,
            [
                x + cosine * 0.12,
                y + 0.04 + random.random_range(0.0..0.12),
                z + sine * 0.12,
            ],
            [
                cosine * out + incoming[0] * speed * 0.32,
                random.random_range(1.2..5.8) * (0.4 + 0.8 * steep),
                sine * out + incoming[1] * speed * 0.32,
            ],
            if drop {
                random.random_range(0.020..0.054)
            } else {
                random.random_range(0.055..0.150)
            },
            random.random_range(0.6..1.7),
            if drop { 1.0 } else { 0.0 },
            Some(if drop { 0.6 } else { 2.2 }),
        );
    }

    // Shallower than a crater and much wetter: this is water landing, so it
    // packs and glazes far more than it displaces.
    deform::brush(
        cast.deform,
        &Brush {
            x,
            z,
            radius: 0.62,
            depth: 0.16,
            berm: 0.13,
            compression: 1.0,
            ice: 0.85,
            yaw: incoming[1].atan2(incoming[0]),
            elongation: 1.35,
            edge: 1.0,
        },
    );
    for _ in 0..3 {
        let angle = random.random_range(0.0..std::f32::consts::TAU);
        let distance = random.random_range(0.55..1.2);
        deform::brush(
            cast.deform,
            &Brush {
                x: x + angle.cos() * distance,
                z: z + angle.sin() * distance,
                radius: random.random_range(0.30..0.52),
                depth: 0.05,
                berm: 0.07,
                compression: 0.6,
                ice: 0.5,
                yaw: angle,
                elongation: 1.3,
                edge: 1.0,
            },
        );
    }

    cast.trauma = cast.trauma.max(0.09);
}

/// Resolves the spine into the strand table.
///
/// Column zero is the live tip rather than the newest committed sample.
/// Samples are committed every fifth of a metre of head travel, so a spine
/// drawn only from committed samples has a head that advances in jumps, and
/// at a walking swing that is a visible stutter at the leading edge, which is
/// the part the eye is locked onto.
fn write_strand(ribbon: &mut Ribbon, strand: usize, cast: &mut Cast) {
    let count = (ribbon.count + 1).min(STRAND_COLS);
    if count < 3 {
        water::set_params(cast.water, strand, PROFILE_TUBE, 0.12, 0.0, 0);
        return;
    }

    // Seed the frame at the tip with something perpendicular to the first
    // tangent, then transport it down the spine.
    let mut previous = ribbon.tip;
    let newest = ribbon.position[ribbon.head];
    let mut tangent = [
        previous[0] - newest[0],
        previous[1] - newest[1],
        previous[2] - newest[2],
    ];
    let mut length =
        (tangent[0] * tangent[0] + tangent[1] * tangent[1] + tangent[2] * tangent[2]).sqrt();
    if length < 1e-5 {
        let behind = ribbon.position[(ribbon.head + SAMPLES - 1) % SAMPLES];
        tangent = [
            previous[0] - behind[0],
            previous[1] - behind[1],
            previous[2] - behind[2],
        ];
        length = (tangent[0] * tangent[0] + tangent[1] * tangent[1] + tangent[2] * tangent[2])
            .sqrt()
            .max(1e-6);
    }
    tangent = [
        tangent[0] / length,
        tangent[1] / length,
        tangent[2] / length,
    ];

    // Any perpendicular will do for the seed; the transport takes it from
    // there and the section is round anyway.
    let mut right = [-tangent[2], 0.0, tangent[0]];
    let mut right_length = (right[0] * right[0] + right[2] * right[2]).sqrt();
    if right_length < 1e-4 {
        right = [1.0, 0.0, 0.0];
        right_length = 1.0;
    }
    right = [
        right[0] / right_length,
        right[1] / right_length,
        right[2] / right_length,
    ];

    let mut distance = 0.0_f32;
    let twist = cast.time * 2.4;

    for column in 0..count {
        // Column zero is the live tip; the rest walk the ring backwards.
        let point = if column == 0 {
            ribbon.tip
        } else {
            ribbon.position[(ribbon.head + SAMPLES * 2 - (column - 1)) % SAMPLES]
        };
        let sample = (ribbon.head + SAMPLES * 2 - column.saturating_sub(1)) % SAMPLES;

        if column > 0 {
            let delta = [
                previous[0] - point[0],
                previous[1] - point[1],
                previous[2] - point[2],
            ];
            let span = (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt();
            // The tip can sit arbitrarily close to the sample behind it, and
            // right after a commit it sits exactly on it, so a degenerate
            // segment is normal and must not produce an undefined tangent.
            if span > 1e-5 {
                distance += span;
                let next = [-delta[0] / span, -delta[1] / span, -delta[2] / span];
                right = transport(right, tangent, next);
                tangent = next;
            }
        }

        let u = column as f32 / (count - 1) as f32;
        // A pointed head, a shoulder just behind it, and a continuous taper
        // to nothing, with no plateau: a constant radius over any part of the
        // body is the definition of a cylinder. The exponent is close to
        // linear so the body carries almost its whole length.
        let profile = smooth01(u / 0.10) * (1.0 - u).powf(1.05);

        // Thickness from the speed the tip had when this sample was laid, so
        // the body varies without anything periodic in it.
        let stretch = (1.35 - ribbon.speed[sample] * 0.055).clamp(0.55, 1.35);
        let radius = RADIUS * profile * stretch * ribbon.blend;

        // Flattened where it is skimming the snow, on top of the ribbon's own
        // ellipse: water running over a surface spreads across it.
        let clearance = point[1] - terrain::height_at(cast.heightfield, point[0], point[2]);
        let ground = 1.0 - ((clearance - 0.06) / 0.35).clamp(0.0, 1.0);
        let flatten = SECTION_ASPECT * (1.0 - 0.72 * ground);

        // Foam at the head where it is tearing through the air, again where
        // it drags on the ground, and again where the body is stretched thin,
        // because that is where a stream tears.
        let foam = ((1.0 - smooth01(u / 0.16)) * 0.55 + ground * 0.5 + (1.0 - stretch) * 0.45)
            .clamp(0.0, 1.0);

        // The section rolls as it goes, which with an elliptical section
        // turns the broad face over along the body.
        water::column(
            cast.water,
            strand,
            column,
            point,
            radius,
            right,
            twist + distance * 1.35,
            distance,
            u,
            foam,
            flatten,
        );

        previous = point;
    }

    water::set_params(
        cast.water,
        strand,
        PROFILE_TUBE,
        0.14,
        (ribbon.blend * 1.3).clamp(0.0, 1.0),
        count,
    );
}

/// Thin curved lines scored in the snow, only where the body is low enough to
/// touch, and shallow, so the trace of a figure of eight is still legible a
/// minute later.
fn score(ribbon: &mut Ribbon, cast: &mut Cast) {
    let count = ribbon.count.min(STRAND_COLS);
    if count < 2 || ribbon.blend < 0.15 {
        return;
    }
    ribbon.score_owed += cast.delta_time;
    if ribbon.score_owed < 1.0 / 60.0 {
        return;
    }
    let weight = ribbon.score_owed.min(0.05);
    ribbon.score_owed = 0.0;

    // The head end only. The tail has already scored whatever it was going to
    // score, and re-cutting it every frame turns a light trace into a gouge.
    let span = (count - 1).min(10);
    for step in (0..=span).step_by(2) {
        let point = ribbon.position[(ribbon.head + SAMPLES * 2 - step) % SAMPLES];
        let clearance = point[1] - terrain::height_at(cast.heightfield, point[0], point[2]);
        if clearance > 0.34 {
            continue;
        }
        let touch = 1.0 - (clearance / 0.34).clamp(0.0, 1.0);
        let scale = weight * touch * ribbon.blend;
        deform::brush(
            cast.deform,
            &Brush {
                x: point[0],
                z: point[2],
                radius: 0.13,
                depth: 1.15 * scale,
                berm: 0.55 * scale,
                // Packed hard by running water, and glazed.
                compression: 2.6 * scale,
                ice: 1.9 * scale,
                yaw: 0.0,
                elongation: 1.0,
                edge: 0.65,
            },
        );
    }
}

/// Droplets shed from the body rather than from the tip.
///
/// A stream under this much lateral acceleration loses water all the way
/// along its outside, and emitting only at the head puts a comet trail behind
/// a shape that is not a comet.
fn shed(ribbon: &mut Ribbon, cast: &mut Cast) {
    if ribbon.count < 4 || ribbon.blend < 0.2 {
        return;
    }
    let rate = 130.0 * cast.spray_scale * ribbon.blend;
    ribbon.spray_owed += cast.delta_time * rate;
    let mut count = ribbon.spray_owed as usize;
    if count == 0 {
        return;
    }
    ribbon.spray_owed -= count as f32;
    count = count.min(30);

    let mut random = rand::rng();
    for _ in 0..count {
        let step = 1 + random.random_range(0..ribbon.count - 2);
        let index = (ribbon.head + SAMPLES * 2 - step) % SAMPLES;
        let point = ribbon.position[index];
        let ahead = ribbon.position[(index + 1) % SAMPLES];
        // Local velocity of the body, from the spine's own spacing.
        let flow = [
            (point[0] - ahead[0]) * 12.0,
            (point[1] - ahead[1]) * 12.0,
            (point[2] - ahead[2]) * 12.0,
        ];

        spray::emit(
            cast.spray,
            [
                point[0] + random.random_range(-0.1..0.1),
                point[1] + random.random_range(-0.1..0.1),
                point[2] + random.random_range(-0.1..0.1),
            ],
            [
                flow[0] * 0.5 + random.random_range(-0.8..0.8),
                flow[1] * 0.5 + random.random_range(0.4..1.6),
                flow[2] * 0.5 + random.random_range(-0.8..0.8),
            ],
            random.random_range(0.022..0.056),
            random.random_range(0.55..1.30),
            // Droplets rather than powder: hard edged and ballistic.
            1.0,
            Some(0.55),
        );
    }
}

/// Hands the strand back, so the pool does not leak a slot per throw.
fn release(ribbon: &mut Ribbon, water: &mut WaterBody) {
    if let Some(strand) = ribbon.strand.take() {
        water::release(water, strand);
    }
}
