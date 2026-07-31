use crate::input::SnowInput;
use crate::math::{exp_damp, noise1, spring_damp};
use nalgebra_glm::{Mat4, Vec3};

/// Height probes taken along the spring arm each frame.
const ARM_SAMPLES: usize = 5;

const PITCH_MIN: f32 = -0.62;
const PITCH_MAX: f32 = 1.05;
const DIST_MIN: f32 = 2.6;
const DIST_MAX: f32 = 11.0;

pub const Z_NEAR: f32 = 0.12;
pub const Z_FAR: f32 = 4200.0;
const BASE_FOV: f32 = 1.02;

/// What the rig chases: the character's motion, and how hard it is turning.
#[derive(Clone, Copy, Default)]
pub struct RigTarget {
    pub position: Vec3,
    pub velocity: Vec3,
    /// Signed lean, negative to the left, which the rig banks into.
    pub lean: f32,
    /// Speed normalised against the surf maximum, which widens the field of view and
    /// leads the pivot into the direction of travel.
    pub speed01: f32,
}

/// Third-person spring-arm rig.
pub struct CameraRig {
    pub yaw: f32,
    pub pitch: f32,

    pub distance: f32,
    pub distance_target: f32,

    pub pivot: Vec3,
    pivot_velocity: Vec3,

    /// Over-the-shoulder offset, in camera space.
    shoulder: f32,
    pivot_height: f32,

    pub fov: f32,
    pub roll: f32,
    roll_target: f32,

    /// The rig's basis, republished every frame.
    pub forward: Vec3,
    pub right: Vec3,
    pub up: Vec3,

    pub position: Vec3,

    /// Trauma-based shake: shake is trauma squared, so it falls off perceptually rather
    /// than linearly.
    trauma: f32,
    shake_time: f32,

    /// Metres of snow the camera must keep beneath it.
    ground_clearance: f32,
    ground_lift: f32,

    first: bool,
}

impl Default for CameraRig {
    fn default() -> Self {
        Self {
            yaw: 2.4,
            pitch: 0.17,
            distance: 6.2,
            distance_target: 6.2,
            pivot: Vec3::zeros(),
            pivot_velocity: Vec3::zeros(),
            shoulder: 0.85,
            pivot_height: 1.62,
            fov: BASE_FOV,
            roll: 0.0,
            roll_target: 0.0,
            forward: Vec3::new(0.0, 0.0, 1.0),
            right: Vec3::new(1.0, 0.0, 0.0),
            up: Vec3::new(0.0, 1.0, 0.0),
            position: Vec3::new(0.0, 3.0, -6.0),
            trauma: 0.0,
            shake_time: 0.0,
            ground_clearance: 1.35,
            ground_lift: 0.0,
            first: true,
        }
    }
}

pub fn add_trauma(rig: &mut CameraRig, amount: f32) {
    rig.trauma = (rig.trauma + amount).min(1.0);
}

/// Flat camera-space forward on the XZ plane, for movement.
pub fn flat_forward(rig: &CameraRig) -> Vec3 {
    Vec3::new(rig.yaw.sin(), 0.0, rig.yaw.cos())
}

pub fn flat_right(rig: &CameraRig) -> Vec3 {
    Vec3::new(rig.yaw.cos(), 0.0, -rig.yaw.sin())
}

