/// Must match the strand array in `snow::snow_uniforms`.
pub const STRAND_MAX: usize = 8;

/// Spine samples per strand, and the most columns any spell may write.
///
/// Sized for the vortex, whose helices are the tightest curve anything here
/// draws. A cubic through samples on a circular arc carries a radial error that
/// peaks in the middle of each span, which is a scallop per sample; the error
/// falls with the square of the spacing, and this many takes it under a percent.
pub const STRAND_COLS: usize = 64;

/// Spine vertices per strand, decoupled from the sample count.
///
/// The surface is a spline through the samples, so it has real curvature between
/// them, and drawing it at barely more than one vertex per sample renders that
/// curvature as a polygon. Nearly three per sample is where the segmentation
/// stops being findable.
pub const LATTICE_COLS: usize = 176;

/// Vertices around the section, with the last coinciding with the first.
///
/// Duplicating the seam vertex rather than wrapping the index is what lets the
/// same lattice serve the open sheet profile, where the last ring is genuinely
/// the far edge. Twenty-four rather than twelve, because a twelve-sided tube at
/// two metres has a readable silhouette and it is the clearest tell that the
/// water is a mesh.
pub const RING: usize = 24;

pub const PROFILE_TUBE: f32 = 0.0;
pub const PROFILE_SHEET: f32 = 1.0;

/// The bent-water body: one mesh, one material, one draw, eight strands.
///
/// Four of the five spells move a coherent body of water and they are all the
/// same object, so there is one description of it. A strand is claimed, written
/// per frame, and dropped; releasing zeroes its rows, which is also how it is
/// switched off, since a zero radius puts every vertex of that strand on one
/// point and its triangles have no area.
pub struct WaterBody {
    /// Three rows per strand, in the layout the shape library reads.
    texels: Vec<f32>,
    /// (profile, milkiness, alpha, live column count) per strand.
    params: [[f32; 4]; STRAND_MAX],
    used: [bool; STRAND_MAX],
    pub clock: f32,
    live: usize,
}

impl Default for WaterBody {
    fn default() -> Self {
        Self {
            texels: vec![0.0; STRAND_COLS * STRAND_MAX * 3 * 4],
            params: [[0.0; 4]; STRAND_MAX],
            used: [false; STRAND_MAX],
            clock: 0.0,
            live: 0,
        }
    }
}

/// Claims a strand, or nothing when the pool is exhausted.
pub fn acquire(water: &mut WaterBody) -> Option<usize> {
    for strand in 0..STRAND_MAX {
        if !water.used[strand] {
            water.used[strand] = true;
            clear(water, strand);
            return Some(strand);
        }
    }
    None
}

pub fn release(water: &mut WaterBody, strand: usize) {
    if strand >= STRAND_MAX {
        return;
    }
    water.used[strand] = false;
    clear(water, strand);
}

/// Zeroes a strand's rows and parameters.
pub fn clear(water: &mut WaterBody, strand: usize) {
    let base = strand * 3 * STRAND_COLS * 4;
    water.texels[base..base + STRAND_COLS * 3 * 4].fill(0.0);
    water.params[strand] = [0.0; 4];
}

/// Per-strand constants for this frame.
pub fn set_params(
    water: &mut WaterBody,
    strand: usize,
    profile: f32,
    milkiness: f32,
    alpha: f32,
    count: usize,
) {
    water.params[strand] = [
        profile,
        milkiness,
        alpha,
        if count < 2 {
            0.0
        } else {
            count.min(STRAND_COLS) as f32
        },
    ];
}

/// Writes one spine sample.
///
/// The reference right does not have to be exactly perpendicular to the
/// tangent, since the shader re-orthogonalises, but it does have to be
/// transported. The radius must taper to nearly nothing at both ends.
#[allow(clippy::too_many_arguments)]
pub fn column(
    water: &mut WaterBody,
    strand: usize,
    column: usize,
    position: [f32; 3],
    radius: f32,
    right: [f32; 3],
    twist: f32,
    distance: f32,
    age: f32,
    foam: f32,
    flatten: f32,
) {
    if column >= STRAND_COLS {
        return;
    }
    let width = STRAND_COLS * 4;
    let row = strand * 3;
    let mut offset = row * width + column * 4;
    water.texels[offset..offset + 4].copy_from_slice(&[
        position[0],
        position[1],
        position[2],
        radius,
    ]);
    offset += width;
    water.texels[offset..offset + 4].copy_from_slice(&[right[0], right[1], right[2], twist]);
    offset += width;
    water.texels[offset..offset + 4].copy_from_slice(&[distance, age, foam, flatten]);
}

/// Counts what is worth drawing. Called after every spell has written.
pub fn end_frame(water: &mut WaterBody, delta_time: f32) {
    water.clock += delta_time;
    water.live = water
        .params
        .iter()
        .filter(|params| params[2] > 0.003 && params[3] >= 2.0)
        .count();
}

pub fn visible(water: &WaterBody) -> bool {
    water.live > 0
}

pub fn live_strands(water: &WaterBody) -> usize {
    water.live
}

pub fn params(water: &WaterBody) -> [[f32; 4]; STRAND_MAX] {
    water.params
}

pub fn texels(water: &WaterBody) -> &[f32] {
    &water.texels
}
