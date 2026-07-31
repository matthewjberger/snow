use crate::constants::DEPTH_FORMAT;
use crate::render::geometry::{PACKED_LAYOUT, StaticMesh, build_skybox};
use crate::render::gpu::SnowGpu;
use crate::render::pipelines::{sampler_entry, texture_entry, uniform_entry};
use nightshade::prelude::wgpu;

/// The sky material's uniform block, mirroring `SkyUniforms` in `sky.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SkyUniforms {
    pub view_projection: [f32; 16],
    /// (camera position, half the far plane)
    pub camera: [f32; 4],
    /// (direction toward the sun, the shared radiometric scale)
    pub sun: [f32; 4],
    /// (normalised sun hue, ambient intensity)
    pub sun_color: [f32; 4],
    /// (direct solar irradiance at the ground, peak height of the far range)
    pub sun_radiance: [f32; 4],
    /// (seconds, cloud amount, wind direction xz)
    pub weather: [f32; 4],
    /// (fog density, height falloff, fog start, aerial strength)
    pub fog: [f32; 4],
    pub harmonics: [[f32; 4]; 9],
}

impl Default for SkyUniforms {
    fn default() -> Self {
        let mut view_projection = [0.0; 16];
        view_projection[0] = 1.0;
        view_projection[5] = 1.0;
        view_projection[10] = 1.0;
        view_projection[15] = 1.0;
        Self {
            view_projection,
            camera: [0.0; 4],
            sun: [0.0, 1.0, 0.0, 0.0],
            sun_color: [1.0, 1.0, 1.0, 1.0],
            sun_radiance: [0.0; 4],
            weather: [0.0; 4],
            fog: [0.0; 4],
            harmonics: [[0.0; 4]; 9],
        }
    }
}

/// The skybox and the far range it raymarches.
pub struct Sky {
    mesh: StaticMesh,
    pipeline: wgpu::RenderPipeline,
    texture_layout: wgpu::BindGroupLayout,
    uniforms: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    texture_bind_group: Option<wgpu::BindGroup>,
}

pub fn new(
    device: &wgpu::Device,
    module: &wgpu::ShaderModule,
    color_format: wgpu::TextureFormat,
) -> Sky {
    let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("snow_sky_uniforms"),
        entries: &[uniform_entry(0)],
    });
    let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("snow_sky_textures"),
        entries: &[texture_entry(0, true), sampler_entry(1, true)],
    });

    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("snow_sky"),
        bind_group_layouts: &[Some(&uniform_layout), Some(&texture_layout)],
        immediate_size: 0,
    });

    let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("snow_sky"),
        size: std::mem::size_of::<SkyUniforms>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    Sky {
        mesh: build_skybox(device),
        pipeline: device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("snow_sky"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module,
                entry_point: Some("vertexMain"),
                buffers: &[PACKED_LAYOUT],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module,
                entry_point: Some("fragmentMain"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        }),
        uniform_bind_group: device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("snow_sky_uniforms"),
            layout: &uniform_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            }],
        }),
        uniforms,
        texture_layout,
        texture_bind_group: None,
    }
}

pub fn bind(sky: &mut Sky, device: &wgpu::Device, gpu: &SnowGpu) {
    sky.texture_bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("snow_sky_textures"),
        layout: &sky.texture_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&gpu.sky_lut.view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&gpu.sky_sampler),
            },
        ],
    }));
}

pub fn write(sky: &Sky, queue: &wgpu::Queue, uniforms: &SkyUniforms) {
    queue.write_buffer(&sky.uniforms, 0, bytemuck::bytes_of(uniforms));
}

pub fn draw(sky: &Sky, pass: &mut wgpu::RenderPass<'_>) {
    let Some(textures) = &sky.texture_bind_group else {
        return;
    };
    pass.set_pipeline(&sky.pipeline);
    pass.set_bind_group(0, &sky.uniform_bind_group, &[]);
    pass.set_bind_group(1, textures, &[]);
    pass.set_vertex_buffer(0, sky.mesh.vertices.slice(..));
    pass.set_index_buffer(sky.mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
    pass.draw_indexed(0..sky.mesh.index_count, 0, 0..1);
}
