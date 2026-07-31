use crate::constants::*;
use crate::render::geometry::{PACKED_LAYOUT, StaticMesh, build_clipmap};
use crate::render::gpu::SnowGpu;
use crate::render::pipelines::{GeometrySpec, UniformSlot, geometry_pipeline, uniform_slot};
use crate::render::pipelines::{sampler_entry, texture_entry, uniform_entry};
use crate::render::uniforms::SnowUniforms;
use crate::shaders::{self, ShaderLibrary};
use nightshade::prelude::wgpu;

/// The bindings the depth-only terrain programs take: the height field, its derived
/// channels and the terrain state buffer.
fn depth_texture_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("snow_terrain_depth_textures"),
        entries: &[
            texture_entry(0, true),
            sampler_entry(1, true),
            texture_entry(2, true),
            sampler_entry(3, true),
            texture_entry(4, true),
            sampler_entry(5, true),
        ],
    })
}

/// The beauty pass adds the grain map, the sky and the three cascades.
fn beauty_texture_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("snow_terrain_textures"),
        entries: &[
            texture_entry(0, true),
            sampler_entry(1, true),
            texture_entry(2, true),
            sampler_entry(3, true),
            texture_entry(4, true),
            sampler_entry(5, true),
            texture_entry(6, true),
            sampler_entry(7, true),
            texture_entry(8, true),
            sampler_entry(9, true),
            texture_entry(10, true),
            texture_entry(11, true),
            texture_entry(12, true),
            sampler_entry(13, true),
        ],
    })
}

fn uniform_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("snow_terrain_uniforms"),
        entries: &[uniform_entry(0)],
    })
}

/// The clipmap and the three programs that draw it.
pub struct Terrain {
    pub mesh: StaticMesh,
    depth_textures: wgpu::BindGroupLayout,
    beauty_textures: wgpu::BindGroupLayout,

    cascade_pipeline: wgpu::RenderPipeline,
    prepass_pipeline: wgpu::RenderPipeline,
    beauty_pipeline: wgpu::RenderPipeline,

    pub cascade_uniforms: [UniformSlot; CASCADE_COUNT],
    pub prepass_uniforms: UniformSlot,
    pub beauty_uniforms: UniformSlot,

    depth_bind_groups: Option<[wgpu::BindGroup; 2]>,
    beauty_bind_groups: Option<[wgpu::BindGroup; 2]>,
    cascade_views: Option<[wgpu::TextureView; CASCADE_COUNT]>,
    cascade_depth_view: Option<wgpu::TextureView>,
    pub visible: bool,
}

pub fn new(
    device: &wgpu::Device,
    library: &mut ShaderLibrary,
    color_format: wgpu::TextureFormat,
) -> Terrain {
    let uniforms = uniform_layout(device);
    let depth_textures = depth_texture_layout(device);
    let beauty_textures = beauty_texture_layout(device);

    let cascade_module = shaders::compile(library, device, "terrain_depth", shaders::TERRAIN_DEPTH);
    let prepass_module =
        shaders::compile(library, device, "terrain_prepass", shaders::TERRAIN_PREPASS);
    let beauty_module = shaders::compile(library, device, "snow", shaders::SNOW);

    Terrain {
        mesh: build_clipmap(device),
        cascade_pipeline: geometry_pipeline(
            device,
            GeometrySpec {
                label: "snow_terrain_cascade",
                module: &cascade_module,
                layouts: &[Some(&uniforms), Some(&depth_textures)],
                vertices: PACKED_LAYOUT,
                color: CASCADE_FORMAT,
                blend: None,
                depth: Some(DEPTH_FORMAT),
            },
        ),
        prepass_pipeline: geometry_pipeline(
            device,
            GeometrySpec {
                label: "snow_terrain_prepass",
                module: &prepass_module,
                layouts: &[Some(&uniforms), Some(&depth_textures)],
                vertices: PACKED_LAYOUT,
                color: HDR_FORMAT,
                blend: None,
                depth: Some(DEPTH_FORMAT),
            },
        ),
        beauty_pipeline: geometry_pipeline(
            device,
            GeometrySpec {
                label: "snow_terrain",
                module: &beauty_module,
                layouts: &[Some(&uniforms), Some(&beauty_textures)],
                vertices: PACKED_LAYOUT,
                color: color_format,
                blend: None,
                depth: Some(DEPTH_FORMAT),
            },
        ),
        cascade_uniforms: std::array::from_fn(|_| {
            uniform_slot(
                device,
                "snow_cascade_uniforms",
                &uniforms,
                std::mem::size_of::<SnowUniforms>() as u64,
            )
        }),
        prepass_uniforms: uniform_slot(
            device,
            "snow_prepass_uniforms",
            &uniforms,
            std::mem::size_of::<SnowUniforms>() as u64,
        ),
        beauty_uniforms: uniform_slot(
            device,
            "snow_beauty_uniforms",
            &uniforms,
            std::mem::size_of::<SnowUniforms>() as u64,
        ),
        depth_textures,
        beauty_textures,
        depth_bind_groups: None,
        beauty_bind_groups: None,
        cascade_views: None,
        cascade_depth_view: None,
        visible: true,
    }
}

/// The colour and depth attachments for one cascade.
pub fn cascade_targets(
    terrain: &Terrain,
    cascade: usize,
) -> Option<(&wgpu::TextureView, &wgpu::TextureView)> {
    let views = terrain.cascade_views.as_ref()?;
    let depth = terrain.cascade_depth_view.as_ref()?;
    Some((&views[cascade], depth))
}

