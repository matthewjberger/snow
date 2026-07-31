use crate::constants::*;
use crate::render::gpu as gpu_state;
use crate::render::gpu::{SnowGpu, SnowTexture};
use crate::render::pipelines::{
    fullscreen_pipeline, overwrite_pass, sampler_entry, texture_entry, uniform_entry,
};
use crate::shaders::{self, ShaderLibrary};
use nalgebra_glm::Vec3;
use nightshade::prelude::wgpu;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct HeightBakeUniforms {
    /// (origin x, origin z, world size, wind angle)
    world: [f32; 4],
    /// (height amplitude, 0, 0, 0)
    params: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct AuxBakeUniforms {
    /// (world metres per height texel, one over the height resolution, 0, 0)
    params: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct DetailBakeUniforms {
    /// (resolution, grain scale, 0, 0)
    params: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct SkyBakeUniforms {
    /// (sun direction, sun intensity)
    sun: [f32; 4],
    /// (ground bounce radiance, 0)
    bounce: [f32; 4],
}

/// What the sky integral is being asked to solve for on this pass.
pub struct SkySample {
    /// Unit vector pointing toward the sun.
    pub direction: Vec3,
    /// The shared radiometric scale the sun and the baked sky both sit on.
    pub intensity: f32,
    /// Radiance leaving the snow field, which the integral hands back below the horizon
    /// and which the bounce solve refines between passes.
    pub ground_bounce: Vec3,
}

/// The load-time bakes and the mip reduction they share.
pub struct Bakes {
    height_pipeline: wgpu::RenderPipeline,
    height_uniforms: wgpu::Buffer,
    height_bind_group: wgpu::BindGroup,

    aux_pipeline: wgpu::RenderPipeline,
    aux_uniforms: wgpu::Buffer,
    aux_bind_group: wgpu::BindGroup,

    detail_pipeline: wgpu::RenderPipeline,
    detail_uniforms: wgpu::Buffer,
    detail_bind_group: wgpu::BindGroup,

    sky_lut_pipeline: wgpu::RenderPipeline,
    sky_sh_pipeline: wgpu::RenderPipeline,
    sky_uniforms: wgpu::Buffer,
    sky_bind_group: wgpu::BindGroup,

    mip_layout: wgpu::BindGroupLayout,
    detail_mip_pipeline: wgpu::RenderPipeline,
    sky_mip_pipeline: wgpu::RenderPipeline,
}

pub fn new(device: &wgpu::Device, library: &mut ShaderLibrary, gpu: &SnowGpu) -> Bakes {
    let uniform_only = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("snow_bake_uniform"),
        entries: &[uniform_entry(0)],
    });
    let uniform_and_texture = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("snow_bake_uniform_texture"),
        entries: &[
            uniform_entry(0),
            texture_entry(1, true),
            sampler_entry(2, true),
        ],
    });
    let mip_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("snow_mip"),
        entries: &[texture_entry(0, true), sampler_entry(1, true)],
    });

    let height_module =
        shaders::compile_fullscreen(library, device, "height_bake", shaders::HEIGHT_BAKE);
    let aux_module = shaders::compile_fullscreen(library, device, "aux_bake", shaders::AUX_BAKE);
    let detail_module =
        shaders::compile_fullscreen(library, device, "detail_bake", shaders::DETAIL_BAKE);
    let sky_module = shaders::compile_fullscreen(library, device, "sky_bake", shaders::SKY_BAKE);
    let mip_module = shaders::compile_fullscreen(library, device, "mip", shaders::MIP);

    let height_uniforms = uniform_buffer::<HeightBakeUniforms>(device, "snow_height_bake");
    let aux_uniforms = uniform_buffer::<AuxBakeUniforms>(device, "snow_aux_bake");
    let detail_uniforms = uniform_buffer::<DetailBakeUniforms>(device, "snow_detail_bake");
    let sky_uniforms = uniform_buffer::<SkyBakeUniforms>(device, "snow_sky_bake");

    Bakes {
        height_pipeline: fullscreen_pipeline(
            device,
            "snow_height_bake",
            &height_module,
            &[Some(&uniform_only)],
            HEIGHT_FORMAT,
            None,
        ),
        height_bind_group: device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("snow_height_bake"),
            layout: &uniform_only,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: height_uniforms.as_entire_binding(),
            }],
        }),
        height_uniforms,

        aux_pipeline: fullscreen_pipeline(
            device,
            "snow_aux_bake",
            &aux_module,
            &[Some(&uniform_and_texture)],
            AUX_FORMAT,
            None,
        ),
        aux_bind_group: device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("snow_aux_bake"),
            layout: &uniform_and_texture,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: aux_uniforms.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&gpu.height.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&gpu.linear_clamp),
                },
            ],
        }),
        aux_uniforms,

        detail_pipeline: fullscreen_pipeline(
            device,
            "snow_detail_bake",
            &detail_module,
            &[Some(&uniform_only)],
            DETAIL_FORMAT,
            None,
        ),
        detail_bind_group: device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("snow_detail_bake"),
            layout: &uniform_only,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: detail_uniforms.as_entire_binding(),
            }],
        }),
        detail_uniforms,

        sky_lut_pipeline: fullscreen_pipeline(
            device,
            "snow_sky_lut_bake",
            &sky_module,
            &[Some(&uniform_only)],
            SKY_LUT_FORMAT,
            None,
        ),
        sky_sh_pipeline: fullscreen_pipeline(
            device,
            "snow_sky_sh_bake",
            &sky_module,
            &[Some(&uniform_only)],
            SKY_SH_FORMAT,
            None,
        ),
        sky_bind_group: device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("snow_sky_bake"),
            layout: &uniform_only,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: sky_uniforms.as_entire_binding(),
            }],
        }),
        sky_uniforms,

        detail_mip_pipeline: fullscreen_pipeline(
            device,
            "snow_detail_mip",
            &mip_module,
            &[Some(&mip_layout)],
            DETAIL_FORMAT,
            None,
        ),
        sky_mip_pipeline: fullscreen_pipeline(
            device,
            "snow_sky_mip",
            &mip_module,
            &[Some(&mip_layout)],
            SKY_LUT_FORMAT,
            None,
        ),
        mip_layout,
    }
}

