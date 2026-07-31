//! The lofting toolkit the figure's meshes are built with.
//!
//! One vertex format, one ring-to-ring loft, and three builders on top of it, so
//! the body, the cowl and the fur trim all deform from the same skeleton and the
//! same skin weights rather than each inventing its own.

pub mod body;
pub mod fur;
pub mod hood;

pub use body::build_body;
pub use fur::build_fur;

use crate::render::geometry::{StaticMesh, upload_mesh};
use crate::systems::figure::{B_CHEST, B_NECK, B_ROOT, B_SPINE};
use nightshade::prelude::wgpu;

pub const M_ROBE: f32 = 0.0;
pub const M_LEATHER: f32 = 3.0;
pub const M_SKIN: f32 = 4.0;
pub const M_TRIM: f32 = 5.0;

/// Segments around a limb.
const SEG: usize = 14;

const HEAD_CENTRE: [f32; 3] = [0.0, 1.655, 0.005];

/// Shells per fur band.
const HOOD_SHELLS: usize = 22;
const CUFF_SHELLS: usize = 18;

/// Cross-section steps across a fur band, and the arc they cover in radians.
const FUR_ARC_STEPS: usize = 4;
const FUR_ARC: f32 = 2.1;

const HOOD_COLS: usize = 34;
const HOOD_ROWS: usize = 9;

/// One skinned vertex: eighteen floats, matching `SKINNED_LAYOUT`.
const VERTEX_FLOATS: usize = 18;

/// Accumulates a procedural mesh.
pub struct Builder {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    /// Material slot and baked occlusion on the body; the shell parameter and occlusion
    /// on the fur.
    aux: Vec<[f32; 2]>,
    bone_indices: Vec<[f32; 4]>,
    bone_weights: Vec<[f32; 4]>,
    indices: Vec<u32>,
    explicit_normals: bool,
}

/// A lofted cross-section: centre, two radii, occlusion and a two-bone binding.
#[derive(Clone, Copy)]
pub struct Ring {
    centre: [f32; 3],
    radius: [f32; 2],
    occlusion: f32,
    bones: [f32; 4],
}

fn ring(centre: [f32; 3], radius: [f32; 2], occlusion: f32, bones: [f32; 4]) -> Ring {
    Ring {
        centre,
        radius,
        occlusion,
        bones,
    }
}

pub fn builder() -> Builder {
    Builder {
        positions: Vec::new(),
        normals: Vec::new(),
        uvs: Vec::new(),
        aux: Vec::new(),
        bone_indices: Vec::new(),
        bone_weights: Vec::new(),
        indices: Vec::new(),
        explicit_normals: false,
    }
}

pub fn vertex(
    builder: &mut Builder,
    position: [f32; 3],
    uv: [f32; 2],
    material: f32,
    occlusion: f32,
    bones: [f32; 4],
) -> u32 {
    builder.positions.push(position);
    builder.normals.push([0.0; 3]);
    builder.uvs.push(uv);
    builder.aux.push([material, occlusion]);
    builder.bone_indices.push([bones[0], bones[2], 0.0, 0.0]);
    builder.bone_weights.push([bones[1], bones[3], 0.0, 0.0]);
    builder.positions.len() as u32 - 1
}

pub fn set_normal(builder: &mut Builder, vertex: u32, normal: [f32; 3]) {
    builder.normals[vertex as usize] = normal;
}

pub fn triangle(builder: &mut Builder, a: u32, b: u32, c: u32) {
    builder.indices.extend_from_slice(&[a, b, c]);
}

pub fn quad(builder: &mut Builder, a: u32, b: u32, c: u32, d: u32) {
    builder.indices.extend_from_slice(&[a, b, c, a, c, d]);
}

