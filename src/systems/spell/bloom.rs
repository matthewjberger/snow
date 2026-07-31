use crate::ecs::{BLOOM, Bloom};
use crate::systems::deform;
use crate::systems::deform::Brush;
use crate::systems::spell::bending::{bell, smooth01, transport};
use crate::systems::spell::lights;
use crate::systems::spell::water::{self, PROFILE_TUBE, WaterBody};
use crate::systems::spell::{self, Cast};
use crate::systems::spray;
use nightshade::prelude::*;
use rand::Rng;

const COLS: usize = 34;

/// Full height of the column at peak, in metres.
const HEIGHT: f32 = 5.6;

/// Radius of the column at its widest, in metres.
///
/// An eruption is a mass of material leaving the ground and the aspect ratio is
/// most of what says so. The water's absorption is keyed to the radius as well,
/// so a thin column is also a colourless one.
const GIRTH: f32 = 0.66;

/// Seconds from cast to the column being gone.
const LIFE: f32 = 1.75;

/// Seconds of fallout after that.
const FALLOUT: f32 = 3.4;

/// Casting erupts a column, or restarts the one already up.
pub fn cast(world: &mut World, cast: &mut Cast, target: [f32; 3]) {
    let standing = spell::live::<Bloom>(world, BLOOM);
    let mut bloom = standing.map(|(_, bloom)| bloom).unwrap_or_default();
    begin(&mut bloom, cast.water, target);
    spell::commit(world, standing.map(|(entity, _)| entity), bloom);
}

/// Runs the column, despawning it once the fallout has settled.
pub fn update(world: &mut World, cast: &mut Cast) {
    let Some((entity, mut bloom)) = spell::live::<Bloom>(world, BLOOM) else {
        return;
    };
    if tick(&mut bloom, cast) {
        world.set(entity, bloom);
    } else {
        release(&mut bloom, cast.water);
        world.despawn_recursive(entity);
    }
}

/// Drops the column, for the settings toggle.
pub fn cancel_all(world: &mut World, water: &mut WaterBody) {
    let Some((entity, mut bloom)) = spell::live::<Bloom>(world, BLOOM) else {
        return;
    };
    release(&mut bloom, water);
    world.despawn_recursive(entity);
}

fn begin(bloom: &mut Bloom, water: &mut WaterBody, target: [f32; 3]) {
    if bloom.strand.is_none() {
        bloom.strand = water::acquire(water);
    }
    bloom.centre = target;
    bloom.time = 0.0;
    bloom.burst = false;
    bloom.curtain_owed = 0.0;
    // A different lean each cast, so two blooms in the same place are not the
    // same object twice.
    let angle = rand::rng().random_range(0.0..std::f32::consts::TAU);
    bloom.lean = [angle.cos() * 0.16, angle.sin() * 0.16];
}

/// Advances the column, reporting whether it is still going.
fn tick(bloom: &mut Bloom, cast: &mut Cast) -> bool {
    bloom.time += cast.delta_time;
    if bloom.time >= LIFE + FALLOUT {
        return false;
    }

    // Fires once, on the frame the column reaches the surface, so the crater,
    // the ring of thrown snow and the light spike are all the same event.
    if !bloom.burst && bloom.time >= 0.10 {
        bloom.burst = true;
        crater(bloom, cast);
        throw(bloom, cast);
    }

    column(bloom, cast);
    curtain(bloom, cast);
    true
}

