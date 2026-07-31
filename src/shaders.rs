use naga_oil::compose::{
    ComposableModuleDescriptor, Composer, NagaModuleDescriptor, ShaderLanguage,
};
use nightshade::prelude::wgpu;
use std::collections::HashMap;

/// Every shared module, in dependency order.
const MODULES: [(&str, &str); 20] = [
    (
        "fullscreen.wgsl",
        include_str!("shaders/lib/fullscreen.wgsl"),
    ),
    ("noise.wgsl", include_str!("shaders/lib/noise.wgsl")),
    ("terrain.wgsl", include_str!("shaders/lib/terrain.wgsl")),
    ("clipmap.wgsl", include_str!("shaders/lib/clipmap.wgsl")),
    ("deform.wgsl", include_str!("shaders/lib/deform.wgsl")),
    ("shading.wgsl", include_str!("shaders/lib/shading.wgsl")),
    (
        "atmosphere.wgsl",
        include_str!("shaders/lib/atmosphere.wgsl"),
    ),
    (
        "shadow_lookup.wgsl",
        include_str!("shaders/lib/shadow_lookup.wgsl"),
    ),
    (
        "spell_lights.wgsl",
        include_str!("shaders/lib/spell_lights.wgsl"),
    ),
    (
        "terrain_vertex.wgsl",
        include_str!("shaders/lib/terrain_vertex.wgsl"),
    ),
    (
        "snow_uniforms.wgsl",
        include_str!("shaders/lib/snow_uniforms.wgsl"),
    ),
    ("ridge.wgsl", include_str!("shaders/lib/ridge.wgsl")),
    ("wake.wgsl", include_str!("shaders/lib/wake.wgsl")),
    ("water.wgsl", include_str!("shaders/lib/water.wgsl")),
    ("crystal.wgsl", include_str!("shaders/lib/crystal.wgsl")),
    ("char_skin.wgsl", include_str!("shaders/lib/char_skin.wgsl")),
    (
        "char_uniforms.wgsl",
        include_str!("shaders/lib/char_uniforms.wgsl"),
    ),
    ("fabric.wgsl", include_str!("shaders/lib/fabric.wgsl")),
    (
        "post_common.wgsl",
        include_str!("shaders/lib/post_common.wgsl"),
    ),
    (
        "post_uniforms.wgsl",
        include_str!("shaders/lib/post_uniforms.wgsl"),
    ),
];

pub const HEIGHT_BAKE: &str = include_str!("shaders/height_bake.wgsl");
pub const AUX_BAKE: &str = include_str!("shaders/aux_bake.wgsl");
pub const CHARACTER: &str = include_str!("shaders/character.wgsl");
pub const CHARACTER_FUR: &str = include_str!("shaders/character_fur.wgsl");
pub const CHARACTER_DEPTH: &str = include_str!("shaders/character_depth.wgsl");
pub const CHARACTER_PREPASS: &str = include_str!("shaders/character_prepass.wgsl");
pub const CLOTH: &str = include_str!("shaders/cloth.wgsl");
pub const CLOTH_DEPTH: &str = include_str!("shaders/cloth_depth.wgsl");
pub const CLOTH_PREPASS: &str = include_str!("shaders/cloth_prepass.wgsl");
pub const DEFORM_SIM: &str = include_str!("shaders/deform_sim.wgsl");
pub const DETAIL_BAKE: &str = include_str!("shaders/detail_bake.wgsl");
pub const SKY_BAKE: &str = include_str!("shaders/sky_bake.wgsl");
pub const MIP: &str = include_str!("shaders/mip.wgsl");
pub const POST_SSR: &str = include_str!("shaders/post_ssr.wgsl");
pub const POST_TAA: &str = include_str!("shaders/post_taa.wgsl");
pub const POST_SHAFTS: &str = include_str!("shaders/post_shafts.wgsl");
pub const POST_BLOOM_DOWN: &str = include_str!("shaders/post_bloom_down.wgsl");
pub const POST_BLOOM_BLUR: &str = include_str!("shaders/post_bloom_blur.wgsl");
pub const POST_DOF: &str = include_str!("shaders/post_dof.wgsl");
pub const POST_TONEMAP: &str = include_str!("shaders/post_tonemap.wgsl");
pub const POST_SHARPEN: &str = include_str!("shaders/post_sharpen.wgsl");
pub const SKY: &str = include_str!("shaders/sky.wgsl");
pub const SNOW: &str = include_str!("shaders/snow.wgsl");
pub const SPRAY: &str = include_str!("shaders/spray.wgsl");
pub const TERRAIN_DEPTH: &str = include_str!("shaders/terrain_depth.wgsl");
pub const TERRAIN_PREPASS: &str = include_str!("shaders/terrain_prepass.wgsl");
pub const WAKE: &str = include_str!("shaders/wake.wgsl");
pub const WAKE_DEPTH: &str = include_str!("shaders/wake_depth.wgsl");
pub const WAKE_PREPASS: &str = include_str!("shaders/wake_prepass.wgsl");
pub const WATER: &str = include_str!("shaders/water.wgsl");
pub const CRYSTAL: &str = include_str!("shaders/crystal.wgsl");
pub const CRYSTAL_DEPTH: &str = include_str!("shaders/crystal_depth.wgsl");
pub const CRYSTAL_PREPASS: &str = include_str!("shaders/crystal_prepass.wgsl");

/// Owns the composer preloaded with every shared module.
pub struct ShaderLibrary {
    composer: Composer,
}

impl Default for ShaderLibrary {
    fn default() -> Self {
        new()
    }
}

pub fn new() -> ShaderLibrary {
    let mut composer = Composer::default();
    for (path, source) in MODULES.iter() {
        composer
            .add_composable_module(ComposableModuleDescriptor {
                source,
                file_path: path,
                language: ShaderLanguage::Wgsl,
                as_name: None,
                additional_imports: &[],
                shader_defs: HashMap::new(),
            })
            .unwrap_or_else(|error| panic!("failed to register {path}: {error}"));
    }
    ShaderLibrary { composer }
}

/// Composes a fullscreen pass, wrapping `source` in the shared triangle's import
/// and vertex stage.
pub fn compile_fullscreen(
    library: &mut ShaderLibrary,
    device: &wgpu::Device,
    label: &str,
    source: &str,
) -> wgpu::ShaderModule {
    let wrapped = format!(
        "#import snow::fullscreen::{{FullscreenVertex, fullscreenTriangle}}\n\
             {source}\n\
             @vertex\n\
             fn fullscreenVertex(@builtin(vertex_index) index: u32) -> FullscreenVertex {{\n\
             \x20   return fullscreenTriangle(index);\n\
             }}\n"
    );
    compile(library, device, label, &wrapped)
}

/// Composes `source` against the shared modules and creates a shader module.
pub fn compile(
    library: &mut ShaderLibrary,
    device: &wgpu::Device,
    label: &str,
    source: &str,
) -> wgpu::ShaderModule {
    let module = library
        .composer
        .make_naga_module(NagaModuleDescriptor {
            source,
            file_path: label,
            shader_type: naga_oil::compose::ShaderType::Wgsl,
            shader_defs: HashMap::new(),
            additional_imports: &[],
        })
        .unwrap_or_else(|error| {
            panic!(
                "failed to compose {label}: {}",
                error.emit_to_string(&library.composer)
            )
        });

    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Naga(std::borrow::Cow::Owned(module)),
    })
}
