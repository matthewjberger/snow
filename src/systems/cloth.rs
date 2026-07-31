use crate::constants::{CHARACTER_TEX_HEIGHT, CHARACTER_TEX_WIDTH, CLOTH_ROW0};
use crate::settings;
use crate::settings::Settings;
use crate::systems::character::Character;
use crate::systems::figure::{
    B_CHEST, B_FORE_L, B_FORE_R, B_HAND_L, B_HAND_R, B_NECK, B_ROOT, B_SHIN_L, B_SHIN_R, B_THIGH_L,
    B_THIGH_R, B_UPPER_L, B_UPPER_R, Figure,
};
use crate::systems::terrain;
use crate::systems::terrain::Heightfield;

/// Which body capsules a panel is allowed to collide against.
const C_TORSO: u32 = 1;
const C_LEGS: u32 = 2;
const C_ARM_L: u32 = 4;
const C_ARM_R: u32 = 8;

/// Material slots the garments draw with, shared with the lofted body.
const M_ROBE: f32 = 0.0;
const M_MANTLE: f32 = 1.0;

/// Constraint relaxation iterations.
const ITERATIONS: usize = 6;

/// A garment: a closed tube of particles, `columns` around by `rows` down.
pub struct ClothPanel {
    pub columns: usize,
    pub rows: usize,
    material: f32,
    pub render_columns: usize,
    pub render_rows: usize,
    pub weave_u: f32,
    pub weave_v: f32,
    pub occlusion_top: f32,
    pub occlusion_bottom: f32,
    collide: u32,
    /// Rows at the bottom that check the snow surface.
    ground_rows: usize,
    /// Row in the shared transform texture where this panel's grid starts.
    pub node_row: usize,

    bind_position: Vec<[f32; 3]>,
    position: Vec<[f32; 3]>,
    previous: Vec<[f32; 3]>,
    target: Vec<[f32; 3]>,
    bone: Vec<usize>,
    /// Infinite for a welded particle, which the integrator and the constraint solver
    /// both read as infinite mass.
    pin_rate: Vec<f32>,

    /// Rest lengths around the ring, down the panel, and for the bending pair two rows
    /// apart.
    rest_u: Vec<f32>,
    rest_v: Vec<f32>,
    rest_bend: Vec<f32>,
}

struct PanelSpec {
    columns: usize,
    rows: usize,
    material: f32,
    render_columns: usize,
    render_rows: usize,
    weave_u: f32,
    weave_v: f32,
    occlusion_top: f32,
    occlusion_bottom: f32,
    collide: u32,
    ground_rows: usize,
}

fn panel(spec: PanelSpec) -> ClothPanel {
    let count = spec.columns * spec.rows;
    ClothPanel {
        columns: spec.columns,
        rows: spec.rows,
        material: spec.material,
        render_columns: spec.render_columns,
        render_rows: spec.render_rows,
        weave_u: spec.weave_u,
        weave_v: spec.weave_v,
        occlusion_top: spec.occlusion_top,
        occlusion_bottom: spec.occlusion_bottom,
        collide: spec.collide,
        ground_rows: spec.ground_rows,
        node_row: 0,
        bind_position: vec![[0.0; 3]; count],
        position: vec![[0.0; 3]; count],
        previous: vec![[0.0; 3]; count],
        target: vec![[0.0; 3]; count],
        bone: vec![0; count],
        pin_rate: vec![0.0; count],
        rest_u: vec![0.0; count],
        rest_v: vec![0.0; count],
        rest_bend: vec![0.0; count],
    }
}

fn count(panel: &ClothPanel) -> usize {
    panel.columns * panel.rows
}

/// The material slot and the baked occlusion at a given height down the panel.
pub fn aux(panel: &ClothPanel, v: f32) -> [f32; 2] {
    [
        panel.material,
        panel.occlusion_top + (panel.occlusion_bottom - panel.occlusion_top) * v,
    ]
}

