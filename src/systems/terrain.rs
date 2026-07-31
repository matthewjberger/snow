use crate::constants::{HEIGHT_RES, PLAY_RADIUS, WORLD_SIZE};
use nalgebra_glm::{Vec2, Vec3};

/// The CPU mirror of the baked macro heightfield.
pub struct Heightfield {
    /// Half-resolution copy of the height channel.
    heights: Vec<f32>,
    resolution: usize,
    texel: f32,
    pub origin: Vec2,
    pub size: f32,
    /// Measured relief, which the cascade fitter sizes its light volume from.
    pub min_height: f32,
    pub max_height: f32,
}

impl Default for Heightfield {
    fn default() -> Self {
        Self {
            heights: Vec::new(),
            resolution: 0,
            texel: 1.0,
            origin: Vec2::new(-WORLD_SIZE * 0.5, -WORLD_SIZE * 0.5),
            size: WORLD_SIZE,
            min_height: 0.0,
            max_height: 0.0,
        }
    }
}

/// Takes the readback of the height bake, in tightly packed RG pairs.
pub fn absorb_readback(heightfield: &mut Heightfield, pairs: &[f32]) {
    let source_resolution = HEIGHT_RES as usize;
    if pairs.len() < source_resolution * source_resolution * 2 {
        return;
    }

    let resolution = source_resolution / 2;
    let mut heights = vec![0.0_f32; resolution * resolution];
    for row in 0..resolution {
        let top = row * 2 * source_resolution;
        let bottom = (row * 2 + 1) * source_resolution;
        for column in 0..resolution {
            let left = column * 2;
            let right = left + 1;
            heights[row * resolution + column] = (pairs[(top + left) * 2]
                + pairs[(top + right) * 2]
                + pairs[(bottom + left) * 2]
                + pairs[(bottom + right) * 2])
                * 0.25;
        }
    }

    let mut lowest = f32::INFINITY;
    let mut highest = f32::NEG_INFINITY;
    for height in &heights {
        lowest = lowest.min(*height);
        highest = highest.max(*height);
    }

    heightfield.heights = heights;
    heightfield.resolution = resolution;
    heightfield.texel = heightfield.size / resolution as f32;
    heightfield.min_height = lowest;
    heightfield.max_height = highest;
}

/// Bicubic B-spline height lookup, matching the vertex shader's own reconstruction
/// so the ground the character stands on is the ground that is drawn.
pub fn height_at(heightfield: &Heightfield, x: f32, z: f32) -> f32 {
    if heightfield.heights.is_empty() {
        return 0.0;
    }
    let resolution = heightfield.resolution;
    let last = resolution as i32 - 1;

    let fx = ((x - heightfield.origin.x) / heightfield.size) * resolution as f32 - 0.5;
    let fz = ((z - heightfield.origin.y) / heightfield.size) * resolution as f32 - 0.5;

    let ix = fx.floor();
    let iz = fz.floor();
    let weights_x = bspline_weights(fx - ix);
    let weights_z = bspline_weights(fz - iz);
    let ix = ix as i32;
    let iz = iz as i32;

    let mut sum = 0.0;
    for (j, weight_z) in weights_z.iter().enumerate() {
        let row = (iz - 1 + j as i32).clamp(0, last) as usize * resolution;
        let mut row_sum = 0.0;
        for (i, weight_x) in weights_x.iter().enumerate() {
            let column = (ix - 1 + i as i32).clamp(0, last) as usize;
            row_sum += heightfield.heights[row + column] * weight_x;
        }
        sum += row_sum * weight_z;
    }
    sum
}

/// Surface normal from the same reconstruction, by central difference.
pub fn normal_at(heightfield: &Heightfield, x: f32, z: f32) -> Vec3 {
    let step = heightfield.texel.max(1e-3);
    let dx = height_at(heightfield, x + step, z) - height_at(heightfield, x - step, z);
    let dz = height_at(heightfield, x, z + step) - height_at(heightfield, x, z - step);
    Vec3::new(-dx / (2.0 * step), 1.0, -dz / (2.0 * step)).normalize()
}

/// Clamps a world position to the playable area, in place.
pub fn clamp_to_play_area(position: &mut Vec3) {
    let distance = (position.x * position.x + position.z * position.z).sqrt();
    if distance > PLAY_RADIUS {
        let scale = PLAY_RADIUS / distance;
        position.x *= scale;
        position.z *= scale;
    }
}

fn bspline_weights(t: f32) -> [f32; 4] {
    let t2 = t * t;
    let t3 = t2 * t;
    [
        (1.0 - 3.0 * t + 3.0 * t2 - t3) / 6.0,
        (4.0 - 6.0 * t2 + 3.0 * t3) / 6.0,
        (1.0 + 3.0 * t + 3.0 * t2 - 3.0 * t3) / 6.0,
        t3 / 6.0,
    ]
}
