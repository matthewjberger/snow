use crate::constants::{CASCADE_COUNT, CASCADE_FORMAT, DEPTH_FORMAT, HDR_FORMAT};
use crate::render::geometry::{StaticMesh, upload_mesh};
use crate::render::gpu::SnowGpu;
use crate::render::pipelines::{GeometrySpec, UniformSlot, geometry_pipeline, uniform_slot};
use crate::render::pipelines::{sampler_entry, texture_entry, uniform_entry};
use crate::render::uniforms::SnowUniforms;
use crate::shaders::{self, ShaderLibrary};
use crate::systems::spell::crystals::{self, CRYSTAL_MAX, CRYSTAL_RING, CRYSTAL_VERTS};
use nightshade::prelude::wgpu;
use nightshade::prelude::*;

/// How many cascades a prism this size is worth drawing into.
const CRYSTAL_CASCADES: usize = 2;

/// The ice formations: one static mesh and the three programs that place it.
pub struct CrystalRender {
    mesh: StaticMesh,
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
    pub visible: bool,
    live: u32,
    /// The prism table, refilled from the live entities each frame.
    staging: Vec<f32>,
}

pub fn new(
    device: &wgpu::Device,
    library: &mut ShaderLibrary,
    color_format: wgpu::TextureFormat,
) -> CrystalRender {
    let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("snow_crystal_uniforms"),
        entries: &[uniform_entry(0)],
    });
    let beauty_texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("snow_crystal_textures"),
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
        label: Some("snow_crystal_depth_textures"),
        entries: &[texture_entry(0, false)],
    });

    let beauty_module = shaders::compile(library, device, "crystal", shaders::CRYSTAL);
    let cascade_module = shaders::compile(library, device, "crystal_depth", shaders::CRYSTAL_DEPTH);
    let prepass_module =
        shaders::compile(library, device, "crystal_prepass", shaders::CRYSTAL_PREPASS);

    CrystalRender {
        mesh: build_prisms(device),
        beauty_pipeline: geometry_pipeline(
            device,
            GeometrySpec {
                label: "snow_crystal",
                module: &beauty_module,
                layouts: &[Some(&uniform_layout), Some(&beauty_texture_layout)],
                vertices: PRISM_LAYOUT,
                color: color_format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                depth: Some(DEPTH_FORMAT),
            },
        ),
        cascade_pipeline: geometry_pipeline(
            device,
            GeometrySpec {
                label: "snow_crystal_cascade",
                module: &cascade_module,
                layouts: &[Some(&uniform_layout), Some(&depth_texture_layout)],
                vertices: PRISM_LAYOUT,
                color: CASCADE_FORMAT,
                blend: None,
                depth: Some(DEPTH_FORMAT),
            },
        ),
        prepass_pipeline: geometry_pipeline(
            device,
            GeometrySpec {
                label: "snow_crystal_prepass",
                module: &prepass_module,
                layouts: &[Some(&uniform_layout), Some(&depth_texture_layout)],
                vertices: PRISM_LAYOUT,
                color: HDR_FORMAT,
                blend: None,
                depth: Some(DEPTH_FORMAT),
            },
        ),
        beauty_uniforms: uniform_slot(
            device,
            "snow_crystal",
            &uniform_layout,
            std::mem::size_of::<SnowUniforms>() as u64,
        ),
        prepass_uniforms: uniform_slot(
            device,
            "snow_crystal_prepass",
            &uniform_layout,
            std::mem::size_of::<SnowUniforms>() as u64,
        ),
        cascade_uniforms: std::array::from_fn(|_| {
            uniform_slot(
                device,
                "snow_crystal_cascade",
                &uniform_layout,
                std::mem::size_of::<SnowUniforms>() as u64,
            )
        }),
        beauty_texture_layout,
        depth_texture_layout,
        beauty_textures: None,
        depth_textures: None,
        visible: false,
        staging: vec![0.0; CRYSTAL_MAX * 4 * 3],
        live: 0,
    }
}

pub fn bind(crystal: &mut CrystalRender, device: &wgpu::Device, gpu: &SnowGpu) {
    crystal.beauty_textures = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("snow_crystal_textures"),
        layout: &crystal.beauty_texture_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&gpu.crystal.view),
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
    crystal.depth_textures = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("snow_crystal_depth_textures"),
        layout: &crystal.depth_texture_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(&gpu.crystal.view),
        }],
    }));
}