/// Area-weighted smooth normals.
pub fn compute_normals(builder: &mut Builder) {
    if builder.explicit_normals {
        return;
    }
    let mut accumulated = vec![[0.0_f32; 3]; builder.positions.len()];
    for triangle in builder.indices.chunks_exact(3) {
        let (a, b, c) = (
            builder.positions[triangle[0] as usize],
            builder.positions[triangle[1] as usize],
            builder.positions[triangle[2] as usize],
        );
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let face = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        for index in triangle {
            let slot = &mut accumulated[*index as usize];
            slot[0] += face[0];
            slot[1] += face[1];
            slot[2] += face[2];
        }
    }
    for (normal, sum) in builder.normals.iter_mut().zip(accumulated) {
        let length = (sum[0] * sum[0] + sum[1] * sum[1] + sum[2] * sum[2])
            .sqrt()
            .max(1e-6);
        *normal = [sum[0] / length, sum[1] / length, sum[2] / length];
    }
}

pub fn finish(mut builder: Builder, device: &wgpu::Device, label: &str) -> StaticMesh {
    compute_normals(&mut builder);
    let mut data = Vec::with_capacity(builder.positions.len() * VERTEX_FLOATS);
    for index in 0..builder.positions.len() {
        data.extend_from_slice(&builder.positions[index]);
        data.extend_from_slice(&builder.normals[index]);
        data.extend_from_slice(&builder.uvs[index]);
        data.extend_from_slice(&builder.aux[index]);
        data.extend_from_slice(&builder.bone_indices[index]);
        data.extend_from_slice(&builder.bone_weights[index]);
    }
    upload_mesh(device, label, &data, &builder.indices)
}

/// Lofts a closed tube through a list of rings.
pub fn loft(
    builder: &mut Builder,
    rings: &[Ring],
    material: f32,
    reference: [f32; 3],
    cap_start: bool,
    cap_end: bool,
) {
    let count = rings.len();
    let mut previous_row: Option<Vec<u32>> = None;
    let mut first_row: Vec<u32> = Vec::new();
    let mut travelled = 0.0;

    for index in 0..count {
        let current = rings[index];
        let previous = rings[index.saturating_sub(1)];
        let next = rings[(index + 1).min(count - 1)];

        let mut axis = [
            next.centre[0] - previous.centre[0],
            next.centre[1] - previous.centre[1],
            next.centre[2] - previous.centre[2],
        ];
        let length = crate::rig::norm(axis).max(1e-6);
        axis = [axis[0] / length, axis[1] / length, axis[2] / length];

        let mut u = crate::rig::cross(axis, reference);
        let u_length = crate::rig::norm(u).max(1e-6);
        u = [u[0] / u_length, u[1] / u_length, u[2] / u_length];
        let w = crate::rig::cross(axis, u);

        if index > 0 {
            travelled += crate::rig::norm([
                current.centre[0] - previous.centre[0],
                current.centre[1] - previous.centre[1],
                current.centre[2] - previous.centre[2],
            ]);
        }

        let circumference = std::f32::consts::PI * (current.radius[0] + current.radius[1]);

        let mut row = Vec::with_capacity(SEG);
        for segment in 0..SEG {
            let angle = (segment as f32 / SEG as f32) * std::f32::consts::TAU;
            let (sin_angle, cos_angle) = angle.sin_cos();
            let position = [
                current.centre[0]
                    + u[0] * current.radius[0] * sin_angle
                    + w[0] * current.radius[1] * cos_angle,
                current.centre[1]
                    + u[1] * current.radius[0] * sin_angle
                    + w[1] * current.radius[1] * cos_angle,
                current.centre[2]
                    + u[2] * current.radius[0] * sin_angle
                    + w[2] * current.radius[1] * cos_angle,
            ];
            row.push(vertex(
                builder,
                position,
                [(segment as f32 / SEG as f32) * circumference, travelled],
                material,
                current.occlusion,
                current.bones,
            ));
        }

        if let Some(previous_row) = &previous_row {
            for segment in 0..SEG {
                let next_segment = (segment + 1) % SEG;
                quad(
                    builder,
                    previous_row[segment],
                    previous_row[next_segment],
                    row[next_segment],
                    row[segment],
                );
            }
        }
        if index == 0 {
            first_row = row.clone();
        }
        previous_row = Some(row);
    }

    if cap_start {
        cap_ring(builder, rings[0], rings[1], &first_row, material, true);
    }
    if cap_end && let Some(last_row) = &previous_row {
        cap_ring(
            builder,
            rings[count - 1],
            rings[count - 2],
            last_row,
            material,
            false,
        );
    }
}

