use crate::constants::CASCADE_COUNT;
use nalgebra_glm::Mat4;

/// The receiving half of the cascaded shadow maps, mirroring `ShadowParams` in
/// `snow::shadow_lookup` field for field.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ShadowUniforms {
    pub matrices: [[f32; 16]; CASCADE_COUNT],
    /// Per cascade: (depth range in metres, ortho width in metres, 0, 0).
    pub cascade: [[f32; 4]; CASCADE_COUNT],
    /// Far distance of each cascade; w repeats the last.
    pub splits: [f32; 4],
    /// (one shadow texel in UV, softness, depth bias in metres, 0)
    pub filter: [f32; 4],
    /// (direction toward the sun, 0)
    pub sun_direction: [f32; 4],
}

impl Default for ShadowUniforms {
    fn default() -> Self {
        Self {
            matrices: [identity(); CASCADE_COUNT],
            cascade: [[1.0, 1.0, 0.0, 0.0]; CASCADE_COUNT],
            splits: [1.0, 1.0, 1.0, 1.0],
            filter: [0.0; 4],
            sun_direction: [0.0, 1.0, 0.0, 0.0],
        }
    }
}

/// The dynamic light pool, mirroring `SpellLights` in `snow::spell_lights`.
#[repr(C)]
#[derive(Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SpellLightUniforms {
    /// xyz world position, w radius in metres.
    pub positions: [[f32; 4]; 4],
    /// rgb colour, w intensity.
    pub colors: [[f32; 4]; 4],
    /// (live count, 0, 0, 0)
    pub count: [f32; 4],
}

/// The terrain's uniform block, mirroring `SnowUniforms` in `snow::snow_uniforms`.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SnowUniforms {
    pub view_projection: [f32; 16],
    /// (camera position, 0)
    pub camera: [f32; 4],
    /// (ring centre xz, base spacing, grid half extent)
    pub clipmap: [f32; 4],
    /// (world origin xz, world size, height resolution)
    pub field: [f32; 4],
    /// (wind angle, macro amplitude, sastrugi amplitude, detail strength)
    pub surface: [f32; 4],
    /// (glint intensity, glint gate, subsurface strength, subsurface radius)
    pub snow: [f32; 4],
    /// (fog density, height falloff, fog start, aerial strength)
    pub fog: [f32; 4],
    /// (deform centre xz, deform size, deform texel)
    pub deform: [f32; 4],
    /// (deform depth scale, ambient intensity, debug mode, 0)
    pub misc: [f32; 4],
    /// (render target size in pixels, 0, 0)
    pub screen: [f32; 4],
    /// (direction toward the sun, 0)
    pub sun_direction: [f32; 4],
    /// (direct solar irradiance at the ground, 0)
    pub sun_radiance: [f32; 4],
    /// (live spine samples, lattice columns, lattice rows, wake clock)
    pub wake: [f32; 4],
    /// (lattice columns, section rings, clock, depth tint)
    pub water: [f32; 4],
    /// Per strand: (profile, milkiness, alpha, live column count)
    pub strands: [[f32; 4]; 8],
    /// The camera's world-space right and up, which the spray billboards face.
    pub billboard: [[f32; 4]; 2],
    pub harmonics: [[f32; 4]; 9],
    pub shadow: ShadowUniforms,
    pub lights: SpellLightUniforms,
}

impl Default for SnowUniforms {
    fn default() -> Self {
        Self {
            view_projection: identity(),
            camera: [0.0; 4],
            clipmap: [0.0; 4],
            field: [0.0; 4],
            surface: [0.0; 4],
            snow: [0.0; 4],
            fog: [0.0; 4],
            deform: [0.0; 4],
            misc: [0.0; 4],
            screen: [1.0, 1.0, 0.0, 0.0],
            sun_direction: [0.0, 1.0, 0.0, 0.0],
            sun_radiance: [0.0; 4],
            wake: [0.0; 4],
            water: [0.0; 4],
            strands: [[0.0; 4]; 8],
            billboard: [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0]],
            harmonics: [[0.0; 4]; 9],
            shadow: ShadowUniforms::default(),
            lights: SpellLightUniforms::default(),
        }
    }
}

/// Flattens a matrix into the column-major order a WGSL `mat4x4f` reads.
pub fn matrix_columns(matrix: &Mat4) -> [f32; 16] {
    let mut out = [0.0; 16];
    out.copy_from_slice(matrix.as_slice());
    out
}

fn identity() -> [f32; 16] {
    let mut out = [0.0; 16];
    out[0] = 1.0;
    out[5] = 1.0;
    out[10] = 1.0;
    out[15] = 1.0;
    out
}
