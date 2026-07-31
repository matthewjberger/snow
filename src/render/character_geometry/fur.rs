//! Shell fur: concentric offset shells along the hood rim and the cuff bands.

use crate::render::character_geometry::hood::{face_direction, hood_rim_point};
use crate::render::character_geometry::{
    Builder, CUFF_SHELLS, FUR_ARC, FUR_ARC_STEPS, HEAD_CENTRE, HOOD_SHELLS, builder, finish, quad,
    set_normal, vertex,
};
use crate::render::geometry::StaticMesh;
use crate::systems::figure::{B_FORE_L, B_FORE_R, B_HOOD};
use nightshade::prelude::wgpu;

/// Shell fur.
pub fn build_fur(device: &wgpu::Device) -> StaticMesh {
    let mut builder = builder();
    builder.explicit_normals = true;
    let face = face_direction();

    let columns = 26;
    let mut bases = Vec::with_capacity(columns);
    let mut outs = Vec::with_capacity(columns);
    for column in 0..columns {
        let point = hood_rim_point(column as f32 / columns as f32);
        bases.push(point);
        let mut away = [
            point[0] - HEAD_CENTRE[0],
            point[1] - HEAD_CENTRE[1],
            point[2] - HEAD_CENTRE[2],
        ];
        let length = crate::rig::norm(away).max(1e-6);
        away = [
            away[0] / length + face[0] * 0.45,
            away[1] / length + face[1] * 0.45,
            away[2] / length + face[2] * 0.45,
        ];
        let length = crate::rig::norm(away).max(1e-6);
        outs.push([away[0] / length, away[1] / length, away[2] / length]);
    }
    emit_fur_band(
        &mut builder,
        &bases,
        &outs,
        &FurBand {
            core_radius: 0.024,
            strand_length: 0.048,
            shells: HOOD_SHELLS,
            bone: B_HOOD,
            occlusion: 0.62,
        },
    );

    for arm in 0..2 {
        let side = if arm == 0 { -1.0 } else { 1.0 };
        let bone = if arm == 0 { B_FORE_L } else { B_FORE_R };
        let columns = 12;
        let mut bases = Vec::with_capacity(columns);
        let mut outs = Vec::with_capacity(columns);
        for column in 0..columns {
            let angle = (column as f32 / columns as f32) * std::f32::consts::TAU;
            let (sin_angle, cos_angle) = angle.sin_cos();
            bases.push([
                side * 0.240 + sin_angle * 0.066,
                0.900,
                0.012 + cos_angle * 0.064,
            ]);
            outs.push([sin_angle, 0.0, cos_angle]);
        }
        emit_fur_band(
            &mut builder,
            &bases,
            &outs,
            &FurBand {
                core_radius: 0.015,
                strand_length: 0.032,
                shells: CUFF_SHELLS,
                bone,
                occlusion: 0.52,
            },
        );
    }

    finish(builder, device, "snow_character_fur")
}

/// One trim band: how thick its core is, how far the strands reach beyond it, how many
/// shells it is emitted as, and which bone carries it.
struct FurBand {
    core_radius: f32,
    strand_length: f32,
    shells: usize,
    bone: usize,
    occlusion: f32,
}

fn emit_fur_band(builder: &mut Builder, bases: &[[f32; 3]], outs: &[[f32; 3]], band: &FurBand) {
    let columns = bases.len();
    let stride = FUR_ARC_STEPS + 1;

    let mut directions = vec![[0.0_f32; 3]; columns * stride];
    for column in 0..columns {
        let next = (column + 1) % columns;
        let previous = (column + columns - 1) % columns;
        let mut tangent = [
            bases[next][0] - bases[previous][0],
            bases[next][1] - bases[previous][1],
            bases[next][2] - bases[previous][2],
        ];
        let length = crate::rig::norm(tangent).max(1e-6);
        tangent = [
            tangent[0] / length,
            tangent[1] / length,
            tangent[2] / length,
        ];

        let out = outs[column];
        let across = crate::rig::cross(tangent, out);

        for step in 0..=FUR_ARC_STEPS {
            let phi = (step as f32 / FUR_ARC_STEPS as f32 - 0.5) * FUR_ARC;
            let (sin_phi, cos_phi) = phi.sin_cos();
            directions[column * stride + step] = [
                out[0] * cos_phi + across[0] * sin_phi,
                out[1] * cos_phi + across[1] * sin_phi,
                out[2] * cos_phi + across[2] * sin_phi,
            ];
        }
    }

    let mut arc = vec![0.0_f32; columns + 1];
    for column in 1..=columns {
        let a = bases[(column - 1) % columns];
        let b = bases[column % columns];
        arc[column] = arc[column - 1] + crate::rig::norm([b[0] - a[0], b[1] - a[1], b[2] - a[2]]);
    }

    for shell in 0..band.shells {
        let t = shell as f32 / (band.shells - 1) as f32;
        let row_base = builder.positions.len() as u32;

        for (column, arc_length) in arc.iter().enumerate() {
            let index = column % columns;
            for step in 0..=FUR_ARC_STEPS {
                let direction = directions[index * stride + step];
                let radius = band.core_radius + band.strand_length * t;
                let across =
                    (step as f32 / FUR_ARC_STEPS as f32 - 0.5) * FUR_ARC * band.core_radius;
                let vertex = vertex(
                    builder,
                    [
                        bases[index][0] + direction[0] * radius,
                        bases[index][1] + direction[1] * radius,
                        bases[index][2] + direction[2] * radius,
                    ],
                    [*arc_length, across],
                    t,
                    band.occlusion,
                    [band.bone as f32, 1.0, 0.0, 0.0],
                );
                set_normal(builder, vertex, direction);
            }
        }

        for column in 0..columns {
            for step in 0..FUR_ARC_STEPS {
                let a = row_base + (column * stride + step) as u32;
                quad(builder, a, a + 1, a + stride as u32 + 1, a + stride as u32);
            }
        }
    }
}
