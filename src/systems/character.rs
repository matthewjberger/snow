use crate::camera;
use crate::camera::CameraRig;
use crate::input::SnowInput;
use crate::math::{angle_damp, angle_delta, clamp01, exp_damp};
use crate::systems::Heightfield;
use crate::systems::terrain;
use nalgebra_glm::Vec3;

const WALK_SPEED: f32 = 2.5;
const RUN_SPEED: f32 = 5.4;
const WALK_ACCEL: f32 = 26.0;
const WALK_DECEL: f32 = 30.0;

const SURF_MAX: f32 = 19.5;
const SURF_THRUST: f32 = 11.0;
const SURF_DRAG: f32 = 0.42;
/// Radians per second at full steer.
const SURF_TURN: f32 = 2.35;
const SURF_GRIP: f32 = 7.5;

/// Metres of travel per full stride cycle, before the speed scaling.
const STRIDE_BASE: f32 = 1.55;

/// Upward speed a jump leaves the ground with, in metres a second.
const JUMP_SPEED: f32 = 6.2;

/// Downward acceleration, in metres a second squared.
///
/// Well above the real figure, which is the usual trade: at 9.81 a jump this
/// high hangs for a second and a third and reads as low gravity. This gives
/// roughly a metre of lift over two thirds of a second.
const GRAVITY: f32 = 18.0;

/// How much of the ground's steering authority survives in the air.
///
/// Enough to adjust a jump in flight, little enough that the launch still
/// decides where it lands.
const AIR_CONTROL: f32 = 0.35;

/// Character locomotion and snow-surf physics.
pub struct Character {
    pub position: Vec3,
    pub velocity: Vec3,
    previous_velocity: Vec3,
    pub acceleration: Vec3,

    /// Yaw, in radians.
    pub facing: f32,
    pub speed: f32,
    /// Normalised against the surf maximum, for the field of view and the wind.
    pub speed01: f32,

    /// Zero walking, one fully surfing.
    pub surf: f32,
    pub surf_active: bool,

    /// Zero not casting, one fully in the bending stance.
    pub cast: f32,
    pub cast_aim: Vec3,

    /// Signed lean, right positive, from lateral acceleration.
    pub lean: f32,
    /// Signed carve amount for shaping the wake.
    pub carve: f32,
    /// How hard the screen-space speed streaks should read.
    pub streak01: f32,

    pub gait_phase: f32,
    /// True when the legs should be running a gait at all.
    pub stepping: bool,
    /// Set true for exactly one frame when a foot plants.
    pub footfall: bool,
    /// Which foot just planted: zero left, one right.
    pub foot_index: usize,
    /// World position of the foot that just planted.
    pub foot_position: Vec3,
    /// Impact strength, which scales the spray and the depression depth.
    pub foot_impact: f32,

    pub ground_height: f32,
    pub ground_normal: Vec3,

    /// Height above the ground, exactly zero while standing on it. The figure
    /// lifts by this, so a grounded frame poses exactly as it always has.
    pub air_height: f32,
    pub airborne: bool,
    /// Vertical speed while airborne, in metres a second.
    pub vertical_velocity: f32,
    /// Set true for exactly one frame on touchdown, with the speed of the
    /// landing in `landing_impact`.
    pub landed: bool,
    pub landing_impact: f32,
}

impl Default for Character {
    fn default() -> Self {
        Self {
            position: Vec3::zeros(),
            velocity: Vec3::zeros(),
            previous_velocity: Vec3::zeros(),
            acceleration: Vec3::zeros(),
            facing: 0.0,
            speed: 0.0,
            speed01: 0.0,
            surf: 0.0,
            surf_active: false,
            cast: 0.0,
            cast_aim: Vec3::new(0.0, 0.0, 1.0),
            lean: 0.0,
            carve: 0.0,
            streak01: 0.0,
            gait_phase: 0.0,
            stepping: true,
            footfall: false,
            foot_index: 0,
            foot_position: Vec3::zeros(),
            foot_impact: 0.0,
            ground_height: 0.0,
            ground_normal: Vec3::new(0.0, 1.0, 0.0),
            air_height: 0.0,
            airborne: false,
            vertical_velocity: 0.0,
            landed: false,
            landing_impact: 0.0,
        }
    }
}

