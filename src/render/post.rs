use crate::constants::HDR_FORMAT;
use crate::render::gpu::SnowGpu;
use crate::render::pipelines::{
    fullscreen_pipeline, overwrite_pass, sampler_entry, texture_entry, uniform_entry,
};
use crate::shaders::{self, ShaderLibrary};
use nightshade::prelude::*;
use nightshade::render::wgpu::rendergraph::{Result, SubGraphRunCommand};

/// Everything the screen-space chain derives from the camera and the settings,
/// mirroring `PostUniforms` in `snow::post_uniforms` field for field.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PostUniforms {
    pub previous_view_projection: [f32; 16],
    pub inverse_view: [f32; 16],
    pub projection: [f32; 4],
    /// (subpixel offset in normalised device coordinates, history validity, feedback at
    /// rest)
    pub temporal: [f32; 4],
    /// (where the sun lands on screen, whether it is in front, aspect ratio)
    pub sun: [f32; 4],
    /// Sun radiance, with the shaft strength in w.
    pub sun_color: [f32; 4],
    /// (exposure, contrast, display transform, grain amount)
    pub tone: [f32; 4],
    /// (seconds, vignette, speed streak, bloom amount)
    pub look: [f32; 4],
    /// (focal distance, largest circle of confusion, depth of field on, shaft amount)
    pub focus: [f32; 4],
    /// (reflections on, temporal resolve on, sharpen amount, reflection strength)
    pub toggles: [f32; 4],
}

impl Default for PostUniforms {
    fn default() -> Self {
        let mut identity = [0.0_f32; 16];
        identity[0] = 1.0;
        identity[5] = 1.0;
        identity[10] = 1.0;
        identity[15] = 1.0;
        Self {
            previous_view_projection: identity,
            inverse_view: identity,
            projection: [1.0, 1.0, 1.0, 1.0],
            temporal: [0.0, 0.0, 0.0, 0.90],
            sun: [0.5, 0.5, 0.0, 1.0],
            sun_color: [1.0, 1.0, 1.0, 1.0],
            tone: [1.0, 1.0, 0.0, 0.0],
            look: [0.0, 0.22, 0.0, 0.0],
            focus: [6.2, 3.0, 0.0, 0.0],
            toggles: [0.0, 0.0, 0.0, 1.0],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
struct BloomUniforms {
    /// (one texel of the source in uv times the spread, the threshold switch, 0)
    source: [f32; 4],
    curve: [f32; 4],
}

/// A render target the chain owns, with its size, so the stages that need one texel of
/// their source can ask for it.
struct Target {
    view: wgpu::TextureView,
    width: u32,
    height: u32,
}

fn target(device: &wgpu::Device, label: &str, width: u32, height: u32) -> Target {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: HDR_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    Target {
        view: texture.create_view(&wgpu::TextureViewDescriptor::default()),
        width: width.max(1),
        height: height.max(1),
    }
}

/// One texel of this target, in uv.
fn target_texel(target: &Target) -> [f32; 2] {
    [1.0 / target.width as f32, 1.0 / target.height as f32]
}

/// One stage of the chain: its program, and the input bindings it draws with.
struct Stage {
    pipeline: wgpu::RenderPipeline,
    texture_layout: wgpu::BindGroupLayout,
    bind_groups: Vec<wgpu::BindGroup>,
}

fn stage(
    device: &wgpu::Device,
    library: &mut ShaderLibrary,
    label: &str,
    source: &str,
    uniform_layout: &wgpu::BindGroupLayout,
    textures: usize,
    format: wgpu::TextureFormat,
) -> Stage {
    let mut entries: Vec<wgpu::BindGroupLayoutEntry> = (0..textures)
        .map(|binding| texture_entry(binding as u32, true))
        .collect();
    entries.push(sampler_entry(textures as u32, true));

    let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(label),
        entries: &entries,
    });
    let module = shaders::compile_fullscreen(library, device, label, source);
    Stage {
        pipeline: fullscreen_pipeline(
            device,
            label,
            &module,
            &[Some(uniform_layout), Some(&texture_layout)],
            format,
            None,
        ),
        texture_layout,
        bind_groups: Vec::new(),
    }
}

fn bind_stage(
    stage: &mut Stage,
    device: &wgpu::Device,
    label: &str,
    views: &[&wgpu::TextureView],
    sampler: &wgpu::Sampler,
) {
    let mut entries: Vec<wgpu::BindGroupEntry> = views
        .iter()
        .enumerate()
        .map(|(binding, view)| wgpu::BindGroupEntry {
            binding: binding as u32,
            resource: wgpu::BindingResource::TextureView(view),
        })
        .collect();
    entries.push(wgpu::BindGroupEntry {
        binding: views.len() as u32,
        resource: wgpu::BindingResource::Sampler(sampler),
    });
    stage
        .bind_groups
        .push(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(label),
            layout: &stage.texture_layout,
            entries: &entries,
        }));
}