/// Gathers the standing prisms out of the world and uploads them.
///
/// Reporting whether anything is there, because a formation that has fully
/// sublimated must stop being drawn rather than draw zero instances.
pub fn write(
    crystal: &mut CrystalRender,
    queue: &wgpu::Queue,
    gpu: &SnowGpu,
    uniforms: &SnowUniforms,
    cascades: &[[f32; 16]],
    world: &World,
) -> bool {
    crystal.live = crystals::gather(world, &mut crystal.staging) as u32;
    if crystal.live == 0 {
        return false;
    }
    queue.write_buffer(
        &crystal.beauty_uniforms.buffer,
        0,
        bytemuck::bytes_of(uniforms),
    );
    queue.write_buffer(
        &crystal.prepass_uniforms.buffer,
        0,
        bytemuck::bytes_of(uniforms),
    );
    for (index, slot) in crystal.cascade_uniforms.iter().enumerate() {
        let mut cascade = *uniforms;
        cascade.view_projection = cascades[index];
        queue.write_buffer(&slot.buffer, 0, bytemuck::bytes_of(&cascade));
    }

    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &gpu.crystal.texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        bytemuck::cast_slice(&crystal.staging),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(CRYSTAL_MAX as u32 * 16),
            rows_per_image: Some(3),
        },
        wgpu::Extent3d {
            width: CRYSTAL_MAX as u32,
            height: 3,
            depth_or_array_layers: 1,
        },
    );
    true
}

fn draw(
    crystal: &CrystalRender,
    pass: &mut wgpu::RenderPass<'_>,
    pipeline: &wgpu::RenderPipeline,
    uniforms: &UniformSlot,
    textures: &wgpu::BindGroup,
) {
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, &uniforms.bind_group, &[]);
    pass.set_bind_group(1, textures, &[]);
    pass.set_vertex_buffer(0, crystal.mesh.vertices.slice(..));
    pass.set_index_buffer(crystal.mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
    pass.draw_indexed(0..crystal.mesh.index_count, 0, 0..1);
}

pub fn draw_cascade(crystal: &CrystalRender, pass: &mut wgpu::RenderPass<'_>, cascade: usize) {
    if !crystal.visible || cascade >= CRYSTAL_CASCADES {
        return;
    }
    let Some(textures) = &crystal.depth_textures else {
        return;
    };
    draw(
        crystal,
        pass,
        &crystal.cascade_pipeline,
        &crystal.cascade_uniforms[cascade],
        textures,
    );
}

pub fn draw_prepass(crystal: &CrystalRender, pass: &mut wgpu::RenderPass<'_>) {
    if !crystal.visible {
        return;
    }
    let Some(textures) = &crystal.depth_textures else {
        return;
    };
    draw(
        crystal,
        pass,
        &crystal.prepass_pipeline,
        &crystal.prepass_uniforms,
        textures,
    );
}

pub fn draw_beauty(crystal: &CrystalRender, pass: &mut wgpu::RenderPass<'_>) {
    if !crystal.visible {
        return;
    }
    let Some(textures) = &crystal.beauty_textures else {
        return;
    };
    draw(
        crystal,
        pass,
        &crystal.beauty_pipeline,
        &crystal.beauty_uniforms,
        textures,
    );
}

pub fn draw_calls(crystal: &CrystalRender) -> u32 {
    if crystal.visible {
        CRYSTAL_CASCADES as u32 + 2
    } else {
        0
    }
}

pub fn triangles(crystal: &CrystalRender) -> u32 {
    if !crystal.visible {
        return 0;
    }
    (CRYSTAL_RING as u32 * 3) * crystal.live * draw_calls(crystal)
}

/// The static prism pool. The one attribute is a crystal index and a vertex
/// index, and it carries no geometry at all.
fn build_prisms(device: &wgpu::Device) -> StaticMesh {
    let mut data = Vec::with_capacity(CRYSTAL_MAX * CRYSTAL_VERTS * 3);
    let mut indices = Vec::with_capacity(CRYSTAL_MAX * CRYSTAL_RING * 9);

    for crystal in 0..CRYSTAL_MAX {
        for vertex in 0..CRYSTAL_VERTS {
            data.extend_from_slice(&[crystal as f32, vertex as f32, 0.0]);
        }
        let base = (crystal * CRYSTAL_VERTS) as u32;
        for facet in 0..CRYSTAL_RING as u32 {
            let next = (facet + 1) % CRYSTAL_RING as u32;
            let low = base + facet;
            let low_next = base + next;
            let high = base + CRYSTAL_RING as u32 + facet;
            let high_next = base + CRYSTAL_RING as u32 + next;
            let apex = base + CRYSTAL_RING as u32 * 2;
            indices.extend_from_slice(&[
                low, high, high_next, low, high_next, low_next, high, apex, high_next,
            ]);
        }
    }

    upload_mesh(device, "snow_crystal", &data, &indices)
}

const PRISM_LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
    array_stride: 12,
    step_mode: wgpu::VertexStepMode::Vertex,
    attributes: &[wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x3,
        offset: 0,
        shader_location: 0,
    }],
};