/// Called once the bind positions are filled in.
fn finalise(panel: &mut ClothPanel) {
    for row in 0..panel.rows {
        for column in 0..panel.columns {
            let index = row * panel.columns + column;
            let a = panel.bind_position[index];
            let around = panel.bind_position[row * panel.columns + (column + 1) % panel.columns];
            panel.rest_u[index] = distance(a, around);
            if row + 1 < panel.rows {
                panel.rest_v[index] =
                    distance(a, panel.bind_position[(row + 1) * panel.columns + column]);
            }
            if row + 2 < panel.rows {
                panel.rest_bend[index] =
                    distance(a, panel.bind_position[(row + 2) * panel.columns + column]);
            }
        }
    }
    panel.position.copy_from_slice(&panel.bind_position);
    panel.previous.copy_from_slice(&panel.bind_position);
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}

/// Piecewise linear lookup over a table of control points, keyed on the first element.
fn curve(table: &[[f32; 3]], t: f32) -> [f32; 2] {
    let mut index = 0;
    while index < table.len() - 2 && t > table[index + 1][0] {
        index += 1;
    }
    let a = table[index];
    let b = table[index + 1];
    let span = if b[0] > a[0] {
        (t - a[0]) / (b[0] - a[0])
    } else {
        0.0
    };
    let k = span.clamp(0.0, 1.0);
    [a[1] + (b[1] - a[1]) * k, a[2] + (b[2] - a[2]) * k]
}

/// The robe: a long tube from the waist, flaring to a hem that is cut high at the front
/// so the boots read, and trails behind.
fn make_robe() -> ClothPanel {
    let mut panel = panel(PanelSpec {
        columns: 36,
        rows: 12,
        material: M_ROBE,
        render_columns: 72,
        render_rows: 32,
        weave_u: 1.75,
        weave_v: 1.05,
        occlusion_top: 0.55,
        occlusion_bottom: 0.42,
        collide: C_TORSO | C_LEGS,
        ground_rows: 2,
    });

    const RATE: [f32; 12] = [
        f32::INFINITY,
        30.0,
        10.0,
        4.0,
        1.6,
        0.9,
        0.55,
        0.4,
        0.35,
        0.3,
        0.3,
        0.3,
    ];

    for (row, rate) in RATE.iter().enumerate() {
        let v = row as f32 / (panel.rows - 1) as f32;
        for column in 0..panel.columns {
            let angle = (column as f32 / panel.columns as f32) * std::f32::consts::TAU;
            let sine = angle.sin();
            let cosine = angle.cos();
            let flare = v.powf(1.25);

            let fold = 0.118 * (angle * 7.0 + 0.6).sin()
                + 0.055 * (angle * 12.0 + 2.1).sin()
                + 0.026 * (angle * 19.0 + 4.4).sin();
            let pleat = 1.0 + flare * fold;

            let hem = 0.300 + 0.200 * cosine - 0.048 * (angle * 7.0 + 0.6).sin();
            let y = 0.990 + (hem - 0.990) * v;

            let radius_x = (0.158 + 0.187 * flare) * pleat;
            let radius_z = (0.128 + 0.190 * flare * (1.0 - 0.12 * cosine)) * pleat;

            let index = row * panel.columns + column;
            panel.bind_position[index] = [radius_x * sine, y, radius_z * cosine - 0.010 * v];
            panel.bone[index] = B_ROOT;
            panel.pin_rate[index] = *rate;
        }
    }
    finalise(&mut panel);
    panel
}

