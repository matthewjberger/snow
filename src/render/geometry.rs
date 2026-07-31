use crate::constants::{CLIPMAP_LEVELS, GRID_N, HOLE_SHRINK};
use nightshade::prelude::wgpu;

/// A static mesh: one vertex buffer, one index buffer, built once.
pub struct StaticMesh {
    pub vertices: wgpu::Buffer,
    pub indices: wgpu::Buffer,
    pub index_count: u32,
}

pub const GRID_HALF_N: f32 = GRID_N as f32 * 0.5;

/// Builds the nested-ring clipmap as a single static mesh.
pub fn build_clipmap(device: &wgpu::Device) -> StaticMesh {
    let side = (GRID_N + 1) as usize;
    let half = (GRID_N / 2) as i32;
    let vertices_per_level = side * side;

    let mut positions = Vec::with_capacity(vertices_per_level * CLIPMAP_LEVELS as usize * 3);
    let mut indices: Vec<u32> = Vec::new();
    let hole_half = half / 2 - HOLE_SHRINK;

    for level in 0..CLIPMAP_LEVELS {
        let base = level as usize * vertices_per_level;

        for j in 0..=GRID_N as i32 {
            let grid_j = (j - half) as f32;
            for i in 0..=GRID_N as i32 {
                positions.push((i - half) as f32);
                positions.push(level as f32);
                positions.push(grid_j);
            }
        }

        for j in 0..GRID_N as i32 {
            let grid_j = j - half;
            for i in 0..GRID_N as i32 {
                let grid_i = i - half;

                if level > 0 {
                    let max_abs = grid_i
                        .abs()
                        .max((grid_i + 1).abs())
                        .max(grid_j.abs())
                        .max((grid_j + 1).abs());
                    if max_abs <= hole_half {
                        continue;
                    }
                }

                let a = (base + j as usize * side + i as usize) as u32;
                let b = a + 1;
                let c = a + side as u32;
                let d = c + 1;

                if (i + j) & 1 == 0 {
                    indices.extend_from_slice(&[a, b, c, b, d, c]);
                } else {
                    indices.extend_from_slice(&[a, d, c, a, b, d]);
                }
            }
        }
    }

    upload_mesh(device, "snow_clipmap", &positions, &indices)
}

/// The skybox: a unit cube whose object-space position is the view ray.
pub fn build_skybox(device: &wgpu::Device) -> StaticMesh {
    let corners: [[f32; 3]; 8] = [
        [-1.0, -1.0, -1.0],
        [1.0, -1.0, -1.0],
        [1.0, 1.0, -1.0],
        [-1.0, 1.0, -1.0],
        [-1.0, -1.0, 1.0],
        [1.0, -1.0, 1.0],
        [1.0, 1.0, 1.0],
        [-1.0, 1.0, 1.0],
    ];
    let faces: [[u32; 4]; 6] = [
        [0, 1, 2, 3],
        [5, 4, 7, 6],
        [4, 0, 3, 7],
        [1, 5, 6, 2],
        [3, 2, 6, 7],
        [4, 5, 1, 0],
    ];

    let mut positions = Vec::with_capacity(corners.len() * 3);
    for corner in corners {
        positions.extend_from_slice(&corner);
    }

    let mut indices = Vec::with_capacity(faces.len() * 6);
    for face in faces {
        indices.extend_from_slice(&[face[0], face[1], face[2], face[0], face[2], face[3]]);
    }

    upload_mesh(device, "snow_skybox", &positions, &indices)
}

/// Uploads an interleaved vertex buffer whose layout the caller declares.
pub fn upload_mesh(
    device: &wgpu::Device,
    label: &str,
    positions: &[f32],
    indices: &[u32],
) -> StaticMesh {
    let vertices = create_buffer(
        device,
        label,
        bytemuck::cast_slice(positions),
        wgpu::BufferUsages::VERTEX,
    );
    let index_buffer = create_buffer(
        device,
        label,
        bytemuck::cast_slice(indices),
        wgpu::BufferUsages::INDEX,
    );
    StaticMesh {
        vertices,
        indices: index_buffer,
        index_count: indices.len() as u32,
    }
}

fn create_buffer(
    device: &wgpu::Device,
    label: &str,
    contents: &[u8],
    usage: wgpu::BufferUsages,
) -> wgpu::Buffer {
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: contents.len() as u64,
        usage,
        mapped_at_creation: true,
    });
    buffer
        .slice(..)
        .get_mapped_range_mut()
        .copy_from_slice(contents);
    buffer.unmap();
    buffer
}

/// The vertex layout every data-driven mesh here shares: three floats that carry
/// addressing rather than geometry.
pub const PACKED_LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
    array_stride: 12,
    step_mode: wgpu::VertexStepMode::Vertex,
    attributes: &[wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x3,
        offset: 0,
        shader_location: 0,
    }],
};
