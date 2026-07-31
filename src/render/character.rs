use crate::constants::{CASCADE_COUNT, CASCADE_FORMAT, DEPTH_FORMAT, HDR_FORMAT};
use crate::render::character_geometry::{SKINNED_LAYOUT, build_body, build_fur};
use crate::render::cloth_geometry::{CLOTH_LAYOUT, build_cloth};
use crate::render::geometry::StaticMesh;
use crate::render::gpu::SnowGpu;
use crate::render::pipelines::{GeometrySpec, UniformSlot, geometry_pipeline, uniform_slot};
use crate::render::pipelines::{sampler_entry, texture_entry, uniform_entry};
use crate::shaders::{self, ShaderLibrary};
use crate::systems::cloth::Cloth;
use nightshade::prelude::wgpu;

/// The character's uniform block, mirroring `CharUniforms` in `snow::char_uniforms`
/// field for field.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CharUniforms {
    pub view_projection: [f32; 16],
    pub camera: [f32; 4],
    pub sun_direction: [f32; 4],
    pub sun_radiance: [f32; 4],
    pub fog: [f32; 4],
    /// (ambient intensity, subsurface strength, weave threads per metre, 0)
    pub misc: [f32; 4],
    /// (world displacement applied to a strand tip, strand cells per metre)
    pub fur: [f32; 4],
    pub fur_color: [f32; 4],
    pub screen: [f32; 4],
    pub harmonics: [[f32; 4]; 9],
    pub material_albedo: [[f32; 4]; 8],
    pub material_params: [[f32; 4]; 8],
    pub panels: [[f32; 4]; 6],
    pub shadow: crate::render::uniforms::ShadowUniforms,
    pub lights: crate::render::uniforms::SpellLightUniforms,
}

impl Default for CharUniforms {
    fn default() -> Self {
        let mut view_projection = [0.0; 16];
        view_projection[0] = 1.0;
        view_projection[5] = 1.0;
        view_projection[10] = 1.0;
        view_projection[15] = 1.0;
        Self {
            view_projection,
            camera: [0.0; 4],
            sun_direction: [0.0, 1.0, 0.0, 0.0],
            sun_radiance: [0.0; 4],
            fog: [0.0; 4],
            misc: [1.0, 1.0, 210.0, 0.0],
            fur: [0.0, 0.0, 0.0, 250.0],
            fur_color: [0.74, 0.755, 0.795, 0.0],
            screen: [1.0, 1.0, 0.0, 0.0],
            harmonics: [[0.0; 4]; 9],
            material_albedo: PALETTE,
            material_params: PARAMS,
            panels: [[0.0; 4]; 6],
            shadow: crate::render::uniforms::ShadowUniforms::default(),
            lights: crate::render::uniforms::SpellLightUniforms::default(),
        }
    }
}

/// Material palette: rgb albedo with the base roughness in w.
const PALETTE: [[f32; 4]; 8] = [
    [0.030, 0.048, 0.125, 0.80],
    [0.075, 0.105, 0.185, 0.74],
    [0.230, 0.225, 0.205, 0.82],
    [0.048, 0.033, 0.024, 0.60],
    [0.135, 0.095, 0.072, 0.85],
    [0.120, 0.195, 0.310, 0.70],
    [0.700, 0.720, 0.760, 0.85],
    [0.100, 0.100, 0.100, 0.80],
];

/// Sheen, anisotropy, transmission and weave depth per slot.
const PARAMS: [[f32; 4]; 8] = [
    [0.22, 0.55, 0.05, 1.00],
    [0.28, 0.45, 0.07, 0.90],
    [0.35, 0.30, 0.22, 1.10],
    [0.06, 0.20, 0.01, 0.35],
    [0.05, 0.00, 0.08, 0.00],
    [0.25, 0.60, 0.12, 1.00],
    [1.00, 0.00, 0.90, 0.00],
    [0.20, 0.00, 0.00, 0.50],
];

/// How many cascades the figure casts into.
const CHARACTER_CASCADES: usize = 2;

/// The figure: its skeleton-driven meshes and the four programs that draw them.
pub struct CharacterRender {
    body: StaticMesh,
    fur: StaticMesh,
    cloth: StaticMesh,

    beauty_pipeline: wgpu::RenderPipeline,
    fur_pipeline: wgpu::RenderPipeline,
    cascade_pipeline: wgpu::RenderPipeline,
    prepass_pipeline: wgpu::RenderPipeline,
    cloth_pipeline: wgpu::RenderPipeline,
    cloth_cascade_pipeline: wgpu::RenderPipeline,
    cloth_prepass_pipeline: wgpu::RenderPipeline,

    beauty_texture_layout: wgpu::BindGroupLayout,
    depth_texture_layout: wgpu::BindGroupLayout,