/// A fan to a centre vertex placed on the ring's own axis.
pub fn cap_ring(
    builder: &mut Builder,
    ring: Ring,
    neighbour: Ring,
    row: &[u32],
    material: f32,
    is_start: bool,
) {
    let mut axis = [
        ring.centre[0] - neighbour.centre[0],
        ring.centre[1] - neighbour.centre[1],
        ring.centre[2] - neighbour.centre[2],
    ];
    let length = crate::rig::norm(axis).max(1e-6);
    axis = [axis[0] / length, axis[1] / length, axis[2] / length];
    let extent = ring.radius[0].max(ring.radius[1]) * 0.7;
    let centre = vertex(
        builder,
        [
            ring.centre[0] + axis[0] * extent,
            ring.centre[1] + axis[1] * extent,
            ring.centre[2] + axis[2] * extent,
        ],
        [0.5, 0.5],
        material,
        ring.occlusion,
        ring.bones,
    );
    for segment in 0..SEG {
        let next_segment = (segment + 1) % SEG;
        if is_start {
            triangle(builder, centre, row[next_segment], row[segment]);
        } else {
            triangle(builder, centre, row[segment], row[next_segment]);
        }
    }
}

/// Bone blend along the spine, by bind-pose height.
fn spine_bones(height: f32) -> [f32; 4] {
    if height < 1.06 {
        let t = ((height - 0.88) / 0.18).clamp(0.0, 1.0);
        return [B_ROOT as f32, 1.0 - t * 0.5, B_SPINE as f32, t * 0.5];
    }
    if height < 1.26 {
        let t = (height - 1.06) / 0.20;
        return [B_SPINE as f32, 1.0 - t, B_CHEST as f32, t];
    }
    let t = ((height - 1.26) / 0.20).min(1.0);
    [B_CHEST as f32, 1.0 - t * 0.35, B_NECK as f32, t * 0.35]
}

/// Rings along a straight bone segment, interpolating radius and bone weights.
fn limb_rings(
    from: [f32; 3],
    to: [f32; 3],
    radius: [f32; 2],
    steps: usize,
    bones: [usize; 2],
    occlusion: f32,
    blend: [f32; 2],
) -> Vec<Ring> {
    (0..=steps)
        .map(|step| {
            let t = step as f32 / steps as f32;
            let weight = ((t - blend[0]) / (blend[1] - blend[0])).clamp(0.0, 1.0);
            let r = radius[0] + (radius[1] - radius[0]) * t;
            ring(
                [
                    from[0] + (to[0] - from[0]) * t,
                    from[1] + (to[1] - from[1]) * t,
                    from[2] + (to[2] - from[2]) * t,
                ],
                [r, r],
                occlusion,
                [bones[0] as f32, 1.0 - weight, bones[1] as f32, weight],
            )
        })
        .collect()
}

pub const SKINNED_LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
    array_stride: (VERTEX_FLOATS * 4) as u64,
    step_mode: wgpu::VertexStepMode::Vertex,
    attributes: &[
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x3,
            offset: 0,
            shader_location: 0,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x3,
            offset: 12,
            shader_location: 1,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x2,
            offset: 24,
            shader_location: 2,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x2,
            offset: 32,
            shader_location: 3,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x4,
            offset: 40,
            shader_location: 4,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x4,
            offset: 56,
            shader_location: 5,
        },
    ],
};