/// Metres of travel per full stride cycle at the current speed.
pub fn stride(character: &Character) -> f32 {
    STRIDE_BASE * (0.72 + 0.28 * (character.speed / RUN_SPEED).min(1.0))
}

pub fn update(
    character: &mut Character,
    delta_time: f32,
    input: &SnowInput,
    rig: &mut CameraRig,
    heightfield: &Heightfield,
) {
    let step = delta_time.min(1.0 / 30.0);

    character.footfall = false;
    character.landed = false;
    if step <= 0.0 {
        return;
    }

    character.previous_velocity = character.velocity;
    character.surf_active = input.surf;

    let surf_rate = if character.surf_active { 2.6 } else { 3.4 };
    let surf_target = if character.surf_active { 1.0 } else { 0.0 };
    character.surf = exp_damp(character.surf, surf_target, surf_rate, step);

    if character.surf > 0.5 {
        surf_step(character, step, input, rig, heightfield);
    } else {
        walk_step(character, step, input, rig);
    }

    character.position.x += character.velocity.x * step;
    character.position.z += character.velocity.z * step;

    character.ground_height =
        terrain::height_at(heightfield, character.position.x, character.position.z);
    character.ground_normal =
        terrain::normal_at(heightfield, character.position.x, character.position.z);
    vertical_step(character, step, input, rig);

    character.speed = (character.velocity.x * character.velocity.x
        + character.velocity.z * character.velocity.z)
        .sqrt();
    character.speed01 = clamp01(character.speed / SURF_MAX);

    character.acceleration.x = (character.velocity.x - character.previous_velocity.x) / step;
    character.acceleration.z = (character.velocity.z - character.previous_velocity.z) / step;

    let right_x = character.facing.cos();
    let right_z = -character.facing.sin();
    let lateral = character.acceleration.x * right_x + character.acceleration.z * right_z;
    let lean_want = (lateral / 26.0).clamp(-1.0, 1.0) * (0.35 + 0.65 * character.surf);
    character.lean = exp_damp(character.lean, lean_want, 6.5, step);
    character.carve = exp_damp(character.carve, lean_want, 9.0, step);

    character.streak01 = character.surf * clamp01((character.speed - 7.0) / 11.0);

    gait(character, step);
}

/// Leaves the ground, falls back to it, and settles onto it.
///
/// Grounded, the height is a spring onto the terrain, which carries the
/// character over bumps with the camera steady. Airborne it is plain
/// ballistics. Exactly one of them runs each frame.
fn vertical_step(character: &mut Character, step: f32, input: &SnowInput, rig: &mut CameraRig) {
    if !character.airborne && input.jump {
        character.airborne = true;
        character.vertical_velocity = JUMP_SPEED;
        // From the spring's current height, so a jump taken mid-bump launches
        // from where the character actually is.
        character.position.y = character.position.y.max(character.ground_height);
    }

    if !character.airborne {
        character.position.y = exp_damp(character.position.y, character.ground_height, 26.0, step);
        character.air_height = 0.0;
        return;
    }

    character.vertical_velocity -= GRAVITY * step;
    character.position.y += character.vertical_velocity * step;

    if character.position.y <= character.ground_height && character.vertical_velocity <= 0.0 {
        character.airborne = false;
        character.landed = true;
        character.landing_impact = (-character.vertical_velocity / JUMP_SPEED).clamp(0.2, 1.6);
        character.vertical_velocity = 0.0;
        character.position.y = character.ground_height;
        character.air_height = 0.0;
        camera::add_trauma(rig, 0.10 * character.landing_impact);
        return;
    }

    character.air_height = (character.position.y - character.ground_height).max(0.0);
}