/// The over-mantle: a short cape that clears the shoulders and falls to the small of
/// the back.
fn make_mantle() -> ClothPanel {
    let mut panel = panel(PanelSpec {
        columns: 28,
        rows: 7,
        material: M_MANTLE,
        render_columns: 64,
        render_rows: 22,
        weave_u: 1.35,
        weave_v: 0.72,
        occlusion_top: 0.85,
        occlusion_bottom: 0.6,
        collide: C_TORSO | C_ARM_L | C_ARM_R,
        ground_rows: 0,
    });

    const RATE: [f32; 7] = [f32::INFINITY, 40.0, 12.0, 4.0, 1.5, 0.8, 0.45];
    const RADIUS: [[f32; 3]; 4] = [
        [0.00, 0.176, 0.148],
        [0.20, 0.222, 0.176],
        [0.55, 0.235, 0.196],
        [1.00, 0.246, 0.214],
    ];
    let mut height: [[f32; 3]; 4] = [
        [0.00, 1.442, 0.0],
        [0.20, 1.352, 0.0],
        [0.55, 1.220, 0.0],
        [1.00, 0.000, 0.0],
    ];

    for (row, rate) in RATE.iter().enumerate() {
        let v = row as f32 / (panel.rows - 1) as f32;
        let [radius_x, radius_z] = curve(&RADIUS, v);
        for column in 0..panel.columns {
            let angle = (column as f32 / panel.columns as f32) * std::f32::consts::TAU;
            let sine = angle.sin();
            let cosine = angle.cos();
            height[3][1] = 1.045 + 0.115 * cosine + 0.035 * (angle * 7.0 + 1.4).sin();
            let y = curve(&height, v)[0];
            let pleat =
                1.0 + v * (0.062 * (angle * 7.0 + 1.4).sin() + 0.026 * (angle * 11.0 + 3.0).sin());

            let index = row * panel.columns + column;
            panel.bind_position[index] = [
                radius_x * sine * pleat,
                y,
                radius_z * cosine * pleat - 0.012,
            ];
            panel.bone[index] = B_CHEST;
            panel.pin_rate[index] = *rate;
        }
    }
    finalise(&mut panel);
    panel
}

/// A sleeve.
fn make_sleeve(side: usize) -> ClothPanel {
    let sign = if side == 0 { -1.0 } else { 1.0 };
    let mut panel = panel(PanelSpec {
        columns: 10,
        rows: 8,
        material: M_ROBE,
        render_columns: 26,
        render_rows: 20,
        weave_u: 0.46,
        weave_v: 0.66,
        occlusion_top: 0.6,
        occlusion_bottom: 0.5,
        collide: if side == 0 { C_ARM_L } else { C_ARM_R },
        ground_rows: 0,
    });

    let upper: [f32; 3] = [sign * 0.185, 1.400, 0.000];
    let elbow = [sign * 0.230, 1.123, 0.000];
    let wrist = [sign * 0.243, 0.866, 0.016];

    let mut beyond = [
        wrist[0] - elbow[0],
        wrist[1] - elbow[1],
        wrist[2] - elbow[2],
    ];
    let length = (beyond[0] * beyond[0] + beyond[1] * beyond[1] + beyond[2] * beyond[2]).sqrt();
    beyond[0] /= length;
    beyond[1] /= length;
    beyond[2] /= length;

    const ROWS: [(usize, f32, f32); 8] = [
        (0, 0.00, 0.084),
        (0, 0.45, 0.076),
        (0, 1.00, 0.072),
        (1, 0.40, 0.068),
        (1, 0.75, 0.064),
        (1, 1.00, 0.061),
        (2, 0.045, 0.072),
        (2, 0.125, 0.098),
    ];
    let bones: [usize; 8] = if side == 0 {
        [
            B_UPPER_L, B_UPPER_L, B_UPPER_L, B_FORE_L, B_FORE_L, B_FORE_L, B_FORE_L, B_HAND_L,
        ]
    } else {
        [
            B_UPPER_R, B_UPPER_R, B_UPPER_R, B_FORE_R, B_FORE_R, B_FORE_R, B_FORE_R, B_HAND_R,
        ]
    };
    const RATE: [f32; 8] = [f32::INFINITY, 50.0, 26.0, 40.0, 18.0, 9.0, 5.0, 1.2];

    for row in 0..panel.rows {
        let (segment, t, radius) = ROWS[row];
        let centre = match segment {
            0 => [
                upper[0] + (elbow[0] - upper[0]) * t,
                upper[1] + (elbow[1] - upper[1]) * t,
                upper[2] + (elbow[2] - upper[2]) * t,
            ],
            1 => [
                elbow[0] + (wrist[0] - elbow[0]) * t,
                elbow[1] + (wrist[1] - elbow[1]) * t,
                elbow[2] + (wrist[2] - elbow[2]) * t,
            ],
            _ => [
                wrist[0] + beyond[0] * t,
                wrist[1] + beyond[1] * t,
                wrist[2] + beyond[2] * t,
            ],
        };
        for column in 0..panel.columns {
            let angle = (column as f32 / panel.columns as f32) * std::f32::consts::TAU;
            let index = row * panel.columns + column;
            panel.bind_position[index] = [
                centre[0] + angle.sin() * radius,
                centre[1],
                centre[2] + angle.cos() * radius,
            ];
            panel.bone[index] = bones[row];
            panel.pin_rate[index] = RATE[row];
        }
    }
    finalise(&mut panel);
    panel
}

