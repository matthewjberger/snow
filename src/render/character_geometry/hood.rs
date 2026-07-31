//! The cowl: a lofted shell over the head, and the ring the fur trim sits on.

use crate::render::character_geometry::{
    Builder, HEAD_CENTRE, HOOD_COLS, HOOD_ROWS, M_ROBE, quad, vertex,
};
use crate::systems::figure::B_HOOD;

pub fn face_direction() -> [f32; 3] {
    let v = [0.0, -0.28, 0.96];
    let length = crate::rig::norm(v);
    [v[0] / length, v[1] / length, v[2] / length]
}

/// Face-opening rim point.
pub fn hood_rim_point(s: f32) -> [f32; 3] {
    let face = face_direction();
    let angle = s * std::f32::consts::TAU;
    let (sin_angle, cos_angle) = angle.sin_cos();
    let u = [1.0, 0.0, 0.0];
    let w = crate::rig::cross(face, u);
    let centre = [
        HEAD_CENTRE[0] + face[0] * 0.105,
        HEAD_CENTRE[1] + face[1] * 0.105,
        HEAD_CENTRE[2] + face[2] * 0.105,
    ];
    [
        centre[0] + u[0] * 0.152 * sin_angle + w[0] * 0.163 * cos_angle,
        centre[1] + u[1] * 0.152 * sin_angle + w[1] * 0.163 * cos_angle,
        centre[2] + u[2] * 0.152 * sin_angle + w[2] * 0.163 * cos_angle,
    ]
}

fn hood_base_point(s: f32) -> [f32; 3] {
    let angle = s * std::f32::consts::TAU;
    [0.212 * angle.sin(), 1.352, -0.012 - 0.182 * angle.cos()]
}

/// The cowl, as a swept quadratic: each strand runs from a point on the face-opening
/// rim to a point where the hood meets the shoulders, bowed outward by a control point
/// pushed furthest over the crown.
pub fn build_hood(builder: &mut Builder) {
    let mut previous_row: Option<Vec<u32>> = None;

    for row_index in 0..=HOOD_ROWS {
        let t = row_index as f32 / HOOD_ROWS as f32;
        let mut row = Vec::with_capacity(HOOD_COLS);
        for column in 0..HOOD_COLS {
            let s = column as f32 / HOOD_COLS as f32;
            let rim = hood_rim_point(s);
            let base = hood_base_point(s);

            let angle = s * std::f32::consts::TAU;
            let (sin_angle, cos_angle) = angle.sin_cos();
            let mut control = [sin_angle, cos_angle * 0.84, cos_angle * -0.54];
            let length = crate::rig::norm(control).max(1e-6);
            control = [
                control[0] / length,
                control[1] / length,
                control[2] / length,
            ];
            let radius = 0.205 + 0.062 * cos_angle;
            let middle = [
                HEAD_CENTRE[0] + control[0] * radius,
                HEAD_CENTRE[1] + control[1] * radius,
                HEAD_CENTRE[2] + control[2] * radius,
            ];

            let inverse = 1.0 - t;
            let position = [
                inverse * inverse * rim[0] + 2.0 * inverse * t * middle[0] + t * t * base[0],
                inverse * inverse * rim[1] + 2.0 * inverse * t * middle[1] + t * t * base[1],
                inverse * inverse * rim[2] + 2.0 * inverse * t * middle[2] + t * t * base[2],
            ];

            let occlusion = 0.34 + 0.55 * (t * 2.2).min(1.0);
            row.push(vertex(
                builder,
                position,
                [s * 1.02, t * 0.45],
                M_ROBE,
                occlusion,
                [B_HOOD as f32, 1.0, 0.0, 0.0],
            ));
        }
        if let Some(previous) = &previous_row {
            for column in 0..HOOD_COLS {
                let next = (column + 1) % HOOD_COLS;
                quad(
                    builder,
                    previous[column],
                    previous[next],
                    row[next],
                    row[column],
                );
            }
        }
        previous_row = Some(row);
    }
}
