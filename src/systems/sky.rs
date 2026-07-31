use crate::constants::{SKY_SH_HEIGHT, SKY_SH_WIDTH};
use crate::settings::Settings;
use nalgebra_glm::Vec3;

/// The four sliders the sun solve reads, lifted out so the solve does not have to
/// borrow the whole settings store while the rest of the state is in hand.
pub struct SunSettings {
    pub azimuth: f32,
    pub elevation: f32,
    pub intensity: f32,
    pub warmth: f32,
}

pub fn sun_settings(settings: &Settings) -> SunSettings {
    SunSettings {
        azimuth: settings.sun_azimuth,
        elevation: settings.sun_elevation,
        intensity: settings.sun_intensity,
        warmth: settings.sun_temp_warm,
    }
}

/// Converts the sun intensity slider into the shared radiometric scale used by both the
/// sky integral and the direct sun.
const SUN_SCALE_BASE: f32 = 5.5;

/// Fresh snow reflects most of what hits it, slightly more at the blue end.
const SNOW_ALBEDO: [f32; 3] = [0.83, 0.86, 0.91];

/// Procedural sky, image-based lighting and the sun's own colour.
pub struct Sky {
    /// Unit vector pointing toward the sun.
    pub sun_direction: Vec3,
    /// Normalised hue of direct sunlight, for tinting effects.
    pub sun_color: Vec3,
    /// Direct solar irradiance reaching the ground, in the same units the sky LUT
    /// stores radiance in.
    pub sun_radiance: Vec3,
    /// Shared radiometric scale for the sun and the baked sky.
    pub sun_scale: f32,
    /// Radiance leaving the snow field, solved iteratively.
    pub ground_bounce: Vec3,
    /// Nine harmonic coefficients as vec4 rows, ready for a uniform block.
    pub harmonics: [[f32; 4]; 9],
    /// Set whenever the sun has moved far enough to need a re-bake.
    pub dirty: bool,
}

impl Default for Sky {
    fn default() -> Self {
        Self {
            sun_direction: Vec3::new(0.0, 0.2, 1.0),
            sun_color: Vec3::new(1.0, 0.85, 0.66),
            sun_radiance: Vec3::new(1.0, 1.0, 1.0),
            sun_scale: 1.0,
            ground_bounce: Vec3::zeros(),
            harmonics: [[0.0; 4]; 9],
            dirty: true,
        }
    }
}

/// Recomputes the sun vector and colour, and marks the LUT for a re-bake if
/// anything actually moved.
pub fn sync_from_settings(sky: &mut Sky, settings: &SunSettings) {
    let azimuth = settings.azimuth.to_radians();
    let elevation = settings.elevation.to_radians();
    let cos_elevation = elevation.cos();
    let direction = Vec3::new(
        azimuth.sin() * cos_elevation,
        elevation.sin(),
        azimuth.cos() * cos_elevation,
    );

    if (direction - sky.sun_direction).abs().max() > 1e-6 {
        sky.sun_direction = direction;
        sky.dirty = true;
    }

    sky.sun_scale = settings.intensity * SUN_SCALE_BASE;

    let zenith_degrees = sky.sun_direction.y.clamp(-1.0, 1.0).acos().to_degrees();

    let denominator = zenith_degrees.to_radians().cos()
        + 0.50572 * (96.07995 - zenith_degrees).max(1e-3).powf(-1.6364);
    let air_mass = if denominator > 0.0 {
        (1.0 / denominator).min(40.0)
    } else {
        40.0
    };

    let warm = settings.warmth;
    let tau_rayleigh = [0.0464_f32, 0.108, 0.265];
    let tau_mie = 0.0252_f32;
    let transmittance = Vec3::new(
        (-(tau_rayleigh[0] * warm + tau_mie) * air_mass).exp(),
        (-(tau_rayleigh[1] * warm + tau_mie) * air_mass).exp(),
        (-(tau_rayleigh[2] * warm + tau_mie) * air_mass).exp(),
    );

    sky.sun_radiance = transmittance * sky.sun_scale;

    let peak = transmittance.max().max(1e-6);
    sky.sun_color = transmittance / peak;
}