/// Binds the persistent textures.
pub fn bind(terrain: &mut Terrain, device: &wgpu::Device, gpu: &SnowGpu) {
    terrain.cascade_views = Some(std::array::from_fn(|index| {
        gpu.cascades[index].view.clone()
    }));
    terrain.cascade_depth_view = Some(gpu.cascade_depth.view.clone());

    terrain.depth_bind_groups = Some(std::array::from_fn(|parity| {
        build_depth_bind_group(terrain, device, gpu, parity)
    }));
    terrain.beauty_bind_groups = Some(std::array::from_fn(|parity| {
        build_beauty_bind_group(terrain, device, gpu, parity)
    }));
}

fn build_depth_bind_group(
    terrain: &Terrain,
    device: &wgpu::Device,
    gpu: &SnowGpu,
    parity: usize,
) -> wgpu::BindGroup {
    let deform = &gpu.deform[parity];
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("snow_terrain_depth_textures"),
        layout: &terrain.depth_textures,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&gpu.height.view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&gpu.linear_clamp),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&gpu.aux.view),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::Sampler(&gpu.linear_clamp),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::TextureView(&deform.view),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::Sampler(&gpu.linear_repeat),
            },
        ],
    })
}

fn build_beauty_bind_group(
    terrain: &Terrain,
    device: &wgpu::Device,
    gpu: &SnowGpu,
    parity: usize,
) -> wgpu::BindGroup {
    let deform = &gpu.deform[parity];
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("snow_terrain_textures"),
        layout: &terrain.beauty_textures,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&gpu.height.view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&gpu.linear_clamp),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&gpu.aux.view),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::Sampler(&gpu.linear_clamp),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::TextureView(&gpu.detail.view),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::Sampler(&gpu.linear_mip_repeat),
            },
            wgpu::BindGroupEntry {
                binding: 6,
                resource: wgpu::BindingResource::TextureView(&gpu.sky_lut.view),
            },
            wgpu::BindGroupEntry {
                binding: 7,
                resource: wgpu::BindingResource::Sampler(&gpu.sky_sampler),
            },
            wgpu::BindGroupEntry {
                binding: 8,
                resource: wgpu::BindingResource::TextureView(&deform.view),
            },
            wgpu::BindGroupEntry {
                binding: 9,
                resource: wgpu::BindingResource::Sampler(&gpu.linear_repeat),
            },
            wgpu::BindGroupEntry {
                binding: 10,
                resource: wgpu::BindingResource::TextureView(&gpu.cascades[0].view),
            },
            wgpu::BindGroupEntry {
                binding: 11,
                resource: wgpu::BindingResource::TextureView(&gpu.cascades[1].view),
            },
            wgpu::BindGroupEntry {
                binding: 12,
                resource: wgpu::BindingResource::TextureView(&gpu.cascades[2].view),
            },
            wgpu::BindGroupEntry {
                binding: 13,
                resource: wgpu::BindingResource::Sampler(&gpu.linear_clamp),
            },
        ],
    })
}

fn draw(
    terrain: &Terrain,
    pass: &mut wgpu::RenderPass<'_>,
    pipeline: &wgpu::RenderPipeline,
    uniforms: &UniformSlot,
    textures: &wgpu::BindGroup,
) {
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, &uniforms.bind_group, &[]);
    pass.set_bind_group(1, textures, &[]);
    pass.set_vertex_buffer(0, terrain.mesh.vertices.slice(..));
    pass.set_index_buffer(terrain.mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
    pass.draw_indexed(0..terrain.mesh.index_count, 0, 0..1);
}

pub fn draw_cascade(
    terrain: &Terrain,
    pass: &mut wgpu::RenderPass<'_>,
    cascade: usize,
    deform: usize,
) {
    if !terrain.visible {
        return;
    }
    let Some(textures) = &terrain.depth_bind_groups else {
        return;
    };
    draw(
        terrain,
        pass,
        &terrain.cascade_pipeline,
        &terrain.cascade_uniforms[cascade],
        &textures[deform],
    );
}

pub fn draw_prepass(terrain: &Terrain, pass: &mut wgpu::RenderPass<'_>, deform: usize) {
    if !terrain.visible {
        return;
    }
    let Some(textures) = &terrain.depth_bind_groups else {
        return;
    };
    draw(
        terrain,
        pass,
        &terrain.prepass_pipeline,
        &terrain.prepass_uniforms,
        &textures[deform],
    );
}

pub fn draw_beauty(terrain: &Terrain, pass: &mut wgpu::RenderPass<'_>, deform: usize) {
    if !terrain.visible {
        return;
    }
    let Some(textures) = &terrain.beauty_bind_groups else {
        return;
    };
    draw(
        terrain,
        pass,
        &terrain.beauty_pipeline,
        &terrain.beauty_uniforms,
        &textures[deform],
    );
}

/// Draws issued into the frame: one per cascade, the prepass and the beauty pass.
pub fn draw_calls(terrain: &Terrain) -> u32 {
    if terrain.visible {
        crate::constants::CASCADE_COUNT as u32 + 2
    } else {
        0
    }
}

pub fn triangles(terrain: &Terrain) -> u32 {
    (terrain.mesh.index_count / 3) * draw_calls(terrain)
}