/// Capsule table of two bones, a radius and the mask of panels it repels.
const CAPSULES: [(usize, usize, f32, u32); 9] = [
    (B_ROOT, B_NECK, 0.175, C_TORSO),
    (B_THIGH_L, B_SHIN_L, 0.125, C_LEGS),
    (B_SHIN_L, crate::systems::figure::B_FOOT_L, 0.098, C_LEGS),
    (B_THIGH_R, B_SHIN_R, 0.125, C_LEGS),
    (B_SHIN_R, crate::systems::figure::B_FOOT_R, 0.098, C_LEGS),
    (B_UPPER_L, B_FORE_L, 0.078, C_ARM_L),
    (B_FORE_L, B_HAND_L, 0.068, C_ARM_L),
    (B_UPPER_R, B_FORE_R, 0.078, C_ARM_R),
    (B_FORE_R, B_HAND_R, 0.068, C_ARM_R),
];

/// The garment solver: verlet integration on the coarse grids, with hard pins, distance
/// and bending constraints, capsule collision and a ground clamp.
pub struct Cloth {
    pub panels: Vec<ClothPanel>,
    wind: [f32; 3],
    time: f32,
    settled: bool,
}

impl Default for Cloth {
    fn default() -> Self {
        let mut panels = vec![make_robe(), make_mantle(), make_sleeve(0), make_sleeve(1)];

        let mut row = CLOTH_ROW0 as usize;
        for panel in &mut panels {
            assert!(
                panel.columns <= CHARACTER_TEX_WIDTH as usize,
                "garment panel is wider than the transform texture"
            );
            panel.node_row = row;
            row += panel.rows;
        }
        assert!(
            row <= CHARACTER_TEX_HEIGHT as usize,
            "transform texture is too short for the garment panels"
        );

        Self {
            panels,
            wind: [0.0; 3],
            time: 0.0,
            settled: false,
        }
    }
}

/// Flat rowBase, columns, rows per panel, for the vertex shaders.
pub fn panel_params(cloth: &Cloth) -> [[f32; 4]; 6] {
    let mut params = [[0.0_f32; 4]; 6];
    for (index, panel) in cloth.panels.iter().enumerate() {
        params[index] = [
            panel.node_row as f32,
            panel.columns as f32,
            panel.rows as f32,
            0.0,
        ];
    }
    params
}

/// Writes every panel's node positions into the transform texture, one row of the
/// grid per texture row.
pub fn write_nodes(cloth: &Cloth, texels: &mut [f32], width: usize) {
    for panel in &cloth.panels {
        for row in 0..panel.rows {
            let base = (panel.node_row + row) * width * 4;
            for column in 0..panel.columns {
                let node = panel.position[row * panel.columns + column];
                let offset = base + column * 4;
                texels[offset..offset + 4].copy_from_slice(&[node[0], node[1], node[2], 1.0]);
            }
        }
    }
}

