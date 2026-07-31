use crate::camera::{Z_FAR, Z_NEAR};
use crate::constants::{CASCADE_COUNT, CASCADE_RESOLUTION, CASCADE_SPLITS};
use nalgebra_glm::{Mat4, Vec3, Vec4};

/// NDC cube corners.
const NDC: [[f32; 3]; 8] = [
    [-1.0, -1.0, 0.0],
    [1.0, -1.0, 0.0],
    [1.0, 1.0, 0.0],
    [-1.0, 1.0, 0.0],
    [-1.0, -1.0, 1.0],
    [1.0, -1.0, 1.0],
    [1.0, 1.0, 1.0],
    [-1.0, 1.0, 1.0],
];

/// Cascaded shadow maps, fitted on the CPU.
pub struct Shadows {
    pub matrices: [Mat4; CASCADE_COUNT],
    /// Per cascade: (depth range in metres, ortho width in metres, 0, 0).
    pub params: [[f32; 4]; CASCADE_COUNT],
    pub splits: [f32; 4],
    pub texel_size: f32,

    light_direction: Vec3,
    /// World height range the casters occupy, which the light volume's depth solve
    /// needs.
    min_height: f32,
    max_height: f32,
    /// Slack on the cascade's lateral extent, covering the texel snap.
    texel_world_pad: f32,
}

impl Default for Shadows {
    fn default() -> Self {
        Self {
            matrices: [Mat4::identity(); CASCADE_COUNT],
            params: [[1.0, 1.0, 0.0, 0.0]; CASCADE_COUNT],
            splits: [
                CASCADE_SPLITS[0],
                CASCADE_SPLITS[1],
                CASCADE_SPLITS[2],
                CASCADE_SPLITS[CASCADE_COUNT - 1],
            ],
            texel_size: 1.0 / CASCADE_RESOLUTION as f32,
            light_direction: Vec3::new(0.0, -1.0, 0.0),
            min_height: -60.0,
            max_height: 60.0,
            texel_world_pad: 2.0,
        }
    }
}

/// Tells the fitter how tall the world actually is.
pub fn set_height_bounds(shadows: &mut Shadows, min_height: f32, max_height: f32) {
    shadows.min_height = min_height;
    shadows.max_height = max_height;
}

/// Refits every cascade to the current camera frustum and sun direction.
pub fn update(shadows: &mut Shadows, view_projection: &Mat4, sun_direction: &Vec3) {
    shadows.light_direction = -sun_direction.normalize();

    let Some(inverse) = view_projection.try_inverse() else {
        return;
    };

    let mut slice_near = Z_NEAR;
    for (cascade, slice_far) in CASCADE_SPLITS.into_iter().enumerate() {
        fit(shadows, cascade, &inverse, slice_near, slice_far);
        slice_near = slice_far * 0.88;
    }
}

fn fit(
    shadows: &mut Shadows,
    cascade: usize,
    inverse_view_projection: &Mat4,
    slice_near: f32,
    slice_far: f32,
) {
    let mut corners: [Vec3; 8] = std::array::from_fn(|index| {
        let ndc = NDC[index];
        let projected = inverse_view_projection * Vec4::new(ndc[0], ndc[1], ndc[2], 1.0);
        projected.xyz() / projected.w
    });
    for index in 0..4 {
        let near_corner = corners[index];
        let far_corner = corners[index + 4];
        let edge = far_corner - near_corner;
        let length = edge.norm();
        if length < 1e-6 {
            continue;
        }
        let direction = edge / length;
        let t0 = (slice_near - Z_NEAR) / (Z_FAR - Z_NEAR);
        let t1 = (slice_far - Z_NEAR) / (Z_FAR - Z_NEAR);
        corners[index + 4] = near_corner + direction * (length * t1);
        corners[index] = near_corner + direction * (length * t0);
    }

    let mut center = Vec3::zeros();
    for corner in &corners {
        center += corner;
    }
    center /= 8.0;

    let mut radius = 0.0_f32;
    for corner in &corners {
        radius = radius.max((center - corner).norm());
    }

    radius = radius.max(0.5);
    let quantum = (2.0_f32).powf(radius.log2().ceil() - 8.0);
    radius = (radius / quantum).ceil() * quantum;

    let up = if shadows.light_direction.y.abs() > 0.995 {
        Vec3::new(0.0, 0.0, 1.0)
    } else {
        Vec3::new(0.0, 1.0, 0.0)
    };

    let right = up.cross(&shadows.light_direction).normalize();
    let light_up = shadows.light_direction.cross(&right);

    let texel_world = (radius * 2.0) / CASCADE_RESOLUTION as f32;
    let snapped_right = (center.dot(&right) / texel_world).floor() * texel_world;
    let snapped_up = (center.dot(&light_up) / texel_world).floor() * texel_world;
    let along_light = center.dot(&shadows.light_direction);
    center = right * snapped_right + light_up * snapped_up + shadows.light_direction * along_light;

    let forward_y = shadows.light_direction.y.min(-0.0349);
    let relief = radius + shadows.texel_world_pad;

    let mut depth_min = f32::INFINITY;
    let mut depth_max = f32::NEG_INFINITY;
    for corner in 0..4 {
        let vertical = if corner < 2 { -relief } else { relief };
        let height = if corner % 2 == 0 {
            shadows.min_height
        } else {
            shadows.max_height
        };
        let depth = (height - center.y - vertical * light_up.y) / forward_y;
        depth_min = depth_min.min(depth);
        depth_max = depth_max.max(depth);
    }

    const MARGIN: f32 = 12.0;
    let backoff = MARGIN - depth_min;
    let eye = center - shadows.light_direction * backoff;

    let view = nalgebra_glm::look_at_lh(&eye, &center, &up);
    let near = MARGIN * 0.5;
    let far = backoff + depth_max + MARGIN;
    let projection = nalgebra_glm::ortho_lh_zo(-radius, radius, -radius, radius, near, far);

    shadows.matrices[cascade] = projection * view;
    shadows.params[cascade] = [far - near, radius * 2.0, 0.0, 0.0];
}