/// The column, wide at the base, waisted in the middle and flared at the
/// head, which is what a real ejection does: the mass at the top has had the
/// longest to spread and the least to hold it together.
///
/// It leans, because a perfectly vertical cylinder of water reads as a
/// rendered primitive no matter what is on it.
fn column(bloom: &mut Bloom, cast: &mut Cast) {
    let Some(strand) = bloom.strand else {
        return;
    };
    let time = bloom.time;

    // Rise, hold, collapse. The collapse runs the height back down rather
    // than fading the alpha, so the column withdraws into the crater.
    let rise = smooth01((time - 0.10) / 0.34);
    let drop = 1.0 - smooth01((time - 0.95) / 0.80);
    let envelope = rise * drop;
    if envelope <= 0.002 {
        water::set_params(cast.water, strand, PROFILE_TUBE, 0.5, 0.0, 0);
        return;
    }

    let top = HEIGHT * envelope;
    let sway = (time * 3.1).sin() * 0.12;

    let mut previous = [0.0_f32; 3];
    let mut right = [1.0, 0.0, 0.0];
    let mut tangent = [0.0, -1.0, 0.0];

    for column in 0..COLS {
        let u = column as f32 / (COLS - 1) as f32;
        // Column zero is the head, so the parameter runs downward. That
        // matches every other strand, where it always means distance behind
        // the leading edge, and keeps the relief drifting the right way.
        let up = 1.0 - u;
        let leaning = up * up;
        let point = [
            bloom.centre[0] + (bloom.lean[0] + sway) * leaning * top * 0.5,
            bloom.centre[1] + top * up,
            bloom.centre[2] + (bloom.lean[1] - sway * 0.6) * leaning * top * 0.5,
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
            next = [next[0] / length, next[1] / length, next[2] / length];
            right = transport(right, tangent, next);
            tangent = next;
        }

        let shape = 0.42
            + 0.58 * bell((up * 1.15).clamp(0.0, 1.0))
            + 0.55 * smooth01((up - 0.72) / 0.28)
            + 0.75 * (1.0 - smooth01(up / 0.22));
        let radius = GIRTH * shape * envelope * (0.9 + 0.2 * (u * 9.0 + time * 6.0).sin());

        // The head is where it is coming apart; the foot is where it is
        // grinding against the crater rim.
        let foam = (0.30 + 0.55 * smooth01((up - 0.55) / 0.45) + 0.4 * (1.0 - smooth01(up / 0.18)))
            .clamp(0.0, 1.0);

        water::column(
            cast.water,
            strand,
            column,
            point,
            radius,
            right,
            time * 1.5 + u * 4.0,
            u * top,
            u,
            foam,
            1.0,
        );
        previous = point;
    }

    water::set_params(
        cast.water,
        strand,
        PROFILE_TUBE,
        0.42,
        (envelope * 1.5).clamp(0.0, 1.0),
        COLS,
    );

    // Two lights. The one down in the crater is what lights the rim and the
    // fallout around the base, and it is the reason the effect reads as a
    // hole full of light rather than a bright column on dark ground.
    lights::add(
        cast.lights,
        [bloom.centre[0], bloom.centre[1] + 0.35, bloom.centre[2]],
        11.0,
        [0.44, 0.78, 1.0],
        22.0 * envelope,
    );
    lights::add(
        cast.lights,
        [
            bloom.centre[0] + bloom.lean[0] * top * 0.5,
            bloom.centre[1] + top * 0.92,
            bloom.centre[2] + bloom.lean[1] * top * 0.5,
        ],
        7.5,
        [0.55, 0.82, 1.0],
        9.0 * envelope,
    );
}

/// One deep brush with a heavy rim, then a broken outer ring thrown clear of
/// it, because a crater with a perfectly even rim is the tell that gives a
/// single radial brush away.
fn crater(bloom: &mut Bloom, cast: &mut Cast) {
    let mut random = rand::rng();
    deform::brush(
        cast.deform,
        &Brush {
            x: bloom.centre[0],
            z: bloom.centre[2],
            radius: 1.15,
            depth: 0.52,
            berm: 0.40,
            compression: 0.72,
            ice: 0.30,
            yaw: random.random_range(0.0..std::f32::consts::PI),
            // Very slightly oval, so it is not a stamped circle.
            elongation: 1.15,
            edge: 1.0,
        },
    );

    for quarter in 0..4 {
        let angle = (quarter as f32 / 4.0) * std::f32::consts::TAU + random.random_range(0.0..1.2);
        let distance = random.random_range(1.5..2.2);
        deform::brush(
            cast.deform,
            &Brush {
                x: bloom.centre[0] + angle.cos() * distance,
                z: bloom.centre[2] + angle.sin() * distance,
                radius: random.random_range(0.5..0.85),
                depth: 0.0,
                berm: random.random_range(0.20..0.34),
                compression: 0.15,
                ice: 0.0,
                yaw: angle,
                elongation: 1.4,
                edge: 1.0,
            },
        );
    }
    cast.trauma = cast.trauma.max(0.28);
}