/// Radiance leaving the snow, from everything currently landing on it.
pub fn update_ground_bounce(sky: &mut Sky) {
    let up = irradiance_up(sky);
    let cosine = sky.sun_direction.y.max(0.0);
    let arriving = Vec3::new(
        sky.sun_radiance.x * cosine + up.x,
        sky.sun_radiance.y * cosine + up.y,
        sky.sun_radiance.z * cosine + up.z,
    );
    let scale = 1.0 / std::f32::consts::PI;
    sky.ground_bounce = Vec3::new(
        SNOW_ALBEDO[0] * arriving.x * scale,
        SNOW_ALBEDO[1] * arriving.y * scale,
        SNOW_ALBEDO[2] * arriving.z * scale,
    );
}

/// The sky harmonics with the cool shadow shift scaled, as every material's
/// ambient reads them.
///
/// A chroma control rather than a brightness one: each coefficient keeps its
/// luminance and only its distance from grey moves. Scaling the coefficients
/// outright would make this a second ambient slider sitting next to the one that
/// already is, and the two would fight. At zero the ambient is neutral and snow
/// shadows go grey; at one it is the sky as baked.
///
/// Applied here rather than in the bounce solve on purpose. The bounce is real
/// radiance feeding the atmosphere integral, so it stays as the sky computed it,
/// and this stays a shading control.
pub fn ambient_harmonics(sky: &Sky, cool_shift: f32) -> [[f32; 4]; 9] {
    let mut tinted = sky.harmonics;
    for coefficient in &mut tinted {
        let luminance = coefficient[0] * 0.2126 + coefficient[1] * 0.7152 + coefficient[2] * 0.0722;
        for channel in coefficient.iter_mut().take(3) {
            *channel = luminance + (*channel - luminance) * cool_shift;
        }
    }
    tinted
}

/// Harmonic irradiance for an up-facing normal: only the bands that survive a
/// normal of (0, 1, 0).
fn irradiance_up(sky: &Sky) -> Vec3 {
    let mut out = Vec3::zeros();
    for channel in 0..3 {
        out[channel] = sky.harmonics[0][channel] * 0.886227
            + sky.harmonics[1][channel] * 2.0 * 0.511664
            + sky.harmonics[6][channel] * -0.247708
            + sky.harmonics[8][channel] * -0.429043;
    }
    out
}

/// Projects the baked sky into nine harmonic coefficients on the CPU.
pub fn project_harmonics(sky: &mut Sky, pixels: &[f32]) {
    let width = SKY_SH_WIDTH as usize;
    let height = SKY_SH_HEIGHT as usize;
    if pixels.len() < width * height * 4 {
        return;
    }

    sky.harmonics = [[0.0; 4]; 9];

    let solid_angle =
        (std::f32::consts::TAU / width as f32) * (std::f32::consts::PI / height as f32);

    let mut basis = [0.0_f32; 9];
    for row in 0..height {
        let theta = ((row as f32 + 0.5) / height as f32) * std::f32::consts::PI;
        let sin_theta = theta.sin();
        let cos_theta = theta.cos();
        let weight = sin_theta * solid_angle;

        for column in 0..width {
            let phi = ((column as f32 + 0.5) / width as f32 - 0.5) * std::f32::consts::TAU;
            let x = sin_theta * phi.sin();
            let y = cos_theta;
            let z = sin_theta * phi.cos();

            basis[0] = 0.282095;
            basis[1] = 0.488603 * y;
            basis[2] = 0.488603 * z;
            basis[3] = 0.488603 * x;
            basis[4] = 1.092548 * x * y;
            basis[5] = 1.092548 * y * z;
            basis[6] = 0.315392 * (3.0 * z * z - 1.0);
            basis[7] = 1.092548 * x * z;
            basis[8] = 0.546274 * (x * x - y * y);

            let offset = (row * width + column) * 4;
            let radiance = [
                pixels[offset] * weight,
                pixels[offset + 1] * weight,
                pixels[offset + 2] * weight,
            ];

            for (coefficient, weight) in basis.iter().enumerate() {
                for (channel, value) in radiance.iter().enumerate() {
                    sky.harmonics[coefficient][channel] += value * weight;
                }
            }
        }
    }
}
