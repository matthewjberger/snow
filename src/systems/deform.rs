use crate::constants::{BRUSH_ROWS, DEFORM_COVERAGE, MAX_BRUSHES};
use nalgebra_glm::{Vec2, Vec3};

/// Seconds of relaxation banked before it is worth applying.
const RELAX_STEP: f32 = 0.4;

/// The CPU half of the terrain state buffer.
pub struct Deformation {
    /// Three rows of brush parameters, laid out as the rows of a data texture.
    staging: Vec<f32>,
    count: usize,
    /// Window centre this frame, snapped to texel boundaries.
    pub centre: Vec2,
    pub previous_centre: Vec2,
    pub resolution: u32,
    pub texel: f32,
    relax_owed: f32,
    /// Seconds of relaxation to apply this dispatch, which is zero on most frames.
    pub relax_step: f32,
}

impl Default for Deformation {
    fn default() -> Self {
        let resolution = 2048;
        Self {
            staging: vec![0.0; MAX_BRUSHES * BRUSH_ROWS as usize * 4],
            count: 0,
            centre: Vec2::zeros(),
            previous_centre: Vec2::zeros(),
            resolution,
            texel: DEFORM_COVERAGE / resolution as f32,
            relax_owed: 0.0,
            relax_step: 0.0,
        }
    }
}

pub fn brush_count(deform: &Deformation) -> u32 {
    deform.count as u32
}

pub fn staging(deform: &Deformation) -> &[f32] {
    &deform.staging
}

/// Queues a brush for this frame, accumulating additively into whatever is already
/// there.
pub fn brush(deform: &mut Deformation, brush: &Brush) {
    if deform.count >= MAX_BRUSHES || brush.radius <= 0.0 {
        return;
    }

    let reach = DEFORM_COVERAGE * 0.5 + brush.radius * 2.0;
    if (brush.x - deform.centre.x).abs() > reach || (brush.z - deform.centre.y).abs() > reach {
        return;
    }

    let index = deform.count;
    deform.count += 1;
    let stride = MAX_BRUSHES * 4;
    let offset = index * 4;

    deform.staging[offset] = brush.x;
    deform.staging[offset + 1] = brush.z;
    deform.staging[offset + 2] = brush.radius;
    deform.staging[offset + 3] = brush.elongation;

    deform.staging[stride + offset] = brush.yaw.cos();
    deform.staging[stride + offset + 1] = brush.yaw.sin();
    deform.staging[stride + offset + 2] = brush.depth;
    deform.staging[stride + offset + 3] = brush.berm;

    deform.staging[stride * 2 + offset] = brush.compression;
    deform.staging[stride * 2 + offset + 1] = brush.ice;
    deform.staging[stride * 2 + offset + 2] = brush.edge;
    deform.staging[stride * 2 + offset + 3] = (brush.x * 0.37 + brush.z * 0.71) % 100.0;
}

/// Advances the window and decides how much relaxation this dispatch spends.
pub fn update(deform: &mut Deformation, delta_time: f32, focus: &Vec3) {
    deform.previous_centre = deform.centre;

    deform.centre.x = (focus.x / deform.texel).round() * deform.texel;
    deform.centre.y = (focus.z / deform.texel).round() * deform.texel;

    deform.relax_owed += delta_time;
    deform.relax_step = 0.0;
    if deform.relax_owed >= RELAX_STEP {
        deform.relax_step = deform.relax_owed;
        deform.relax_owed = 0.0;
    }
}

/// Clears the staged brushes, and zeroes the radius of every slot past the live
/// ones so every later frame reads a radius this frame wrote.
pub fn end_frame(deform: &mut Deformation) {
    for index in deform.count..MAX_BRUSHES {
        deform.staging[index * 4 + 2] = 0.0;
    }
    deform.count = 0;
}

/// One mark on the snow.
pub struct Brush {
    pub x: f32,
    pub z: f32,
    /// Metres, across the short axis.
    pub radius: f32,
    /// Metres of depression at the centre.
    pub depth: f32,
    /// Metres of displaced mass thrown to the rim.
    pub berm: f32,
    pub compression: f32,
    /// Taken as a maximum rather than added.
    pub ice: f32,
    /// Radians, orienting the long axis.
    pub yaw: f32,
    /// Long-axis multiple of the radius.
    pub elongation: f32,
    /// Rim roughness.
    pub edge: f32,
}

impl Default for Brush {
    fn default() -> Self {
        Self {
            x: 0.0,
            z: 0.0,
            radius: 0.0,
            depth: 0.0,
            berm: 0.0,
            compression: 0.0,
            ice: 0.0,
            yaw: 0.0,
            elongation: 1.0,
            edge: 1.0,
        }
    }
}