/// The instant of the burst: a hard ring of thrown snow and water.
fn throw(bloom: &mut Bloom, cast: &mut Cast) {
    let count = (430.0 * cast.spray_scale) as usize;
    let mut random = rand::rng();

    for _ in 0..count {
        let angle = random.random_range(0.0..std::f32::consts::TAU);
        // Biased toward the rim, because that is where the mass leaves.
        let radius = 0.35 + random.random_range(0.0..1.0_f32).sqrt() * 1.25;
        let up = random.random_range(5.5..14.0);
        let out = random.random_range(1.6..6.6);
        let clod = random.random_range(0.0..1.0_f32) < 0.26;

        spray::emit(
            cast.spray,
            [
                bloom.centre[0] + angle.cos() * radius,
                bloom.centre[1] + 0.10 + random.random_range(0.0..0.5),
                bloom.centre[2] + angle.sin() * radius,
            ],
            [
                angle.cos() * out,
                up * if clod { 0.7 } else { 1.0 },
                angle.sin() * out,
            ],
            if clod {
                random.random_range(0.028..0.066)
            } else {
                random.random_range(0.075..0.190)
            },
            if clod {
                random.random_range(1.1..1.9)
            } else {
                random.random_range(1.4..2.9)
            },
            if clod { 1.0 } else { 0.0 },
            // Ballistic, or it never leaves the crater.
            Some(if clod {
                0.65
            } else {
                random.random_range(1.1..1.9)
            }),
        );
    }
}

/// The fallout: fine, slow, high drag, and emitted above the player's eye
/// line over a wide disc, so it drifts down through the frame rather than
/// sitting in a cone over the crater.
///
/// This is the part of the spell that lasts, and where the glinting has the
/// best chance of being seen, since every grain is lit from below by the
/// crater light.
fn curtain(bloom: &mut Bloom, cast: &mut Cast) {
    // Ramps in behind the burst and decays over the whole fallout window.
    let strength = smooth01((bloom.time - 0.25) / 0.5)
        * (1.0 - smooth01((bloom.time - 0.9) / (FALLOUT * 0.9)));
    if strength <= 0.01 {
        return;
    }

    let rate = 360.0 * cast.spray_scale * strength;
    bloom.curtain_owed += cast.delta_time * rate;
    let mut count = bloom.curtain_owed as usize;
    if count == 0 {
        return;
    }
    bloom.curtain_owed -= count as f32;
    count = count.min(60);

    let mut random = rand::rng();
    for _ in 0..count {
        let angle = random.random_range(0.0..std::f32::consts::TAU);
        let radius = random.random_range(0.0..1.0_f32).sqrt() * 3.6;
        spray::emit(
            cast.spray,
            [
                bloom.centre[0] + angle.cos() * radius,
                bloom.centre[1] + random.random_range(2.2..6.4),
                bloom.centre[2] + angle.sin() * radius,
            ],
            [
                random.random_range(-0.45..0.45),
                random.random_range(0.2..1.3),
                random.random_range(-0.45..0.45),
            ],
            random.random_range(0.028..0.083),
            random.random_range(1.6..3.5),
            0.0,
            // High drag: this is meant to hang and settle, not to fly.
            Some(4.6),
        );
    }
}

/// Hands the strand back, so the pool does not leak a slot per cast.
fn release(bloom: &mut Bloom, water: &mut WaterBody) {
    if let Some(strand) = bloom.strand.take() {
        water::release(water, strand);
    }
}