/// Drops every garment straight onto its kinematic target.
fn settle(cloth: &mut Cloth, figure: &Figure) {
    for panel in &mut cloth.panels {
        for index in 0..count(panel) {
            let bone = &figure.skin[panel.bone[index]];
            panel.position[index] = transform(bone, panel.bind_position[index]);
        }
        panel.previous.copy_from_slice(&panel.position);
    }
    cloth.settled = true;
}

pub fn update(
    cloth: &mut Cloth,
    delta_time: f32,
    settings: &Settings,
    figure: &Figure,
    character: &Character,
    heightfield: &Heightfield,
) {
    if !cloth.settled {
        settle(cloth, figure);
    }
    if delta_time <= 0.0 {
        return;
    }

    let mut step = delta_time.min(1.0 / 30.0);
    let mut steps = 1;
    if step > 1.0 / 55.0 {
        steps = 2;
        step *= 0.5;
    }
    cloth.time += delta_time;

    let angle = settings::wind_angle(settings);
    let speed = 3.2 * settings.wind_strength;
    let gust = 1.0 + 0.35 * (cloth.time * 0.7).sin() + 0.18 * (cloth.time * 2.3 + 1.1).sin();
    cloth.wind = [
        angle.sin() * speed * gust - character.velocity.x,
        0.35 * (cloth.time * 1.9).sin(),
        angle.cos() * speed * gust - character.velocity.z,
    ];

    for _ in 0..steps {
        for index in 0..cloth.panels.len() {
            step_panel(cloth, index, step, figure, heightfield);
        }
    }
}

fn step_panel(
    cloth: &mut Cloth,
    panel_index: usize,
    step: f32,
    figure: &Figure,
    heightfield: &Heightfield,
) {
    let wind = cloth.wind;
    let time = cloth.time;
    let panel = &mut cloth.panels[panel_index];
    let count = count(panel);

    for index in 0..count {
        let bone = &figure.skin[panel.bone[index]];
        panel.target[index] = transform(bone, panel.bind_position[index]);
    }

    let magnitude = (wind[0] * wind[0] + wind[1] * wind[1] + wind[2] * wind[2]).sqrt();
    let drag = 0.085 * magnitude;
    let damping = 0.90_f32.powf(step * 60.0);
    let step_squared = step * step;

    for index in 0..count {
        if !panel.pin_rate[index].is_finite() {
            continue;
        }
        let phase = index as f32 * 1.7 + time * 4.5;
        let turbulence = [
            phase.sin() * 0.9,
            (phase * 1.31 + 2.1).sin() * 0.7,
            (phase * 0.87 + 0.4).cos() * 0.9,
        ];
        let acceleration = [
            wind[0] * drag + turbulence[0] * drag * 0.25,
            wind[1] * drag - 9.81 + turbulence[1] * drag * 0.25,
            wind[2] * drag + turbulence[2] * drag * 0.25,
        ];

        let position = panel.position[index];
        let previous = panel.previous[index];
        panel.previous[index] = position;
        for axis in 0..3 {
            let velocity = (position[axis] - previous[axis]) * damping;
            panel.position[index][axis] += velocity + acceleration[axis] * step_squared;
        }
    }

    for iteration in 0..ITERATIONS {
        anchors(panel, step);
        distance_constraints(panel, iteration);
    }
    collide(panel, figure, heightfield);
}

/// Pulls each particle toward its skinned target at its own rate.
fn anchors(panel: &mut ClothPanel, step: f32) {
    for index in 0..panel.columns * panel.rows {
        let rate = panel.pin_rate[index];
        if !rate.is_finite() {
            panel.position[index] = panel.target[index];
            continue;
        }
        if rate <= 0.0 {
            continue;
        }
        let weight = (1.0 - (-rate * step).exp()) / ITERATIONS as f32;
        for axis in 0..3 {
            panel.position[index][axis] +=
                (panel.target[index][axis] - panel.position[index][axis]) * weight;
        }
    }
}