fn walk_step(character: &mut Character, step: f32, input: &SnowInput, rig: &CameraRig) {
    let max_speed = if input.sprint { RUN_SPEED } else { WALK_SPEED };

    let forward = camera::flat_forward(rig);
    let right = camera::flat_right(rig);
    let mut wish = Vec3::new(
        forward.x * input.move_z + right.x * input.move_x,
        0.0,
        forward.z * input.move_z + right.z * input.move_x,
    );

    let wish_length = (wish.x * wish.x + wish.z * wish.z).sqrt();
    if wish_length > 0.001 {
        wish.x = (wish.x / wish_length) * max_speed;
        wish.z = (wish.z / wish_length) * max_speed;

        let authority = if character.airborne { AIR_CONTROL } else { 1.0 };
        let accel = WALK_ACCEL * authority * step;
        character.velocity.x += (wish.x - character.velocity.x).clamp(-accel, accel);
        character.velocity.z += (wish.z - character.velocity.z).clamp(-accel, accel);

        let want = wish.x.atan2(wish.z);
        character.facing = angle_damp(character.facing, want, 11.0 * authority, step);
    } else {
        // Speed is held through the arc, so a jump keeps the run that launched
        // it.
        if character.airborne {
            return;
        }
        let decel = WALK_DECEL * step;
        let speed = (character.velocity.x * character.velocity.x
            + character.velocity.z * character.velocity.z)
            .sqrt();
        if speed > 0.0001 {
            let scale = (speed - decel).max(0.0) / speed;
            character.velocity.x *= scale;
            character.velocity.z *= scale;
        }
    }
}

fn surf_step(
    character: &mut Character,
    step: f32,
    input: &SnowInput,
    rig: &mut CameraRig,
    heightfield: &Heightfield,
) {
    let steer =
        (input.move_x * 0.85 + angle_delta(character.facing, rig.yaw) * 1.25).clamp(-1.0, 1.0);
    character.facing += steer * SURF_TURN * step;

    let load = steer.abs() * (character.speed / SURF_MAX);
    if load > 0.25 {
        camera::add_trauma(rig, (load - 0.25) * 1.35 * step);
    }

    let forward_x = character.facing.sin();
    let forward_z = character.facing.cos();

    let normal = terrain::normal_at(heightfield, character.position.x, character.position.z);
    let slope_assist = -(normal.x * forward_x + normal.z * forward_z) * 26.0;

    let mut thrust = SURF_THRUST + slope_assist;
    if input.move_z < 0.0 {
        thrust -= 14.0;
    }

    character.velocity.x += forward_x * thrust * step;
    character.velocity.z += forward_z * thrust * step;

    let right_x = character.facing.cos();
    let right_z = -character.facing.sin();
    let lateral = character.velocity.x * right_x + character.velocity.z * right_z;
    let grip = (SURF_GRIP * step).min(1.0);
    character.velocity.x -= right_x * lateral * grip;
    character.velocity.z -= right_z * lateral * grip;

    let speed = (character.velocity.x * character.velocity.x
        + character.velocity.z * character.velocity.z)
        .sqrt();
    if speed > 0.0001 {
        let drag = SURF_DRAG * speed * speed * 0.02 + 0.9;
        let scale = (speed - drag * step).max(0.0) / speed;
        character.velocity.x *= scale;
        character.velocity.z *= scale;
    }
    if speed > SURF_MAX {
        let scale = SURF_MAX / speed;
        character.velocity.x *= scale;
        character.velocity.z *= scale;
    }
}

/// Distance-driven gait.
fn gait(character: &mut Character, step: f32) {
    character.stepping =
        !character.airborne && character.surf <= 0.5 && character.speed <= RUN_SPEED * 1.2;
    if !character.stepping {
        character.gait_phase = 0.0;
        return;
    }

    let distance = character.speed * step;
    let stride = stride(character);
    let previous = character.gait_phase;
    character.gait_phase = (character.gait_phase + distance / stride) % 1.0;

    if character.speed < 0.15 {
        return;
    }

    let crossed =
        (previous < 0.5 && character.gait_phase >= 0.5) || character.gait_phase < previous;
    if !crossed {
        return;
    }

    character.footfall = true;
    character.foot_index = usize::from(character.gait_phase >= 0.5);

    character.foot_impact = (0.35 + character.speed / RUN_SPEED).clamp(0.0, 1.3);

    let side = if character.foot_index == 0 {
        -0.17
    } else {
        0.17
    };
    let right_x = character.facing.cos();
    let right_z = -character.facing.sin();
    character.foot_position = Vec3::new(
        character.position.x + right_x * side,
        character.position.y,
        character.position.z + right_z * side,
    );
}
