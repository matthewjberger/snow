use crate::constants::*;
use nightshade::prelude::wgpu;

/// A texture kept alive for the life of the program, with its default view.
pub struct SnowTexture {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub width: u32,
    pub height: u32,
    pub mip_levels: u32,
}

fn texture(
    device: &wgpu::Device,
    label: &str,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
    mip_levels: u32,
    usage: wgpu::TextureUsages,
) -> SnowTexture {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: mip_levels,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    SnowTexture {
        texture,
        view,
        width,
        height,
        mip_levels,
    }
}

/// A view of one mip level, for writing that level as a render attachment or
/// reading the level above it.
pub fn mip_view(texture: &SnowTexture, level: u32) -> wgpu::TextureView {
    texture.texture.create_view(&wgpu::TextureViewDescriptor {
        base_mip_level: level,
        mip_level_count: Some(1),
        ..Default::default()
    })
}

/// Every GPU resource that outlives a frame.
pub struct SnowGpu {
    /// Macro landform: R is height in metres, G the rock mask.
    pub height: SnowTexture,
    /// Slope XY, rock mask, exposure.
    pub aux: SnowTexture,
    /// Tiling snow grain: normal XY, cavity, height.
    pub detail: SnowTexture,
    /// Equirectangular sky radiance, mipped so the aerial perspective can read a
    /// blurred hemisphere.
    pub sky_lut: SnowTexture,
    /// The same bake at low resolution, read back for the harmonic projection.
    pub sky_sh: SnowTexture,

    /// Terrain state, ping-ponged: depression, displaced mass, compression, ice.
    pub deform: [SnowTexture; 2],
    /// Index of the target holding the current frame's state.
    pub deform_read: usize,

    /// Sun shadow cascades, plain depth in a colour target so the soft-shadow filter
    /// can run a real blocker search.
    pub cascades: [SnowTexture; CASCADE_COUNT],
    pub cascade_depth: SnowTexture,

    /// Brush staging for the deformation simulation.
    pub brush: SnowTexture,
    /// Bone matrices and simulated cloth nodes, uploaded once per frame.
    pub character: SnowTexture,
    /// Two rows of particle state, rewritten every frame by the spray field.
    pub spray: SnowTexture,
    /// Three rows of wake spine, rewritten every frame while surfing.
    pub wake: SnowTexture,
    /// Three rows per strand of bent water, rewritten every frame.
    pub water: SnowTexture,
    /// Three rows per grown ice prism.
    pub crystal: SnowTexture,

    /// The scene the world pass draws into and the chain reads. Owned here
    /// rather than taken from the graph because the demo renders at its own
    /// internal resolution, and the graph's transients are sized to the surface
    /// for the engine's own present and screenshot paths.
    pub scene: SnowTexture,
    pub scene_depth: SnowTexture,
    /// Camera-space depth and roughness, drawn before the beauty pass.
    pub prepass: SnowTexture,
    pub prepass_depth: SnowTexture,

    pub linear_clamp: wgpu::Sampler,
    pub linear_repeat: wgpu::Sampler,
    pub linear_mip_repeat: wgpu::Sampler,
    /// The sky LUT is equirectangular, so longitude wraps and latitude does not.
    pub sky_sampler: wgpu::Sampler,
}

const RENDER_AND_SAMPLE: wgpu::TextureUsages =
    wgpu::TextureUsages::RENDER_ATTACHMENT.union(wgpu::TextureUsages::TEXTURE_BINDING);

/// The scene targets, at whatever internal resolution is in force.
fn scene_targets(device: &wgpu::Device, width: u32, height: u32) -> [SnowTexture; 4] {
    let width = width.max(1);
    let height = height.max(1);
    [
        texture(
            device,
            "snow_scene",
            HDR_FORMAT,
            width,
            height,
            1,
            RENDER_AND_SAMPLE,
        ),
        texture(
            device,
            "snow_scene_depth",
            DEPTH_FORMAT,
            width,
            height,
            1,
            RENDER_AND_SAMPLE,
        ),
        texture(
            device,
            "snow_prepass",
            HDR_FORMAT,
            width,
            height,
            1,
            RENDER_AND_SAMPLE,
        ),
        texture(
            device,
            "snow_prepass_depth",
            DEPTH_FORMAT,
            width,
            height,
            1,
            RENDER_AND_SAMPLE,
        ),
    ]
}

/// Reallocates the scene targets at a new internal resolution.
pub fn resize_scene(gpu: &mut SnowGpu, device: &wgpu::Device, width: u32, height: u32) {
    let [scene, scene_depth, prepass, prepass_depth] = scene_targets(device, width, height);
    gpu.scene = scene;
    gpu.scene_depth = scene_depth;
    gpu.prepass = prepass;
    gpu.prepass_depth = prepass_depth;
}

