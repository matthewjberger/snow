use nightshade::prelude::wgpu;

/// Builds a pipeline whose vertex stage is the shared fullscreen triangle.
pub fn fullscreen_pipeline(
    device: &wgpu::Device,
    label: &str,
    module: &wgpu::ShaderModule,
    layouts: &[Option<&wgpu::BindGroupLayout>],
    format: wgpu::TextureFormat,
    blend: Option<wgpu::BlendState>,
) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(label),
        bind_group_layouts: layouts,
        immediate_size: 0,
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some("fullscreenVertex"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some("fragmentMain"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

/// A bind group layout entry for a uniform buffer visible to both stages.
pub fn uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

/// A bind group layout entry for a sampled two-dimensional float texture.
pub fn texture_entry(binding: u32, filterable: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

pub fn sampler_entry(binding: u32, filtering: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Sampler(if filtering {
            wgpu::SamplerBindingType::Filtering
        } else {
            wgpu::SamplerBindingType::NonFiltering
        }),
        count: None,
    }
}

/// Opens a colour-only render pass that overwrites its target.
pub fn overwrite_pass<'e>(
    encoder: &'e mut wgpu::CommandEncoder,
    label: &str,
    view: &wgpu::TextureView,
) -> wgpu::RenderPass<'e> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(label),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                store: wgpu::StoreOp::Store,
            },
            depth_slice: None,
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    })
}

/// One uniform buffer with the bind group that reaches it.
///
/// Every geometry program in the demo wants the same thing: one uniform block at
/// binding zero, one slot per view it draws from. The block's type differs, so
/// the size comes in rather than the type.
pub struct UniformSlot {
    pub buffer: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
}

pub fn uniform_slot(
    device: &wgpu::Device,
    label: &str,
    layout: &wgpu::BindGroupLayout,
    size: u64,
) -> UniformSlot {
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: buffer.as_entire_binding(),
        }],
    });
    UniformSlot { buffer, bind_group }
}

pub fn write_uniform<T: bytemuck::Pod>(slot: &UniformSlot, queue: &wgpu::Queue, value: &T) {
    queue.write_buffer(&slot.buffer, 0, bytemuck::bytes_of(value));
}

/// What a geometry program varies from every other one.
///
/// The four geometry passes differ only in their vertex layout, their blend and
/// whether they write depth. Everything else, down to the entry point names, is
/// the same, so they share one builder rather than four copies that drift.
pub struct GeometrySpec<'a> {
    pub label: &'a str,
    pub module: &'a wgpu::ShaderModule,
    pub layouts: &'a [Option<&'a wgpu::BindGroupLayout>],
    pub vertices: wgpu::VertexBufferLayout<'a>,
    pub color: wgpu::TextureFormat,
    pub blend: Option<wgpu::BlendState>,
    /// `None` draws with no depth attachment at all, which is what the cascade
    /// programs that render straight into a depth target want.
    pub depth: Option<wgpu::TextureFormat>,
}

pub fn geometry_pipeline(device: &wgpu::Device, spec: GeometrySpec<'_>) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(spec.label),
        bind_group_layouts: spec.layouts,
        immediate_size: 0,
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(spec.label),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: spec.module,
            entry_point: Some("vertexMain"),
            buffers: &[spec.vertices],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: spec.module,
            entry_point: Some("fragmentMain"),
            targets: &[Some(wgpu::ColorTargetState {
                format: spec.color,
                blend: spec.blend,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        // Both faces everywhere: the far facets of a transparent prism are what
        // carry the refraction, and the generated hexagons and lattices have no
        // dependable winding to cull on.
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: spec.depth.map(|format| wgpu::DepthStencilState {
            format,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}