    beauty_uniforms: UniformSlot,
    prepass_uniforms: UniformSlot,
    cascade_uniforms: [UniformSlot; CASCADE_COUNT],

    beauty_textures: Option<wgpu::BindGroup>,
    depth_textures: Option<wgpu::BindGroup>,
    pub visible: bool,
}

pub fn new(
    device: &wgpu::Device,
    library: &mut ShaderLibrary,
    color_format: wgpu::TextureFormat,
) -> CharacterRender {
    let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("snow_character_uniforms"),
        entries: &[uniform_entry(0)],
    });
    let beauty_texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("snow_character_textures"),
        entries: &[
            texture_entry(0, true),
            texture_entry(1, true),
            sampler_entry(2, true),
            texture_entry(3, true),
            texture_entry(4, true),
            texture_entry(5, true),
            sampler_entry(6, true),
        ],
    });
    let depth_texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("snow_character_depth_textures"),
        entries: &[texture_entry(0, true)],
    });

    let beauty_module = shaders::compile(library, device, "character", shaders::CHARACTER);
    let fur_module = shaders::compile(library, device, "character_fur", shaders::CHARACTER_FUR);
    let cascade_module =
        shaders::compile(library, device, "character_depth", shaders::CHARACTER_DEPTH);
    let prepass_module = shaders::compile(
        library,
        device,
        "character_prepass",
        shaders::CHARACTER_PREPASS,
    );
    let cloth_module = shaders::compile(library, device, "cloth", shaders::CLOTH);
    let cloth_cascade_module =
        shaders::compile(library, device, "cloth_depth", shaders::CLOTH_DEPTH);
    let cloth_prepass_module =
        shaders::compile(library, device, "cloth_prepass", shaders::CLOTH_PREPASS);

    CharacterRender {
        body: build_body(device),
        fur: build_fur(device),
        cloth: build_cloth(device, &Cloth::default()),
        beauty_pipeline: geometry_pipeline(
            device,
            GeometrySpec {
                label: "snow_character",
                module: &beauty_module,
                layouts: &[Some(&uniform_layout), Some(&beauty_texture_layout)],
                vertices: SKINNED_LAYOUT,
                color: color_format,
                blend: None,
                depth: Some(DEPTH_FORMAT),
            },
        ),
        fur_pipeline: geometry_pipeline(
            device,
            GeometrySpec {
                label: "snow_character_fur",
                module: &fur_module,
                layouts: &[Some(&uniform_layout), Some(&beauty_texture_layout)],
                vertices: SKINNED_LAYOUT,
                color: color_format,
                blend: None,
                depth: Some(DEPTH_FORMAT),
            },
        ),
        cascade_pipeline: geometry_pipeline(
            device,
            GeometrySpec {
                label: "snow_character_cascade",
                module: &cascade_module,
                layouts: &[Some(&uniform_layout), Some(&depth_texture_layout)],
                vertices: SKINNED_LAYOUT,
                color: CASCADE_FORMAT,
                blend: None,
                depth: Some(DEPTH_FORMAT),
            },
        ),
        prepass_pipeline: geometry_pipeline(
            device,
            GeometrySpec {
                label: "snow_character_prepass",
                module: &prepass_module,
                layouts: &[Some(&uniform_layout), Some(&depth_texture_layout)],
                vertices: SKINNED_LAYOUT,
                color: HDR_FORMAT,
                blend: None,
                depth: Some(DEPTH_FORMAT),
            },
        ),
        cloth_pipeline: geometry_pipeline(
            device,
            GeometrySpec {
                label: "snow_cloth",
                module: &cloth_module,
                layouts: &[Some(&uniform_layout), Some(&beauty_texture_layout)],
                vertices: CLOTH_LAYOUT,
                color: color_format,
                blend: None,
                depth: Some(DEPTH_FORMAT),
            },
        ),
        cloth_cascade_pipeline: geometry_pipeline(
            device,
            GeometrySpec {
                label: "snow_cloth_cascade",
                module: &cloth_cascade_module,
                layouts: &[Some(&uniform_layout), Some(&depth_texture_layout)],
                vertices: CLOTH_LAYOUT,
                color: CASCADE_FORMAT,
                blend: None,
                depth: Some(DEPTH_FORMAT),
            },
        ),
        cloth_prepass_pipeline: geometry_pipeline(
            device,
            GeometrySpec {
                label: "snow_cloth_prepass",
                module: &cloth_prepass_module,
                layouts: &[Some(&uniform_layout), Some(&depth_texture_layout)],
                vertices: CLOTH_LAYOUT,
                color: HDR_FORMAT,
                blend: None,
                depth: Some(DEPTH_FORMAT),
            },
        ),
        beauty_uniforms: uniform_slot(
            device,
            "snow_character",
            &uniform_layout,
            std::mem::size_of::<CharUniforms>() as u64,
        ),
        prepass_uniforms: uniform_slot(
            device,
            "snow_character_prepass",
            &uniform_layout,
            std::mem::size_of::<CharUniforms>() as u64,
        ),
        cascade_uniforms: std::array::from_fn(|_| {
            uniform_slot(
                device,
                "snow_character_cascade",
                &uniform_layout,
                std::mem::size_of::<CharUniforms>() as u64,
            )
        }),
        beauty_texture_layout,
        depth_texture_layout,
        beauty_textures: None,
        depth_textures: None,
        visible: true,
    }
}