/// Distance and bending constraints, Gauss-Seidel.
fn distance_constraints(panel: &mut ClothPanel, iteration: usize) {
    let bend = if iteration >= ITERATIONS - 3 {
        0.22
    } else {
        0.0
    };
    let (columns, rows) = (panel.columns, panel.rows);

    for row in 0..rows {
        for column in 0..columns {
            let index = row * columns + column;

            let around = row * columns + (column + 1) % columns;
            solve_link(panel, index, around, panel.rest_u[index], 1.0);

            if row + 1 < rows {
                let down = (row + 1) * columns + column;
                solve_link(panel, index, down, panel.rest_v[index], 1.0);
            }
            if bend > 0.0 && row + 2 < rows {
                let bending = (row + 2) * columns + column;
                solve_link(panel, index, bending, panel.rest_bend[index], bend);
            }
        }
    }
}

/// One distance constraint.
fn solve_link(panel: &mut ClothPanel, a: usize, b: usize, rest: f32, stiffness: f32) {
    let start = panel.position[a];
    let end = panel.position[b];
    let delta = [end[0] - start[0], end[1] - start[1], end[2] - start[2]];
    let length = (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt();
    if length < 1e-7 {
        return;
    }
    let correction = ((length - rest) / length) * stiffness;

    let a_free = panel.pin_rate[a].is_finite();
    let b_free = panel.pin_rate[b].is_finite();
    let (share_a, share_b) = match (a_free, b_free) {
        (true, true) => (correction * 0.5, correction * 0.5),
        (true, false) => (correction, 0.0),
        (false, true) => (0.0, correction),
        (false, false) => return,
    };
    for (component, offset) in panel.position[a].iter_mut().zip(delta) {
        *component += offset * share_a;
    }
    for (component, offset) in panel.position[b].iter_mut().zip(delta) {
        *component -= offset * share_b;
    }
}

/// Pushes particles out of the body capsules and off the snow.
fn collide(panel: &mut ClothPanel, figure: &Figure, heightfield: &Heightfield) {
    let count = count(panel);

    for (first, second, radius, mask) in CAPSULES {
        if panel.collide & mask == 0 {
            continue;
        }
        let start = figure.joint[first];
        let end = figure.joint[second];
        let axis = [end[0] - start[0], end[1] - start[1], end[2] - start[2]];
        let axis_length_squared =
            (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).max(1e-6);

        for index in 0..count {
            if !panel.pin_rate[index].is_finite() {
                continue;
            }
            let position = panel.position[index];
            let along = ((position[0] - start[0]) * axis[0]
                + (position[1] - start[1]) * axis[1]
                + (position[2] - start[2]) * axis[2])
                / axis_length_squared;
            let along = along.clamp(0.0, 1.0);
            let closest = [
                start[0] + axis[0] * along,
                start[1] + axis[1] * along,
                start[2] + axis[2] * along,
            ];
            let offset = [
                position[0] - closest[0],
                position[1] - closest[1],
                position[2] - closest[2],
            ];
            let separation =
                (offset[0] * offset[0] + offset[1] * offset[1] + offset[2] * offset[2]).sqrt();
            if separation >= radius || separation < 1e-6 {
                continue;
            }
            let push = (radius - separation) / separation;
            for (component, away) in panel.position[index].iter_mut().zip(offset) {
                *component += away * push;
            }
        }
    }

    if panel.ground_rows > 0 {
        let start = (panel.rows - panel.ground_rows) * panel.columns;
        for index in start..count {
            let position = panel.position[index];
            let ground = terrain::height_at(heightfield, position[0], position[2]) + 0.012;
            if position[1] < ground {
                panel.position[index][1] = ground;
            }
        }
    }
}

fn transform(matrix: &crate::rig::Matrix, point: [f32; 3]) -> [f32; 3] {
    [
        matrix[0] * point[0] + matrix[4] * point[1] + matrix[8] * point[2] + matrix[12],
        matrix[1] * point[0] + matrix[5] * point[1] + matrix[9] * point[2] + matrix[13],
        matrix[2] * point[0] + matrix[6] * point[1] + matrix[10] * point[2] + matrix[14],
    ]
}
