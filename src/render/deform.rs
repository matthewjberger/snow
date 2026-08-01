use crate::constants::{BRUSH_ROWS, DEFORM_FORMAT, MAX_BRUSHES};
use crate::render::gpu::SnowGpu;
use crate::render::pipelines::{
    fullscreen_pipeline, overwrite_pass, sampler_entry, texture_entry, uniform_entry,
};
use crate::shaders::{self, ShaderLibrary};
use nightshade::prelude::wgpu;

#[repr(C)]
#[derive(Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct DeformUniforms {
    /// (window centre this frame, window centre last frame)
    pub centres: [f32; 4],
    /// (coverage in metres, texels across, seconds of relaxation, brush count)
    pub window: [f32; 4],
    /// (refill rate, maximum depression, maximum berm, wind angle)
    pub relax: [f32; 4],
}

/// The persistent, additive terrain state buffer.
pub struct Deform {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    uniforms: wgpu::Buffer,
    /// One bind group per source parity, so the flip is a pointer swap.
    bind_groups: Option<[wgpu::BindGroup; 2]>,
    targets: Option<[wgpu::TextureView; 2]>,
    /// Which target holds the state the simulation reads this frame.
    pub source: usize,
    /// True until both targets have been written, which is what stands in for the
    /// reference's explicit clear: two passes with the previous centre placed far
    /// outside the window make every texel read as just-scrolled-in, and the shader
    /// answers that by writing zero.
    priming: u32,
}

pub fn new(device: &wgpu::Device, library: &mut ShaderLibrary) -> Deform {
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("snow_deform"),
        entries: &[
            uniform_entry(0),
            texture_entry(1, true),
            sampler_entry(2, true),
            texture_entry(3, true),
        ],
    });
    let module = shaders::compile_fullscreen(library, device, "deform_sim", shaders::DEFORM_SIM);

    Deform {
        pipeline: fullscreen_pipeline(
            device,
            "snow_deform",
            &module,
            &[Some(&layout)],
            DEFORM_FORMAT,
            None,
        ),
        uniforms: device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("snow_deform"),
            size: std::mem::size_of::<DeformUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }),
        layout,
        bind_groups: None,
        targets: None,
        source: 0,
        priming: 2,
    }
}

pub fn bind(deform: &mut Deform, device: &wgpu::Device, gpu: &SnowGpu) {
    deform.bind_groups = Some(std::array::from_fn(|parity| {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("snow_deform"),
            layout: &deform.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: deform.uniforms.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&gpu.deform[parity].view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&gpu.linear_repeat),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&gpu.brush.view),
                },
            ],
        })
    }));
    deform.targets = Some(std::array::from_fn(|parity| {
        gpu.deform[parity].view.clone()
    }));
}

/// Uploads this frame's brushes and simulation parameters.
pub fn write(
    deform: &mut Deform,
    queue: &wgpu::Queue,
    gpu: &SnowGpu,
    uniforms: &DeformUniforms,
    brushes: &[f32],
) {
    let mut uniforms = *uniforms;
    if deform.priming > 0 {
        uniforms.centres[2] = uniforms.centres[0] + 1.0e6;
        uniforms.centres[3] = uniforms.centres[1] + 1.0e6;
        uniforms.window[2] = 0.0;
        uniforms.window[3] = 0.0;
        deform.priming -= 1;
    }
    queue.write_buffer(&deform.uniforms, 0, bytemuck::bytes_of(&uniforms));

    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &gpu.brush.texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        bytemuck::cast_slice(brushes),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(MAX_BRUSHES as u32 * 16),
            rows_per_image: Some(BRUSH_ROWS),
        },
        wgpu::Extent3d {
            width: MAX_BRUSHES as u32,
            height: BRUSH_ROWS,
            depth_or_array_layers: 1,
        },
    );
}

/// Advances the buffer one frame, into whichever target the source left free.
pub fn record(deform: &Deform, encoder: &mut wgpu::CommandEncoder) {
    let (Some(bind_groups), Some(targets)) = (&deform.bind_groups, &deform.targets) else {
        return;
    };
    let target = 1 - deform.source;
    let mut pass = overwrite_pass(encoder, "snow_deform", &targets[target]);
    pass.set_pipeline(&deform.pipeline);
    pass.set_bind_group(0, &bind_groups[deform.source], &[]);
    pass.draw(0..3, 0..1);
}