pub fn bind(character: &mut CharacterRender, device: &wgpu::Device, gpu: &SnowGpu) {
    character.beauty_textures = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("snow_character_textures"),
        layout: &character.beauty_texture_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&gpu.character.view),
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
    character.depth_textures = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("snow_character_depth_textures"),
        layout: &character.depth_texture_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(&gpu.character.view),
        }],
    }));
}

pub fn write(
    character: &CharacterRender,
    queue: &wgpu::Queue,
    uniforms: &CharUniforms,
    cascades: &[[f32; 16]],
) {
    queue.write_buffer(
        &character.beauty_uniforms.buffer,
        0,
        bytemuck::bytes_of(uniforms),
    );
    queue.write_buffer(
        &character.prepass_uniforms.buffer,
        0,
        bytemuck::bytes_of(uniforms),
    );
    for (index, slot) in character.cascade_uniforms.iter().enumerate() {
        let mut cascade = *uniforms;
        cascade.view_projection = cascades[index];
        queue.write_buffer(&slot.buffer, 0, bytemuck::bytes_of(&cascade));
    }
}

fn draw(
    pass: &mut wgpu::RenderPass<'_>,
    pipeline: &wgpu::RenderPipeline,
    uniforms: &UniformSlot,
    textures: &wgpu::BindGroup,
    mesh: &StaticMesh,
) {
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, &uniforms.bind_group, &[]);
    pass.set_bind_group(1, textures, &[]);
    pass.set_vertex_buffer(0, mesh.vertices.slice(..));
    pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
    pass.draw_indexed(0..mesh.index_count, 0, 0..1);
}

pub fn draw_cascade(character: &CharacterRender, pass: &mut wgpu::RenderPass<'_>, cascade: usize) {
    if !character.visible || cascade >= CHARACTER_CASCADES {
        return;
    }
    let Some(textures) = &character.depth_textures else {
        return;
    };
    draw(
        pass,
        &character.cascade_pipeline,
        &character.cascade_uniforms[cascade],
        textures,
        &character.body,
    );
    draw(
        pass,
        &character.cloth_cascade_pipeline,
        &character.cascade_uniforms[cascade],
        textures,
        &character.cloth,
    );
}

pub fn draw_prepass(character: &CharacterRender, pass: &mut wgpu::RenderPass<'_>) {
    if !character.visible {
        return;
    }
    let Some(textures) = &character.depth_textures else {
        return;
    };
    draw(
        pass,
        &character.prepass_pipeline,
        &character.prepass_uniforms,
        textures,
        &character.body,
    );
    draw(
        pass,
        &character.cloth_prepass_pipeline,
        &character.prepass_uniforms,
        textures,
        &character.cloth,
    );
}

pub fn draw_beauty(character: &CharacterRender, pass: &mut wgpu::RenderPass<'_>) {
    if !character.visible {
        return;
    }
    let Some(textures) = &character.beauty_textures else {
        return;
    };
    draw(
        pass,
        &character.beauty_pipeline,
        &character.beauty_uniforms,
        textures,
        &character.body,
    );
    draw(
        pass,
        &character.cloth_pipeline,
        &character.beauty_uniforms,
        textures,
        &character.cloth,
    );
    draw(
        pass,
        &character.fur_pipeline,
        &character.beauty_uniforms,
        textures,
        &character.fur,
    );
}

/// The body and the garments in each of the two near cascades and the prepass, plus
/// both and the fur in the beauty pass.
pub fn draw_calls(character: &CharacterRender) -> u32 {
    if character.visible {
        CHARACTER_CASCADES as u32 * 2 + 2 + 3
    } else {
        0
    }
}

pub fn triangles(character: &CharacterRender) -> u32 {
    if !character.visible {
        return 0;
    }
    let body = character.body.index_count / 3;
    let cloth = character.cloth.index_count / 3;
    (body + cloth) * (CHARACTER_CASCADES as u32 + 1) + body + cloth + character.fur.index_count / 3
}
