use nightshade::prelude::wgpu::TextureFormat;

/// Metres across the whole baked field.
pub const WORLD_SIZE: f32 = 2048.0;
/// Height texture resolution: half a metre per texel.
pub const HEIGHT_RES: u32 = 4096;
/// Slope, rock mask and exposure, derived from the height bake.
pub const AUX_RES: u32 = 2048;
/// Half-extent the player is kept inside, leaving margin for the far rings.
pub const PLAY_RADIUS: f32 = 620.0;

/// Tiled snow grain, sampled at three world scales by the snow material.
pub const DETAIL_RES: u32 = 1024;

pub const SKY_LUT_WIDTH: u32 = 512;
pub const SKY_LUT_HEIGHT: u32 = 256;
/// Low-resolution copy of the same bake, read back on the CPU for the spherical
/// harmonic projection.
pub const SKY_SH_WIDTH: u32 = 64;
pub const SKY_SH_HEIGHT: u32 = 32;

/// Three cascades, not four.
pub const CASCADE_COUNT: usize = 3;
pub const CASCADE_RESOLUTION: u32 = 2048;
/// Far distance of each cascade, in metres.
pub const CASCADE_SPLITS: [f32; CASCADE_COUNT] = [26.0, 95.0, 330.0];

/// Deformation window coverage in metres.
pub const DEFORM_COVERAGE: f32 = 80.0;
/// Rows in the brush data texture, and the most brushes one frame may stage.
pub const BRUSH_ROWS: u32 = 3;
pub const MAX_BRUSHES: usize = 96;

/// The character's transform texture.
pub const CHARACTER_TEX_WIDTH: u32 = 48;
pub const CHARACTER_TEX_HEIGHT: u32 = 64;
/// First texture row available to garment panels.
pub const CLOTH_ROW0: u32 = 4;

/// Quads per side, per clipmap ring.
pub const GRID_N: u32 = 160;
/// Number of rings.
pub const CLIPMAP_LEVELS: u32 = 8;
/// Vertex spacing of the innermost ring, in metres.
pub const BASE_SPACING: f32 = 0.085;
/// How many cells to shrink each hole by, guaranteeing ring overlap.
pub const HOLE_SHRINK: i32 = 3;

pub const HDR_FORMAT: TextureFormat = TextureFormat::Rgba16Float;
pub const HEIGHT_FORMAT: TextureFormat = TextureFormat::Rg32Float;
pub const AUX_FORMAT: TextureFormat = TextureFormat::Rgba16Float;
pub const DETAIL_FORMAT: TextureFormat = TextureFormat::Rgba8Unorm;
pub const SKY_LUT_FORMAT: TextureFormat = TextureFormat::Rgba16Float;
pub const SKY_SH_FORMAT: TextureFormat = TextureFormat::Rgba32Float;
pub const CASCADE_FORMAT: TextureFormat = TextureFormat::R32Float;
pub const DEFORM_FORMAT: TextureFormat = TextureFormat::Rgba16Float;
pub const DATA_FORMAT: TextureFormat = TextureFormat::Rgba32Float;
pub const DEPTH_FORMAT: TextureFormat = TextureFormat::Depth32Float;