fn draw_stage(
    stage: &Stage,
    encoder: &mut wgpu::CommandEncoder,
    label: &str,
    target: &wgpu::TextureView,
    uniforms: &wgpu::BindGroup,
    binding: usize,
) {
    let Some(textures) = stage.bind_groups.get(binding) else {
        return;
    };
    let mut pass = overwrite_pass(encoder, label, target);
    pass.set_pipeline(&stage.pipeline);
    pass.set_bind_group(0, uniforms, &[]);
    pass.set_bind_group(1, textures, &[]);
    pass.draw(0..3, 0..1);
}

/// The screen-space chain, in one pass node.
pub struct SnowPostPass {
    uniforms: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    bloom_uniforms: [wgpu::Buffer; 3],
    bloom_bind_groups: [wgpu::BindGroup; 3],
    sampler: wgpu::Sampler,

    ssr: Stage,
    taa: Stage,
    shafts: Stage,
    bloom_down: Stage,
    bloom_down_far: Stage,
    bloom_blur: Stage,
    depth_of_field: Stage,
    tonemap: Stage,
    sharpen: Stage,

    reflected: Target,
    history: [Target; 2],
    shafts_target: Target,
    bloom: [Target; 3],
    defocused: Target,
    composed: Target,

    /// Which history slot this frame resolves into.
    parity: usize,
    /// Set once the bind groups have been built against the current inputs.
    bound: bool,
    /// The scene and prepass views the chain reads, cloned at bind time.
    inputs: Option<(wgpu::TextureView, wgpu::TextureView)>,
}

pub fn new(
    device: &wgpu::Device,
    library: &mut ShaderLibrary,
    surface_format: wgpu::TextureFormat,
    width: u32,
    height: u32,
) -> SnowPostPass {
    let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("snow_post_uniforms"),
        entries: &[uniform_entry(0)],
    });
    let bloom_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("snow_post_bloom_uniforms"),
        entries: &[uniform_entry(0)],
    });

    let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("snow_post_uniforms"),
        size: std::mem::size_of::<PostUniforms>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("snow_post_uniforms"),
        layout: &uniform_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: uniforms.as_entire_binding(),
        }],
    });

    let bloom_uniforms: [wgpu::Buffer; 3] = std::array::from_fn(|_| {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("snow_post_bloom_uniforms"),
            size: std::mem::size_of::<BloomUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    });
    let bloom_bind_groups: [wgpu::BindGroup; 3] = std::array::from_fn(|level| {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("snow_post_bloom_uniforms"),
            layout: &bloom_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: bloom_uniforms[level].as_entire_binding(),
            }],
        })
    });

    let quarter = (width.div_ceil(4).max(1), height.div_ceil(4).max(1));
    let sixteenth = (width.div_ceil(16).max(1), height.div_ceil(16).max(1));

    SnowPostPass {
        ssr: stage(
            device,
            library,
            "post_ssr",
            shaders::POST_SSR,
            &uniform_layout,
            2,
            HDR_FORMAT,
        ),
        taa: stage(
            device,
            library,
            "post_taa",
            shaders::POST_TAA,
            &uniform_layout,
            3,
            HDR_FORMAT,
        ),
        shafts: stage(
            device,
            library,
            "post_shafts",
            shaders::POST_SHAFTS,
            &uniform_layout,
            1,
            HDR_FORMAT,
        ),
        bloom_down: stage(
            device,
            library,
            "post_bloom_down",
            shaders::POST_BLOOM_DOWN,
            &bloom_layout,
            1,
            HDR_FORMAT,
        ),
        bloom_down_far: stage(
            device,
            library,
            "post_bloom_down_far",
            shaders::POST_BLOOM_DOWN,
            &bloom_layout,
            1,
            HDR_FORMAT,
        ),
        bloom_blur: stage(
            device,
            library,
            "post_bloom_blur",
            shaders::POST_BLOOM_BLUR,
            &bloom_layout,
            1,
            HDR_FORMAT,
        ),
        depth_of_field: stage(
            device,
            library,
            "post_dof",
            shaders::POST_DOF,
            &uniform_layout,
            2,
            HDR_FORMAT,
        ),
        tonemap: stage(
            device,
            library,
            "post_tonemap",
            shaders::POST_TONEMAP,
            &uniform_layout,
            4,
            HDR_FORMAT,
        ),
        sharpen: stage(
            device,
            library,
            "post_sharpen",
            shaders::POST_SHARPEN,
            &uniform_layout,
            1,
            surface_format,
        ),

        reflected: target(device, "snow_post_reflected", width, height),
        history: std::array::from_fn(|slot| {
            target(
                device,
                if slot == 0 {
                    "snow_post_history_0"
                } else {
                    "snow_post_history_1"
                },
                width,
                height,
            )
        }),
        shafts_target: target(device, "snow_post_shafts", quarter.0, quarter.1),
        bloom: [
            target(device, "snow_post_bloom_near", quarter.0, quarter.1),
            target(device, "snow_post_bloom_mid", sixteenth.0, sixteenth.1),
            target(device, "snow_post_bloom_far", sixteenth.0, sixteenth.1),
        ],
        defocused: target(device, "snow_post_defocused", width, height),
        composed: target(device, "snow_post_composed", width, height),

        sampler: device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("snow_post"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        }),

        uniforms,
        uniform_bind_group,
        bloom_uniforms,
        bloom_bind_groups,
        parity: 0,
        bound: false,
        inputs: None,
    }
}

