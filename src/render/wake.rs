use crate::constants::{CASCADE_COUNT, CASCADE_FORMAT, DEPTH_FORMAT, HDR_FORMAT};
use crate::render::geometry::{StaticMesh, upload_mesh};
use crate::render::gpu::SnowGpu;
use crate::render::pipelines::{GeometrySpec, UniformSlot, geometry_pipeline, uniform_slot};
use crate::render::pipelines::{sampler_entry, texture_entry, uniform_entry};
use crate::render::uniforms::SnowUniforms;
use crate::shaders::{self, ShaderLibrary};
use crate::systems::wake::{SPINE_MAX, WAKE_COLS, WAKE_ROWS};
use nightshade::prelude::wgpu;

/// How many cascades the wake casts into.
const WAKE_CASCADES: usize = 2;

/// The wake: one static lattice and the three programs that place it.
pub struct WakeRender {
    lattice: StaticMesh,
    beauty_pipeline: wgpu::RenderPipeline,
    cascade_pipeline: wgpu::RenderPipeline,
    prepass_pipeline: wgpu::RenderPipeline,

    beauty_texture_layout: wgpu::BindGroupLayout,
    depth_texture_layout: wgpu::BindGroupLayout,

    beauty_uniforms: UniformSlot,
    prepass_uniforms: UniformSlot,
    cascade_uniforms: [UniformSlot; CASCADE_COUNT],

    beauty_textures: Option<wgpu::BindGroup>,
    depth_textures: Option<wgpu::BindGroup>,
    /// Set from the simulation each frame: there is no wave worth drawing until the
    /// player is actually surfing.
    pub visible: bool,
}

pub fn new(
    device: &wgpu::Device,
    library: &mut ShaderLibrary,
    color_format: wgpu::TextureFormat,
) -> WakeRender {
    let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("snow_wake_uniforms"),
        entries: &[uniform_entry(0)],
    });
    let beauty_texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("snow_wake_textures"),
        entries: &[
            texture_entry(0, false),
            texture_entry(1, true),
            sampler_entry(2, true),
            texture_entry(3, true),
            texture_entry(4, true),
            texture_entry(5, true),
            sampler_entry(6, true),
        ],
    });
    let depth_texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("snow_wake_depth_textures"),
        entries: &[texture_entry(0, false)],
    });

    let beauty_module = shaders::compile(library, device, "wake", shaders::WAKE);
    let cascade_module = shaders::compile(library, device, "wake_depth", shaders::WAKE_DEPTH);
    let prepass_module = shaders::compile(library, device, "wake_prepass", shaders::WAKE_PREPASS);

    WakeRender {
        lattice: build_lattice(device),
        beauty_pipeline: geometry_pipeline(
            device,
            GeometrySpec {
                label: "snow_wake",
                module: &beauty_module,
                layouts: &[Some(&uniform_layout), Some(&beauty_texture_layout)],
                vertices: LATTICE_LAYOUT,
                color: color_format,
                blend: None,
                depth: Some(DEPTH_FORMAT),
            },
        ),
        cascade_pipeline: geometry_pipeline(
            device,
            GeometrySpec {
                label: "snow_wake_cascade",
                module: &cascade_module,
                layouts: &[Some(&uniform_layout), Some(&depth_texture_layout)],
                vertices: LATTICE_LAYOUT,
                color: CASCADE_FORMAT,
                blend: None,
                depth: Some(DEPTH_FORMAT),
            },
        ),
        prepass_pipeline: geometry_pipeline(
            device,
            GeometrySpec {
                label: "snow_wake_prepass",
                module: &prepass_module,
                layouts: &[Some(&uniform_layout), Some(&depth_texture_layout)],
                vertices: LATTICE_LAYOUT,
                color: HDR_FORMAT,
                blend: None,
                depth: Some(DEPTH_FORMAT),
            },
        ),
        beauty_uniforms: uniform_slot(
            device,
            "snow_wake",
            &uniform_layout,
            std::mem::size_of::<SnowUniforms>() as u64,
        ),
        prepass_uniforms: uniform_slot(
            device,
            "snow_wake_prepass",
            &uniform_layout,
            std::mem::size_of::<SnowUniforms>() as u64,
        ),
        cascade_uniforms: std::array::from_fn(|_| {
            uniform_slot(
                device,
                "snow_wake_cascade",
                &uniform_layout,
                std::mem::size_of::<SnowUniforms>() as u64,
            )
        }),
        beauty_texture_layout,
        depth_texture_layout,
        beauty_textures: None,
        depth_textures: None,
        visible: false,
    }
}

