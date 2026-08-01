use crate::math::{clamp01, exp_damp};
use crate::rig::{
    IDENTITY, Matrix, invert_rigid, multiply, norm, set_frame_from_direction, transform_point,
};
use crate::systems::Character;
use crate::systems::Heightfield;
use crate::systems::character;
use crate::systems::terrain;

pub const B_ROOT: usize = 0;
pub const B_SPINE: usize = 1;
pub const B_CHEST: usize = 2;
pub const B_NECK: usize = 3;
pub const B_HEAD: usize = 4;
pub const B_HOOD: usize = 5;
pub const B_UPPER_L: usize = 6;
pub const B_FORE_L: usize = 7;
pub const B_HAND_L: usize = 8;
pub const B_UPPER_R: usize = 9;
pub const B_FORE_R: usize = 10;
pub const B_HAND_R: usize = 11;
pub const B_THIGH_L: usize = 12;
pub const B_SHIN_L: usize = 13;
pub const B_FOOT_L: usize = 14;
pub const B_THIGH_R: usize = 15;
pub const B_SHIN_R: usize = 16;
pub const B_FOOT_R: usize = 17;
pub const BONE_COUNT: usize = 18;

/// Bind pose: joint position, bone direction and front reference per bone.
const BIND: [[f32; 9]; BONE_COUNT] = [
    [0.0, 0.95, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
    [0.0, 1.06, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
    [0.0, 1.26, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
    [0.0, 1.46, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
    [0.0, 1.55, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
    [0.0, 1.55, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
    [-0.185, 1.400, 0.000, -0.16, -0.987, 0.0, 0.0, 0.0, 1.0],
    [-0.230, 1.123, 0.000, -0.05, -0.997, 0.06, 0.0, 0.0, 1.0],
    [-0.243, 0.866, 0.016, -0.02, -0.992, 0.12, 0.0, 0.0, 1.0],
    [0.185, 1.400, 0.000, 0.16, -0.987, 0.0, 0.0, 0.0, 1.0],
    [0.230, 1.123, 0.000, 0.05, -0.997, 0.06, 0.0, 0.0, 1.0],
    [0.243, 0.866, 0.016, 0.02, -0.992, 0.12, 0.0, 0.0, 1.0],
    [-0.100, 0.900, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 1.0],
    [-0.100, 0.460, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 1.0],
    [-0.100, 0.090, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0],
    [0.100, 0.900, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 1.0],
    [0.100, 0.460, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 1.0],
    [0.100, 0.090, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0],
];

/// Segment lengths implied by the bind table, in metres.
const THIGH_LEN: f32 = 0.44;
const SHIN_LEN: f32 = 0.37;
const UPPER_LEN: f32 = 0.28;
const FORE_LEN: f32 = 0.26;

/// How close and how far the hand is allowed to sit from the shoulder, as a
/// fraction of the arm's reach.
///
/// The solver answers for any target, but outside this band the answers are an
/// arm folded flat against itself or one locked dead straight, and the elbow
/// swings hard between them as the target crosses. Holding the hand inside the
/// band keeps a real bend in the joint whatever the aim is doing.
const ARM_NEAR: f32 = 0.55;
const ARM_FAR: f32 = 0.92;

/// Pelvis height above the feet in the bind pose.
pub const HIP_HEIGHT: f32 = 0.95;

/// How far below the bind pose a standing pelvis sits, in metres.
const STANCE_BEND: f32 = 0.07;

/// An orthonormal basis: right, up and forward.
struct Basis {
    right: [f32; 3],
    up: [f32; 3],
    forward: [f32; 3],
}

/// Composes a basis from a yaw, then a pitch about its own right axis, then a roll
/// about its own forward axis.
fn compose_basis(yaw: f32, pitch: f32, roll: f32) -> Basis {
    let (sin_yaw, cos_yaw) = yaw.sin_cos();
    let mut right = [cos_yaw, 0.0, -sin_yaw];
    let mut up = [0.0, 1.0, 0.0];
    let mut forward = [sin_yaw, 0.0, cos_yaw];

    if pitch != 0.0 {
        let (s, c) = pitch.sin_cos();
        let new_up = [
            up[0] * c + forward[0] * s,
            up[1] * c + forward[1] * s,
            up[2] * c + forward[2] * s,
        ];
        let new_forward = [
            forward[0] * c - up[0] * s,
            forward[1] * c - up[1] * s,
            forward[2] * c - up[2] * s,
        ];
        up = new_up;
        forward = new_forward;
    }
    if roll != 0.0 {
        let (s, c) = roll.sin_cos();
        let new_right = [
            right[0] * c - up[0] * s,
            right[1] * c - up[1] * s,
            right[2] * c - up[2] * s,
        ];
        let new_up = [
            up[0] * c + right[0] * s,
            up[1] * c + right[1] * s,
            up[2] * c + right[2] * s,
        ];
        right = new_right;
        up = new_up;
    }

    Basis { right, up, forward }
}

/// Two-bone inverse kinematics.
fn solve_two_bone(
    root: [f32; 3],
    target: [f32; 3],
    pole: [f32; 3],
    first: f32,
    second: f32,
) -> ([f32; 3], [f32; 3]) {
    let mut delta = [
        target[0] - root[0],
        target[1] - root[1],
        target[2] - root[2],
    ];
    let mut distance = norm(delta);
    let max_reach = (first + second) * 0.995;
    if distance < 1e-4 {
        delta = [0.0, -1.0, 0.0];
        distance = 1e-4;
    }
    if distance > max_reach {
        distance = max_reach;
    }
    let inverse = 1.0 / norm(delta).max(1e-6);
    let axis = [delta[0] * inverse, delta[1] * inverse, delta[2] * inverse];

    let along = (first * first - second * second + distance * distance) / (2.0 * distance);
    let off = (first * first - along * along).max(0.0).sqrt();

    let projection = pole[0] * axis[0] + pole[1] * axis[1] + pole[2] * axis[2];
    let mut perpendicular = [
        pole[0] - axis[0] * projection,
        pole[1] - axis[1] * projection,
        pole[2] - axis[2] * projection,
    ];
    let mut length = norm(perpendicular);
    if length < 1e-5 {
        // The hint has come to lie along the limb, so it has lost its say in
        // which way the joint breaks. Fall back to a direction square to the
        // limb, taken off whichever world axis the limb leans on least, which
        // holds the joint square to the bone wherever the target has swung to.
        let spare = if axis[1].abs() < 0.9 {
            [0.0, 1.0, 0.0]
        } else {
            [0.0, 0.0, 1.0]
        };
        let along_spare = spare[0] * axis[0] + spare[1] * axis[1] + spare[2] * axis[2];
        perpendicular = [
            spare[0] - axis[0] * along_spare,
            spare[1] - axis[1] * along_spare,
            spare[2] - axis[2] * along_spare,
        ];
        length = norm(perpendicular).max(1e-6);
    }

    (
        [
            root[0] + axis[0] * along + perpendicular[0] / length * off,
            root[1] + axis[1] * along + perpendicular[1] / length * off,
            root[2] + axis[2] * along + perpendicular[2] / length * off,
        ],
        [
            root[0] + axis[0] * distance,
            root[1] + axis[1] * distance,
            root[2] + axis[2] * distance,
        ],
    )
}

/// Pulls a target into the band the joint bends comfortably through.
///
/// Applied after every pose has blended, so the walk, the cast and the surf can
/// each aim wherever reads best and the arm still lands somewhere it can hold.
fn hold_in_reach(root: [f32; 3], target: [f32; 3], reach: f32) -> [f32; 3] {
    let delta = [
        target[0] - root[0],
        target[1] - root[1],
        target[2] - root[2],
    ];
    let distance = norm(delta);
    if distance < 1e-4 {
        return [root[0], root[1] + reach * ARM_NEAR, root[2]];
    }

    let held = distance.clamp(reach * ARM_NEAR, reach * ARM_FAR);
    let scale = held / distance;
    [
        root[0] + delta[0] * scale,
        root[1] + delta[1] * scale,
        root[2] + delta[2] * scale,
    ]
}

/// The skeleton, its bind pose, and the procedural locomotion that poses it.
pub struct Figure {
    /// World matrix per bone.
    pub world: Vec<Matrix>,
    inverse_bind: Vec<Matrix>,
    /// World times inverse bind, the matrix geometry is actually skinned by.
    pub skin: Vec<Matrix>,
    /// World joint positions, which the cloth collision reads.
    pub joint: Vec<[f32; 3]>,

    /// Where each foot is planted.
    pub plant: [[f32; 3]; 2],
    /// Live foot position, equal to the plant during stance.
    foot_position: [[f32; 3]; 2],
    /// One while the foot carries weight, zero mid-swing.
    foot_weight: [f32; 2],
    was_stance: [bool; 2],
    /// Set for one frame when a foot touches down, which drives the spray and the
    /// footprint.
    pub touchdown: [bool; 2],

    hip_height: f32,
    pitch: f32,
    roll: f32,
    bob: f32,
    head_yaw: f32,
    head_pitch: f32,
    hood_yaw: f32,
    hood_pitch: f32,
    /// How far the figure has settled into the snow, in metres.
    sink: f32,
    time: f32,
}

impl Default for Figure {
    fn default() -> Self {
        let mut bind = vec![IDENTITY; BONE_COUNT];
        let mut inverse_bind = vec![IDENTITY; BONE_COUNT];
        for (index, entry) in BIND.iter().enumerate() {
            set_frame_from_direction(
                &mut bind[index],
                [entry[0], entry[1], entry[2]],
                [entry[3], entry[4], entry[5]],
                [entry[6], entry[7], entry[8]],
            );
            inverse_bind[index] = invert_rigid(&bind[index]);
        }

        Self {
            world: bind,
            inverse_bind,
            skin: vec![IDENTITY; BONE_COUNT],
            joint: vec![[0.0; 3]; BONE_COUNT],
            plant: [[0.0; 3]; 2],
            foot_position: [[0.0; 3]; 2],
            foot_weight: [1.0; 2],
            was_stance: [true; 2],
            touchdown: [false; 2],
            hip_height: HIP_HEIGHT - STANCE_BEND,
            pitch: 0.0,
            roll: 0.0,
            bob: 0.0,
            head_yaw: 0.0,
            head_pitch: 0.0,
            hood_yaw: 0.0,
            hood_pitch: 0.0,
            sink: 0.04,
            time: 0.0,
        }
    }
}

/// Where the feet go with no ground under them.
///
/// Hung off the hips, gathered on the way up and reaching down again on the way
/// back, which is what a body does when it expects to land. It also keeps the
/// legs inside their reach: feet held at snow height while the hips climb would
/// pull them straight.
fn tuck_feet(
    figure: &mut Figure,
    step: f32,
    character: &Character,
    forward: [f32; 2],
    right: [f32; 2],
    vertical: f32,
) {
    // One on the way up, zero once the fall has built, so the gather happens at
    // the top of the arc and the legs are already extending before touchdown.
    let rising = clamp01(character.vertical_velocity / 3.0);
    let tuck = 0.30 * rising;
    let drop = vertical - tuck;

    for foot in 0..2 {
        let side = if foot == 0 { -0.105 } else { 0.105 };
        // Trailing slightly, split fore and aft so each leg reads on its own.
        let along = if foot == 0 { 0.06 } else { -0.04 } - 0.10 * rising;

        let want = [
            character.position.x + right[0] * side + forward[0] * along,
            character.position.y + figure.hip_height + figure.bob - figure.sink - drop,
            character.position.z + right[1] * side + forward[1] * along,
        ];

        for (axis, target) in want.iter().enumerate() {
            figure.foot_position[foot][axis] =
                exp_damp(figure.foot_position[foot][axis], *target, 14.0, step);
            figure.plant[foot][axis] = figure.foot_position[foot][axis];
        }
        figure.touchdown[foot] = false;
        figure.foot_weight[foot] = exp_damp(figure.foot_weight[foot], 0.0, 18.0, step);
        // Leaves the frame after touchdown to count as a fresh plant and stamp.
        figure.was_stance[foot] = false;
    }
}

/// Poses the skeleton for this frame.
pub fn update(
    figure: &mut Figure,
    delta_time: f32,
    character: &Character,
    heightfield: &Heightfield,
) {
    let step = delta_time.min(1.0 / 30.0);
    figure.time += step;

    let surf = character.surf;
    let run = (character.speed / 5.4).min(1.0);

    update_feet(figure, step, character, heightfield);

    let forward_acceleration = character.acceleration.x * character.facing.sin()
        + character.acceleration.z * character.facing.cos();
    let pitch_want = 0.10 * run
        + 0.012 * forward_acceleration.clamp(-9.0, 22.0)
        + surf * (0.30 + 0.16 * character.speed01);
    figure.pitch = exp_damp(figure.pitch, pitch_want, 7.0, step);

    let roll_want = character.lean * (0.16 + 0.34 * surf);
    figure.roll = exp_damp(figure.roll, roll_want, 8.0, step);

    let bob_want = (1.0 - surf)
        * (-0.028 * run * (0.5 - 0.5 * (4.0 * std::f32::consts::PI * character.gait_phase).cos()));
    figure.bob = exp_damp(figure.bob, bob_want, 18.0, step);

    let crouch = STANCE_BEND + 0.035 * run + surf * (0.13 + 0.05 * character.speed01);
    figure.hip_height = exp_damp(figure.hip_height, HIP_HEIGHT - crouch, 9.0, step);

    figure.sink = exp_damp(figure.sink, 0.045 + surf * 0.055, 4.0, step);

    let ground_x = character.position.x;
    let ground_z = character.position.z;
    // Taken from the terrain, which stays steady over a bump, with the jump
    // added on top as a lift. Grounded, the lift is zero and the pose is exactly
    // as it always was.
    let ground_y = terrain::height_at(heightfield, ground_x, ground_z) + character.air_height;
    let root_y = ground_y - figure.sink + figure.hip_height + figure.bob;

    let body = compose_basis(character.facing, figure.pitch, figure.roll);

    let twist =
        (1.0 - surf) * 0.13 * run * (2.0 * std::f32::consts::PI * character.gait_phase).sin();
    let pelvis = compose_basis(character.facing + twist, figure.pitch, figure.roll);
    set_bone(
        figure,
        B_ROOT,
        [ground_x, root_y, ground_z],
        pelvis.up,
        pelvis.forward,
    );

    let spine = [
        ground_x + body.up[0] * 0.11,
        root_y + body.up[1] * 0.11,
        ground_z + body.up[2] * 0.11,
    ];
    set_bone(figure, B_SPINE, spine, body.up, body.forward);

    let chest_twist = -twist * 1.5;
    let chest_pitch = figure.pitch + 0.05 * run + surf * 0.10;
    let chest_basis = compose_basis(
        character.facing + chest_twist,
        chest_pitch,
        figure.roll * 1.15,
    );

    let chest = [
        ground_x + body.up[0] * 0.31,
        root_y + body.up[1] * 0.31,
        ground_z + body.up[2] * 0.31,
    ];
    set_bone(figure, B_CHEST, chest, chest_basis.up, chest_basis.forward);

    let neck = [
        chest[0] + chest_basis.up[0] * 0.20,
        chest[1] + chest_basis.up[1] * 0.20,
        chest[2] + chest_basis.up[2] * 0.20,
    ];
    set_bone(figure, B_NECK, neck, chest_basis.up, chest_basis.forward);

    figure.head_pitch = exp_damp(
        figure.head_pitch,
        -chest_pitch * 0.62 + surf * 0.10,
        9.0,
        step,
    );
    figure.head_yaw = exp_damp(figure.head_yaw, character.lean * -0.22, 6.0, step);
    let head_basis = compose_basis(
        character.facing + chest_twist + figure.head_yaw,
        chest_pitch + figure.head_pitch,
        figure.roll * 0.5,
    );
    let head = [
        neck[0] + chest_basis.up[0] * 0.09,
        neck[1] + chest_basis.up[1] * 0.09,
        neck[2] + chest_basis.up[2] * 0.09,
    ];
    set_bone(figure, B_HEAD, head, head_basis.up, head_basis.forward);

    figure.hood_yaw = exp_damp(
        figure.hood_yaw,
        character.facing + chest_twist + figure.head_yaw,
        11.0,
        step,
    );
    figure.hood_pitch = exp_damp(
        figure.hood_pitch,
        chest_pitch + figure.head_pitch + 0.05,
        9.0,
        step,
    );
    let hood_basis = compose_basis(figure.hood_yaw, figure.hood_pitch, figure.roll * 0.5);
    set_bone(figure, B_HOOD, head, hood_basis.up, hood_basis.forward);

    pose_arms(figure, step, character, chest, &chest_basis);
    pose_leg(figure, 0, [ground_x, root_y, ground_z], &body);
    pose_leg(figure, 1, [ground_x, root_y, ground_z], &body);

    for bone in 0..BONE_COUNT {
        figure.skin[bone] = multiply(&figure.world[bone], &figure.inverse_bind[bone]);
        figure.joint[bone] = [
            figure.world[bone][12],
            figure.world[bone][13],
            figure.world[bone][14],
        ];
    }
}

fn set_bone(figure: &mut Figure, bone: usize, position: [f32; 3], axis: [f32; 3], front: [f32; 3]) {
    set_frame_from_direction(&mut figure.world[bone], position, axis, front);
}

/// Advances the stance and swing state machine and places both ankles.
fn update_feet(figure: &mut Figure, step: f32, character: &Character, heightfield: &Heightfield) {
    let surf = character.surf;
    let run = (character.speed / 5.4).min(1.0);
    let duty = 0.66 - 0.20 * run;

    let forward = [character.facing.sin(), character.facing.cos()];
    let right = [character.facing.cos(), -character.facing.sin()];

    let stance_travel = duty * character::stride(character);
    let vertical = figure.hip_height + figure.bob - 0.14 - 0.3 * figure.sink;
    let reach = (THIGH_LEN + SHIN_LEN) * 0.97;
    let horizontal = (reach * reach - vertical * vertical).max(0.0).sqrt();
    let half = (stance_travel * 0.5).min(horizontal);
    let moving = character.speed > 0.2 && character.stepping;

    if character.airborne {
        tuck_feet(figure, step, character, forward, right, vertical);
        return;
    }

    for foot in 0..2 {
        let side = if foot == 0 { -0.105 } else { 0.105 };
        let phase = (character.gait_phase + if foot == 0 { 0.0 } else { 0.5 }) % 1.0;
        let stance = !moving || phase < duty;

        let next_x = character.position.x + forward[0] * half + right[0] * side;
        let next_z = character.position.z + forward[1] * half + right[1] * side;

        if stance {
            if !figure.was_stance[foot] {
                figure.plant[foot] = [
                    next_x,
                    terrain::height_at(heightfield, next_x, next_z) - figure.sink * 0.7,
                    next_z,
                ];
                figure.touchdown[foot] = true;
            } else {
                figure.touchdown[foot] = false;
            }
            if !moving {
                let settle_x = character.position.x + right[0] * side + forward[0] * 0.02;
                let settle_z = character.position.z + right[1] * side + forward[1] * 0.02;
                figure.plant[foot][0] = exp_damp(figure.plant[foot][0], settle_x, 7.0, step);
                figure.plant[foot][2] = exp_damp(figure.plant[foot][2], settle_z, 7.0, step);
                let ground =
                    terrain::height_at(heightfield, figure.plant[foot][0], figure.plant[foot][2]);
                figure.plant[foot][1] =
                    exp_damp(figure.plant[foot][1], ground - figure.sink * 0.7, 7.0, step);
            }
            figure.foot_position[foot] = figure.plant[foot];
            figure.foot_weight[foot] = exp_damp(figure.foot_weight[foot], 1.0, 22.0, step);
        } else {
            figure.touchdown[foot] = false;
            let along = (phase - duty) / (1.0 - duty);
            let eased = along * along * (3.0 - 2.0 * along);
            let next_y = terrain::height_at(heightfield, next_x, next_z) - figure.sink * 0.7;
            let from = figure.plant[foot];
            figure.foot_position[foot] = [
                from[0] + (next_x - from[0]) * eased,
                from[1]
                    + (next_y - from[1]) * eased
                    + (std::f32::consts::PI * along).sin() * (0.055 + 0.12 * run),
                from[2] + (next_z - from[2]) * eased,
            ];
            figure.foot_weight[foot] = exp_damp(figure.foot_weight[foot], 0.0, 22.0, step);
        }

        figure.was_stance[foot] = stance;
    }

    if surf > 0.001 {
        for foot in 0..2 {
            let lateral = if foot == 0 { -0.17 } else { 0.17 };
            let along = if foot == 0 { 0.11 } else { -0.11 };
            let stance_x = character.position.x + forward[0] * along + right[0] * lateral;
            let stance_z = character.position.z + forward[1] * along + right[1] * lateral;
            let stance_y = terrain::height_at(heightfield, stance_x, stance_z) - figure.sink;
            let current = figure.foot_position[foot];
            figure.foot_position[foot] = [
                current[0] + (stance_x - current[0]) * surf,
                current[1] + (stance_y - current[1]) * surf,
                current[2] + (stance_z - current[2]) * surf,
            ];
            figure.foot_weight[foot] = figure.foot_weight[foot].max(surf);
        }
    }
}

/// Solves one leg.
fn pose_leg(figure: &mut Figure, foot: usize, root: [f32; 3], body: &Basis) {
    let side = if foot == 0 { -0.10 } else { 0.10 };
    let (thigh_bone, shin_bone, foot_bone) = if foot == 0 {
        (B_THIGH_L, B_SHIN_L, B_FOOT_L)
    } else {
        (B_THIGH_R, B_SHIN_R, B_FOOT_R)
    };

    let hip = [
        root[0] + body.right[0] * side - body.up[0] * 0.05,
        root[1] + body.right[1] * side - body.up[1] * 0.05,
        root[2] + body.right[2] * side - body.up[2] * 0.05,
    ];

    let want = [
        figure.foot_position[foot][0],
        figure.foot_position[foot][1] + 0.09,
        figure.foot_position[foot][2],
    ];

    let outward = if foot == 0 { -0.22 } else { 0.22 };
    let pole = [
        body.forward[0] + body.right[0] * outward,
        body.forward[1] + body.right[1] * outward,
        body.forward[2] + body.right[2] * outward,
    ];
    let (knee, ankle) = solve_two_bone(hip, want, pole, THIGH_LEN, SHIN_LEN);

    set_bone(
        figure,
        thigh_bone,
        hip,
        [knee[0] - hip[0], knee[1] - hip[1], knee[2] - hip[2]],
        body.forward,
    );
    set_bone(
        figure,
        shin_bone,
        knee,
        [ankle[0] - knee[0], ankle[1] - knee[1], ankle[2] - knee[2]],
        body.forward,
    );

    let toe_down = (1.0 - figure.foot_weight[foot]) * 0.55;
    let (sin_toe, cos_toe) = toe_down.sin_cos();
    let sole = [
        body.forward[0] * cos_toe - body.up[0] * sin_toe,
        body.forward[1] * cos_toe - body.up[1] * sin_toe,
        body.forward[2] * cos_toe - body.up[2] * sin_toe,
    ];
    set_bone(figure, foot_bone, ankle, sole, body.up);
}

/// Arms: counter-swing against the legs while walking, and a wide, low bending
/// stance while surfing, which is both the reference pose and what a person does at
/// twenty metres a second.
fn pose_arms(
    figure: &mut Figure,
    step: f32,
    character: &Character,
    chest: [f32; 3],
    basis: &Basis,
) {
    let _ = step;
    let surf = character.surf;
    let run = (character.speed / 5.4).min(1.0);
    let swing = (2.0 * std::f32::consts::PI * character.gait_phase).sin()
        * (0.20 + 0.42 * run)
        * (1.0 - surf);
    let idle = (figure.time * 0.9).sin() * 0.02 + (figure.time * 1.7 + 1.3).sin() * 0.012;

    for arm in 0..2 {
        let sign = if arm == 0 { -1.0 } else { 1.0 };
        let (upper_bone, fore_bone, hand_bone) = if arm == 0 {
            (B_UPPER_L, B_FORE_L, B_HAND_L)
        } else {
            (B_UPPER_R, B_FORE_R, B_HAND_R)
        };

        let shoulder = [
            chest[0] + basis.right[0] * (sign * 0.185) + basis.up[0] * 0.14,
            chest[1] + basis.right[1] * (sign * 0.185) + basis.up[1] * 0.14,
            chest[2] + basis.right[2] * (sign * 0.185) + basis.up[2] * 0.14,
        ];

        let sweep = swing * -sign;
        let mut target = [
            shoulder[0] + basis.forward[0] * (sweep * 0.38) - basis.up[0] * 0.43
                + basis.right[0] * (sign * 0.11),
            shoulder[1] + basis.forward[1] * (sweep * 0.38) - basis.up[1] * 0.43
                + basis.right[1] * (sign * 0.11)
                + idle * sign,
            shoulder[2] + basis.forward[2] * (sweep * 0.38) - basis.up[2] * 0.43
                + basis.right[2] * (sign * 0.11),
        ];

        if character.cast > 0.001 {
            let aim = character.cast_aim;
            // The leading hand reaches along the aim and the trailing one is
            // drawn back across the body. Both sit inside the reach band, so
            // the leading arm keeps a slight bend at full extension and the
            // trailing one holds a clear one.
            let leading = arm == 1;
            let outward = if leading { 0.05 } else { -0.10 };
            let along = if leading { 0.32 } else { 0.22 };
            let lift = if leading { 0.11 } else { -0.05 };
            let cast_target = [
                shoulder[0]
                    + basis.right[0] * (sign * 0.30 + outward * sign)
                    + aim.x * along
                    + basis.up[0] * lift,
                shoulder[1]
                    + basis.right[1] * (sign * 0.30)
                    + aim.y * along
                    + basis.up[1] * lift
                    + lift * 0.6,
                shoulder[2]
                    + basis.right[2] * (sign * 0.30 + outward * sign)
                    + aim.z * along
                    + basis.up[2] * lift,
            ];
            for axis in 0..3 {
                target[axis] += (cast_target[axis] - target[axis]) * character.cast;
            }
        }

        if surf > 0.001 {
            let rise = 0.02 + character.carve * sign * 0.22;
            let surf_target = [
                shoulder[0]
                    + basis.right[0] * (sign * 0.33)
                    + basis.forward[0] * 0.24
                    + basis.up[0] * rise,
                shoulder[1]
                    + basis.right[1] * (sign * 0.33)
                    + basis.forward[1] * 0.24
                    + basis.up[1] * rise,
                shoulder[2]
                    + basis.right[2] * (sign * 0.33)
                    + basis.forward[2] * 0.24
                    + basis.up[2] * rise,
            ];
            for axis in 0..3 {
                target[axis] += (surf_target[axis] - target[axis]) * surf;
            }
        }

        // Where the elbow wants to sit: behind and outside the shoulder while the
        // arms swing, swinging wider and lower as the casting stance comes in.
        // That is where an elbow goes when the hands come up and forward, and it
        // keeps the hint clear of the arm itself, which is what the hands
        // reaching along the aim would otherwise close on.
        let spread = 0.55 + 0.30 * character.cast;
        let behind = 1.0 - 0.45 * character.cast;
        let drop = 0.35 + 0.30 * character.cast;
        let pole = [
            -basis.forward[0] * behind + basis.right[0] * (sign * spread),
            -basis.forward[1] * behind + basis.right[1] * (sign * spread) - drop,
            -basis.forward[2] * behind + basis.right[2] * (sign * spread),
        ];
        let target = hold_in_reach(shoulder, target, UPPER_LEN + FORE_LEN);
        let (elbow, wrist) = solve_two_bone(shoulder, target, pole, UPPER_LEN, FORE_LEN);

        set_bone(
            figure,
            upper_bone,
            shoulder,
            [
                elbow[0] - shoulder[0],
                elbow[1] - shoulder[1],
                elbow[2] - shoulder[2],
            ],
            basis.forward,
        );
        set_bone(
            figure,
            fore_bone,
            elbow,
            [
                wrist[0] - elbow[0],
                wrist[1] - elbow[1],
                wrist[2] - elbow[2],
            ],
            basis.forward,
        );
        let mut hand_axis = [
            wrist[0] - elbow[0],
            wrist[1] - elbow[1],
            wrist[2] - elbow[2],
        ];
        let length = norm(hand_axis).max(1e-6);
        hand_axis = [
            hand_axis[0] / length,
            hand_axis[1] / length,
            hand_axis[2] / length,
        ];
        set_bone(figure, hand_bone, target, hand_axis, basis.forward);
    }
}

/// World position of a hand, for the spell emitters.
pub fn hand_position(figure: &Figure, which: usize) -> [f32; 3] {
    let bone = if which == 0 { B_HAND_L } else { B_HAND_R };
    transform_point(&figure.world[bone], [0.0, 0.09, 0.0])
}