/// Points the chain at the scene the world pass drew.
pub fn bind(post: &mut SnowPostPass, gpu: &SnowGpu) {
    post.inputs = Some((gpu.scene.view.clone(), gpu.prepass.view.clone()));
    post.bound = false;
}

/// Rebuilds every target the chain owns at a new internal resolution.
///
/// The chain's targets are its own rather than the graph's, so a surface resize
/// or a change to the resolution slider has to reach them here. Dropping the
/// bind groups is what makes the next frame rebind against the new views; the
/// history is uninitialised again afterwards, which the caller signals by
/// resetting `history_valid`.
pub fn resize(post: &mut SnowPostPass, device: &wgpu::Device, width: u32, height: u32) {
    let width = width.max(1);
    let height = height.max(1);
    let quarter = (width.div_ceil(4).max(1), height.div_ceil(4).max(1));
    let sixteenth = (width.div_ceil(16).max(1), height.div_ceil(16).max(1));

    post.reflected = target(device, "snow_post_reflected", width, height);
    post.history = std::array::from_fn(|slot| {
        target(
            device,
            if slot == 0 {
                "snow_post_history_0"
            } else {
                "snow_post_history_1"
            },
            width,
            height,
        )
    });
    post.shafts_target = target(device, "snow_post_shafts", quarter.0, quarter.1);
    post.bloom = [
        target(device, "snow_post_bloom_near", quarter.0, quarter.1),
        target(device, "snow_post_bloom_mid", sixteenth.0, sixteenth.1),
        target(device, "snow_post_bloom_far", sixteenth.0, sixteenth.1),
    ];
    post.defocused = target(device, "snow_post_defocused", width, height);
    post.composed = target(device, "snow_post_composed", width, height);

    post.bound = false;
    post.ssr.bind_groups.clear();
    post.taa.bind_groups.clear();
    post.shafts.bind_groups.clear();
    post.bloom_down.bind_groups.clear();
    post.bloom_down_far.bind_groups.clear();
    post.bloom_blur.bind_groups.clear();
    post.depth_of_field.bind_groups.clear();
    post.tonemap.bind_groups.clear();
    post.sharpen.bind_groups.clear();
}

/// The history slot this frame's resolve lands in, which everything downstream of
/// the resolve reads.
fn resolved(post: &SnowPostPass) -> &Target {
    &post.history[post.parity]
}

pub fn write(post: &mut SnowPostPass, queue: &wgpu::Queue, uniforms: &PostUniforms) {
    queue.write_buffer(&post.uniforms, 0, bytemuck::bytes_of(uniforms));

    let threshold = 3.0_f32;
    let knee = 1.4_f32;
    let curve = [
        threshold,
        threshold - knee,
        knee * 2.0,
        0.25 / knee.max(1e-4),
    ];

    let full = target_texel(&post.history[post.parity]);
    let levels = [
        BloomUniforms {
            source: [full[0] * 2.0, full[1] * 2.0, 1.0, 0.0],
            curve,
        },
        BloomUniforms {
            source: [
                target_texel(&post.bloom[0])[0] * 2.0,
                target_texel(&post.bloom[0])[1] * 2.0,
                0.0,
                0.0,
            ],
            curve: [0.0; 4],
        },
        BloomUniforms {
            source: [
                target_texel(&post.bloom[1])[0] * 2.0,
                target_texel(&post.bloom[1])[1] * 2.0,
                0.0,
                0.0,
            ],
            curve: [0.0; 4],
        },
    ];
    for (buffer, level) in post.bloom_uniforms.iter().zip(levels) {
        queue.write_buffer(buffer, 0, bytemuck::bytes_of(&level));
    }
}

/// Advances the history ping-pong.
pub fn advance(post: &mut SnowPostPass) {
    post.parity = 1 - post.parity;
}

