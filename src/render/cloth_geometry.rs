use crate::render::geometry::{StaticMesh, upload_mesh};
use crate::systems::cloth;
use crate::systems::cloth::Cloth;
use nightshade::prelude::wgpu;

/// One garment vertex: seven floats, matching `CLOTH_LAYOUT`.
const VERTEX_FLOATS: usize = 7;

/// Builds the render mesh for the simulated garments.
pub fn build_cloth(device: &wgpu::Device, cloth: &Cloth) -> StaticMesh {
    let mut data: Vec<f32> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    for (panel_index, panel) in cloth.panels.iter().enumerate() {
        let across = panel.render_columns;
        let down = panel.render_rows;
        let base = (data.len() / VERTEX_FLOATS) as u32;

        for row in 0..=down {
            let v = row as f32 / down as f32;
            let aux = cloth::aux(panel, v);
            for column in 0..=across {
                let u = column as f32 / across as f32;
                data.extend_from_slice(&[u, v, panel_index as f32]);
                data.extend_from_slice(&[u * panel.weave_u, v * panel.weave_v]);
                data.extend_from_slice(&aux);
            }
        }

        let stride = (across + 1) as u32;
        for row in 0..down as u32 {
            for column in 0..across as u32 {
                let a = base + row * stride + column;
                let b = a + 1;
                let c = a + stride;
                let d = c + 1;
                indices.extend_from_slice(&[a, b, d, a, d, c]);
            }
        }
    }

    upload_mesh(device, "snow_character_cloth", &data, &indices)
}

pub const CLOTH_LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
    array_stride: (VERTEX_FLOATS * 4) as u64,
    step_mode: wgpu::VertexStepMode::Vertex,
    attributes: &[
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x3,
            offset: 0,
            shader_location: 0,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x2,
            offset: 12,
            shader_location: 1,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x2,
            offset: 20,
            shader_location: 2,
        },
    ],
};