pub fn update(
    rig: &mut CameraRig,
    delta_time: f32,
    input: &SnowInput,
    target: &RigTarget,
    height_at: impl Fn(f32, f32) -> f32,
) {
    let RigTarget {
        position: target_position,
        velocity: target_velocity,
        lean,
        speed01,
    } = *target;
    rig.yaw += input.look_x;
    rig.pitch = (rig.pitch + input.look_y).clamp(PITCH_MIN, PITCH_MAX);

    rig.distance_target = (rig.distance_target + input.zoom_delta * (rig.distance_target * 0.35))
        .clamp(DIST_MIN, DIST_MAX);
    rig.distance = exp_damp(rig.distance, rig.distance_target, 9.0, delta_time);

    let mut pivot = target_position;
    pivot.y += rig.pivot_height;

    let lead = speed01.min(1.0) * 1.35;
    pivot.x += target_velocity.x * lead * 0.09;
    pivot.z += target_velocity.z * lead * 0.09;

    if rig.first {
        rig.pivot = pivot;
        rig.first = false;
    } else {
        spring_damp(
            &mut rig.pivot,
            &mut rig.pivot_velocity,
            &pivot,
            7.5,
            1.0,
            delta_time,
        );
    }

    let fov_want = BASE_FOV * (1.0 + speed01 * 0.19);
    rig.fov = exp_damp(rig.fov, fov_want, 3.2, delta_time);

    rig.roll_target = -lean * 0.085;
    rig.roll = exp_damp(rig.roll, rig.roll_target, 5.0, delta_time);

    rig.trauma = (rig.trauma - delta_time * 1.15).max(0.0);
    rig.shake_time += delta_time;
    let shake = rig.trauma * rig.trauma;

    let cos_pitch = rig.pitch.cos();
    rig.forward = Vec3::new(
        rig.yaw.sin() * cos_pitch,
        -rig.pitch.sin(),
        rig.yaw.cos() * cos_pitch,
    );
    rig.right = Vec3::new(rig.yaw.cos(), 0.0, -rig.yaw.sin());
    rig.up = rig.forward.cross(&rig.right).normalize();

    let mut desired =
        rig.pivot - rig.forward * rig.distance + rig.right * rig.shoulder - rig.up * 0.22;

    let mut need: f32 = 0.0;
    for sample in 0..=ARM_SAMPLES {
        let along = sample as f32 / ARM_SAMPLES as f32;
        let x = rig.pivot.x + (desired.x - rig.pivot.x) * along;
        let z = rig.pivot.z + (desired.z - rig.pivot.z) * along;
        let y = rig.pivot.y + (desired.y - rig.pivot.y) * along;
        let ground = height_at(x, z) + rig.ground_clearance * (0.35 + 0.65 * along);
        need = need.max(ground - y);
    }
    let lift_rate = if need > rig.ground_lift { 26.0 } else { 4.5 };
    rig.ground_lift = exp_damp(rig.ground_lift, need, lift_rate, delta_time);
    desired.y += rig.ground_lift;

    if shake > 0.0001 {
        let time = rig.shake_time * 26.0;
        desired.x += (noise1(time) * 2.0 - 1.0) * shake * 0.16;
        desired.y += (noise1(time + 31.7) * 2.0 - 1.0) * shake * 0.16;
        desired.z += (noise1(time + 71.3) * 2.0 - 1.0) * shake * 0.10;
    }

    rig.position = desired;

    if shake > 0.0001 {
        let pitch_shake = (noise1(rig.shake_time * 31.0 + 11.0) * 2.0 - 1.0) * shake * 0.02;
        let yaw_shake = (noise1(rig.shake_time * 29.0 + 53.0) * 2.0 - 1.0) * shake * 0.02;
        let roll_shake = (noise1(rig.shake_time * 23.0 + 97.0) * 2.0 - 1.0) * shake * 0.05;
        let shaken_pitch = rig.pitch + pitch_shake;
        let shaken_yaw = rig.yaw + yaw_shake;
        let cos_shaken = shaken_pitch.cos();
        rig.forward = Vec3::new(
            shaken_yaw.sin() * cos_shaken,
            -shaken_pitch.sin(),
            shaken_yaw.cos() * cos_shaken,
        );
        rig.right = Vec3::new(shaken_yaw.cos(), 0.0, -shaken_yaw.sin());
        rig.up = rig.forward.cross(&rig.right).normalize();
        rig.roll += roll_shake;
    }
}

/// World-to-view, left handed, matching the basis the rig publishes.
pub fn view_matrix(rig: &CameraRig) -> Mat4 {
    let rolled_up = rig.up * rig.roll.cos() - rig.right * rig.roll.sin();
    nalgebra_glm::look_at_lh(&rig.position, &(rig.position + rig.forward), &rolled_up)
}

/// View-to-clip, left handed with a zero-to-one depth range, which is what WebGPU
/// clips against.
pub fn projection_matrix(rig: &CameraRig, aspect: f32) -> Mat4 {
    nalgebra_glm::perspective_lh_zo(aspect, rig.fov, Z_NEAR, Z_FAR)
}