impl PassNode<RenderInputs> for SnowPostPass {
    fn name(&self) -> &str {
        "snow_post"
    }

    fn reads(&self) -> Vec<&str> {
        Vec::new()
    }

    fn writes(&self) -> Vec<&str> {
        vec!["output"]
    }

    fn invalidate_bind_groups(&mut self) {
        self.bound = false;
        self.ssr.bind_groups.clear();
        self.taa.bind_groups.clear();
        self.shafts.bind_groups.clear();
        self.bloom_down.bind_groups.clear();
        self.bloom_down_far.bind_groups.clear();
        self.bloom_blur.bind_groups.clear();
        self.depth_of_field.bind_groups.clear();
        self.tonemap.bind_groups.clear();
        self.sharpen.bind_groups.clear();
    }

    fn execute<'r, 'e>(
        &mut self,
        context: PassExecutionContext<'r, 'e, RenderInputs>,
    ) -> Result<Vec<SubGraphRunCommand<'r>>> {
        let Some((scene, prepass)) = self.inputs.clone() else {
            return Ok(Vec::new());
        };

        if !self.bound {
            let device = context.device;
            let sampler = self.sampler.clone();
            bind_stage(
                &mut self.ssr,
                device,
                "post_ssr",
                &[&scene, &prepass],
                &sampler,
            );
            bind_stage(
                &mut self.shafts,
                device,
                "post_shafts",
                &[&prepass],
                &sampler,
            );
            bind_stage(
                &mut self.bloom_down_far,
                device,
                "post_bloom_down_far",
                &[&self.bloom[0].view],
                &sampler,
            );
            bind_stage(
                &mut self.bloom_blur,
                device,
                "post_bloom_blur",
                &[&self.bloom[1].view],
                &sampler,
            );
            bind_stage(
                &mut self.tonemap,
                device,
                "post_tonemap",
                &[
                    &self.defocused.view,
                    &self.bloom[0].view,
                    &self.bloom[2].view,
                    &self.shafts_target.view,
                ],
                &sampler,
            );
            bind_stage(
                &mut self.sharpen,
                device,
                "post_sharpen",
                &[&self.composed.view],
                &sampler,
            );

            for parity in 0..2 {
                bind_stage(
                    &mut self.taa,
                    device,
                    "post_taa",
                    &[
                        &self.reflected.view,
                        &self.history[1 - parity].view,
                        &prepass,
                    ],
                    &sampler,
                );
                bind_stage(
                    &mut self.bloom_down,
                    device,
                    "post_bloom_down",
                    &[&self.history[parity].view],
                    &sampler,
                );
                bind_stage(
                    &mut self.depth_of_field,
                    device,
                    "post_dof",
                    &[&self.history[parity].view, &prepass],
                    &sampler,
                );
            }
            self.bound = true;
        }

        let (output, store) = {
            let (view, _, store) = context.get_color_attachment("output")?;
            (view.clone(), store)
        };

        let encoder = context.encoder;
        let uniforms = &self.uniform_bind_group;
        let parity = self.parity;

        draw_stage(
            &self.ssr,
            encoder,
            "snow_post_ssr",
            &self.reflected.view,
            uniforms,
            0,
        );
        draw_stage(
            &self.taa,
            encoder,
            "snow_post_taa",
            &resolved(self).view,
            uniforms,
            parity,
        );
        draw_stage(
            &self.shafts,
            encoder,
            "snow_post_shafts",
            &self.shafts_target.view,
            uniforms,
            0,
        );
        draw_stage(
            &self.bloom_down,
            encoder,
            "snow_post_bloom_near",
            &self.bloom[0].view,
            &self.bloom_bind_groups[0],
            parity,
        );
        draw_stage(
            &self.bloom_down_far,
            encoder,
            "snow_post_bloom_mid",
            &self.bloom[1].view,
            &self.bloom_bind_groups[1],
            0,
        );
        draw_stage(
            &self.bloom_blur,
            encoder,
            "snow_post_bloom_far",
            &self.bloom[2].view,
            &self.bloom_bind_groups[2],
            0,
        );
        draw_stage(
            &self.depth_of_field,
            encoder,
            "snow_post_dof",
            &self.defocused.view,
            uniforms,
            parity,
        );
        draw_stage(
            &self.tonemap,
            encoder,
            "snow_post_tonemap",
            &self.composed.view,
            uniforms,
            0,
        );

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("snow_post_sharpen"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &output,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        if let Some(textures) = self.sharpen.bind_groups.first() {
            pass.set_pipeline(&self.sharpen.pipeline);
            pass.set_bind_group(0, uniforms, &[]);
            pass.set_bind_group(1, textures, &[]);
            pass.draw(0..3, 0..1);
        }
        Ok(Vec::new())
    }
}