pub fn bind(wake: &mut WakeRender, device: &wgpu::Device, gpu: &SnowGpu) {
    wake.beauty_textures = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("snow_wake_textures"),
        layout: &wake.beauty_texture_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&gpu.wake.view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&gpu.sky_lut.view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(&gpu.sky_sampler),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(&gpu.cascades[0].view),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::TextureView(&gpu.cascades[1].view),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::TextureView(&gpu.cascades[2].view),
            },
            wgpu::BindGroupEntry {
                binding: 6,
                resource: wgpu::BindingResource::Sampler(&gpu.linear_clamp),
            },
        ],
    }));
    wake.depth_textures = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("snow_wake_depth_textures"),
        layout: &wake.depth_texture_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(&gpu.wake.view),
        }],
    }));
}

/// Uploads this frame's uniform block and spine.
pub fn write(
    wake: &WakeRender,
    queue: &wgpu::Queue,
    gpu: &SnowGpu,
    uniforms: &SnowUniforms,
    cascades: &[[f32; 16]],
    texels: &[f32],
) {
    queue.write_buffer(
        &wake.beauty_uniforms.buffer,
        0,
        bytemuck::bytes_of(uniforms),
    );
    queue.write_buffer(
        &wake.prepass_uniforms.buffer,
        0,
        bytemuck::bytes_of(uniforms),
    );
    for (index, slot) in wake.cascade_uniforms.iter().enumerate() {
        let mut cascade = *uniforms;
        cascade.view_projection = cascades[index];
        queue.write_buffer(&slot.buffer, 0, bytemuck::bytes_of(&cascade));
    }

    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &gpu.wake.texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        bytemuck::cast_slice(texels),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(SPINE_MAX as u32 * 16),
            rows_per_image: Some(3),
        },
        wgpu::Extent3d {
            width: SPINE_MAX as u32,
            height: 3,
            depth_or_array_layers: 1,
        },
    );
}

fn draw(
    wake: &WakeRender,
    pass: &mut wgpu::RenderPass<'_>,
    pipeline: &wgpu::RenderPipeline,
    uniforms: &UniformSlot,
    textures: &wgpu::BindGroup,
) {
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, &uniforms.bind_group, &[]);
    pass.set_bind_group(1, textures, &[]);
    pass.set_vertex_buffer(0, wake.lattice.vertices.slice(..));
    pass.set_index_buffer(wake.lattice.indices.slice(..), wgpu::IndexFormat::Uint32);
    pass.draw_indexed(0..wake.lattice.index_count, 0, 0..1);
}

pub fn draw_cascade(wake: &WakeRender, pass: &mut wgpu::RenderPass<'_>, cascade: usize) {
    if !wake.visible || cascade >= WAKE_CASCADES {
        return;
    }
    let Some(textures) = &wake.depth_textures else {
        return;
    };
    draw(
        wake,
        pass,
        &wake.cascade_pipeline,
        &wake.cascade_uniforms[cascade],
        textures,
    );
}

pub fn draw_prepass(wake: &WakeRender, pass: &mut wgpu::RenderPass<'_>) {
    if !wake.visible {
        return;
    }
    let Some(textures) = &wake.depth_textures else {
        return;
    };
    draw(
        wake,
        pass,
        &wake.prepass_pipeline,
        &wake.prepass_uniforms,
        textures,
    );
}

pub fn draw_beauty(wake: &WakeRender, pass: &mut wgpu::RenderPass<'_>) {
    if !wake.visible {
        return;
    }
    let Some(textures) = &wake.beauty_textures else {
        return;
    };
    draw(
        wake,
        pass,
        &wake.beauty_pipeline,
        &wake.beauty_uniforms,
        textures,
    );
}

pub fn draw_calls(wake: &WakeRender) -> u32 {
    if wake.visible {
        WAKE_CASCADES as u32 + 2
    } else {
        0
    }
}

pub fn triangles(wake: &WakeRender) -> u32 {
    (wake.lattice.index_count / 3) * draw_calls(wake)
}

/// The static lattice.
fn build_lattice(device: &wgpu::Device) -> StaticMesh {
    let per_side = WAKE_COLS * WAKE_ROWS;
    let mut data = Vec::with_capacity(per_side * 2 * 3);
    let mut indices = Vec::with_capacity((WAKE_COLS - 1) * (WAKE_ROWS - 1) * 2 * 6);

    for wall in 0..2 {
        let side = if wall == 0 { -1.0 } else { 1.0 };
        let base = (wall * per_side) as u32;
        for column in 0..WAKE_COLS {
            for row in 0..WAKE_ROWS {
                data.extend_from_slice(&[column as f32, row as f32, side]);
            }
        }
        for column in 0..WAKE_COLS as u32 - 1 {
            for row in 0..WAKE_ROWS as u32 - 1 {
                let a = base + column * WAKE_ROWS as u32 + row;
                let b = a + WAKE_ROWS as u32;
                indices.extend_from_slice(&[a, b, b + 1, a, b + 1, a + 1]);
            }
        }
    }

    upload_mesh(device, "snow_wake", &data, &indices)
}

const LATTICE_LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
    array_stride: 12,
    step_mode: wgpu::VertexStepMode::Vertex,
    attributes: &[wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x3,
        offset: 0,
        shader_location: 0,
    }],
};