pub fn new(device: &wgpu::Device, deform_resolution: u32, width: u32, height: u32) -> SnowGpu {
    let [scene, scene_depth, prepass, prepass_depth] = scene_targets(device, width, height);
    let detail_mips = mip_level_count(DETAIL_RES, DETAIL_RES);
    let sky_mips = mip_level_count(SKY_LUT_WIDTH, SKY_LUT_HEIGHT);

    let deform = [
        texture(
            device,
            "snow_deform_0",
            DEFORM_FORMAT,
            deform_resolution,
            deform_resolution,
            1,
            RENDER_AND_SAMPLE,
        ),
        texture(
            device,
            "snow_deform_1",
            DEFORM_FORMAT,
            deform_resolution,
            deform_resolution,
            1,
            RENDER_AND_SAMPLE,
        ),
    ];

    let cascades = std::array::from_fn(|_| {
        texture(
            device,
            "snow_cascade",
            CASCADE_FORMAT,
            CASCADE_RESOLUTION,
            CASCADE_RESOLUTION,
            1,
            RENDER_AND_SAMPLE,
        )
    });

    SnowGpu {
        height: texture(
            device,
            "snow_height",
            HEIGHT_FORMAT,
            HEIGHT_RES,
            HEIGHT_RES,
            1,
            RENDER_AND_SAMPLE | wgpu::TextureUsages::COPY_SRC,
        ),
        aux: texture(
            device,
            "snow_aux",
            AUX_FORMAT,
            AUX_RES,
            AUX_RES,
            1,
            RENDER_AND_SAMPLE,
        ),
        detail: texture(
            device,
            "snow_detail",
            DETAIL_FORMAT,
            DETAIL_RES,
            DETAIL_RES,
            detail_mips,
            RENDER_AND_SAMPLE,
        ),
        sky_lut: texture(
            device,
            "snow_sky_lut",
            SKY_LUT_FORMAT,
            SKY_LUT_WIDTH,
            SKY_LUT_HEIGHT,
            sky_mips,
            RENDER_AND_SAMPLE,
        ),
        sky_sh: texture(
            device,
            "snow_sky_sh",
            SKY_SH_FORMAT,
            SKY_SH_WIDTH,
            SKY_SH_HEIGHT,
            1,
            RENDER_AND_SAMPLE | wgpu::TextureUsages::COPY_SRC,
        ),
        deform,
        deform_read: 0,
        cascades,
        cascade_depth: texture(
            device,
            "snow_cascade_depth",
            DEPTH_FORMAT,
            CASCADE_RESOLUTION,
            CASCADE_RESOLUTION,
            1,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        ),
        brush: texture(
            device,
            "snow_brush",
            DATA_FORMAT,
            MAX_BRUSHES as u32,
            BRUSH_ROWS,
            1,
            wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        ),
        character: texture(
            device,
            "snow_character",
            DATA_FORMAT,
            CHARACTER_TEX_WIDTH,
            CHARACTER_TEX_HEIGHT,
            1,
            wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        ),
        spray: texture(
            device,
            "snow_spray_state",
            DATA_FORMAT,
            crate::systems::spray::SPRAY_CAPACITY as u32,
            2,
            1,
            wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        ),
        wake: texture(
            device,
            "snow_wake_spine",
            DATA_FORMAT,
            crate::systems::wake::SPINE_MAX as u32,
            3,
            1,
            wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        ),
        water: texture(
            device,
            "snow_water_strands",
            DATA_FORMAT,
            crate::systems::spell::water::STRAND_COLS as u32,
            crate::systems::spell::water::STRAND_MAX as u32 * 3,
            1,
            wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        ),
        crystal: texture(
            device,
            "snow_crystal_field",
            DATA_FORMAT,
            crate::systems::spell::crystals::CRYSTAL_MAX as u32,
            3,
            1,
            wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        ),
        scene,
        scene_depth,
        prepass,
        prepass_depth,

        linear_clamp: sampler(
            device,
            "snow_linear_clamp",
            wgpu::AddressMode::ClampToEdge,
            1,
        ),
        linear_repeat: sampler(device, "snow_linear_repeat", wgpu::AddressMode::Repeat, 1),
        linear_mip_repeat: sampler(
            device,
            "snow_linear_mip_repeat",
            wgpu::AddressMode::Repeat,
            detail_mips,
        ),
        sky_sampler: device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("snow_sky"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            lod_min_clamp: 0.0,
            lod_max_clamp: sky_mips as f32,
            ..Default::default()
        }),
    }
}

fn sampler(
    device: &wgpu::Device,
    label: &str,
    address: wgpu::AddressMode,
    mip_levels: u32,
) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some(label),
        address_mode_u: address,
        address_mode_v: address,
        address_mode_w: address,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
        lod_min_clamp: 0.0,
        lod_max_clamp: mip_levels as f32,
        ..Default::default()
    })
}

pub fn mip_level_count(width: u32, height: u32) -> u32 {
    32 - width.max(height).leading_zeros()
}