/// The bakes that only depend on settings fixed at load: the macro heightfield,
/// everything derived from it, and the tiling grain map.
pub fn run_static(
    bakes: &Bakes,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    encoder: &mut wgpu::CommandEncoder,
    gpu: &SnowGpu,
    wind_angle: f32,
    height_amplitude: f32,
) {
    queue.write_buffer(
        &bakes.height_uniforms,
        0,
        bytemuck::bytes_of(&HeightBakeUniforms {
            world: [-WORLD_SIZE * 0.5, -WORLD_SIZE * 0.5, WORLD_SIZE, wind_angle],
            params: [height_amplitude, 0.0, 0.0, 0.0],
        }),
    );
    {
        let mut pass = overwrite_pass(encoder, "snow_height_bake", &gpu.height.view);
        pass.set_pipeline(&bakes.height_pipeline);
        pass.set_bind_group(0, &bakes.height_bind_group, &[]);
        pass.draw(0..3, 0..1);
    }

    queue.write_buffer(
        &bakes.aux_uniforms,
        0,
        bytemuck::bytes_of(&AuxBakeUniforms {
            params: [
                WORLD_SIZE / HEIGHT_RES as f32,
                1.0 / HEIGHT_RES as f32,
                0.0,
                0.0,
            ],
        }),
    );
    {
        let mut pass = overwrite_pass(encoder, "snow_aux_bake", &gpu.aux.view);
        pass.set_pipeline(&bakes.aux_pipeline);
        pass.set_bind_group(0, &bakes.aux_bind_group, &[]);
        pass.draw(0..3, 0..1);
    }

    queue.write_buffer(
        &bakes.detail_uniforms,
        0,
        bytemuck::bytes_of(&DetailBakeUniforms {
            params: [DETAIL_RES as f32, 0.013, 0.0, 0.0],
        }),
    );
    {
        let mut pass = overwrite_pass(
            encoder,
            "snow_detail_bake",
            &gpu_state::mip_view(&gpu.detail, 0),
        );
        pass.set_pipeline(&bakes.detail_pipeline);
        pass.set_bind_group(0, &bakes.detail_bind_group, &[]);
        pass.draw(0..3, 0..1);
    }

    generate_mips(
        bakes,
        device,
        encoder,
        &gpu.detail,
        &gpu.linear_clamp,
        &bakes.detail_mip_pipeline,
    );
}

/// Re-integrates the sky.
pub fn run_sky(
    bakes: &Bakes,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    encoder: &mut wgpu::CommandEncoder,
    gpu: &SnowGpu,
    sample: &SkySample,
) {
    queue.write_buffer(
        &bakes.sky_uniforms,
        0,
        bytemuck::bytes_of(&SkyBakeUniforms {
            sun: [
                sample.direction.x,
                sample.direction.y,
                sample.direction.z,
                sample.intensity,
            ],
            bounce: [
                sample.ground_bounce.x,
                sample.ground_bounce.y,
                sample.ground_bounce.z,
                0.0,
            ],
        }),
    );

    {
        let mut pass = overwrite_pass(
            encoder,
            "snow_sky_lut_bake",
            &gpu_state::mip_view(&gpu.sky_lut, 0),
        );
        pass.set_pipeline(&bakes.sky_lut_pipeline);
        pass.set_bind_group(0, &bakes.sky_bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
    {
        let mut pass = overwrite_pass(encoder, "snow_sky_sh_bake", &gpu.sky_sh.view);
        pass.set_pipeline(&bakes.sky_sh_pipeline);
        pass.set_bind_group(0, &bakes.sky_bind_group, &[]);
        pass.draw(0..3, 0..1);
    }

    generate_mips(
        bakes,
        device,
        encoder,
        &gpu.sky_lut,
        &gpu.linear_clamp,
        &bakes.sky_mip_pipeline,
    );
}

fn generate_mips(
    bakes: &Bakes,
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    texture: &SnowTexture,
    sampler: &wgpu::Sampler,
    pipeline: &wgpu::RenderPipeline,
) {
    for level in 1..texture.mip_levels {
        let source = gpu_state::mip_view(texture, level - 1);
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("snow_mip"),
            layout: &bakes.mip_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&source),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });
        let target = gpu_state::mip_view(texture, level);
        let mut pass = overwrite_pass(encoder, "snow_mip", &target);
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

fn uniform_buffer<T: bytemuck::Pod>(device: &wgpu::Device, label: &str) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: std::mem::size_of::<T>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}
