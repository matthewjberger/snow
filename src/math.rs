use nalgebra_glm::Vec3;

/// Framerate-independent exponential approach.
pub fn exp_damp(current: f32, target: f32, rate: f32, delta_time: f32) -> f32 {
    target + (current - target) * (-rate * delta_time).exp()
}

/// Semi-implicit damped spring toward `target`, mutating `position` and `velocity`.
pub fn spring_damp(
    position: &mut Vec3,
    velocity: &mut Vec3,
    target: &Vec3,
    frequency: f32,
    damping: f32,
    delta_time: f32,
) {
    let stiffness = frequency * frequency;
    let drag = 2.0 * damping * frequency;
    let step = delta_time.min(1.0 / 45.0);
    velocity.x += (stiffness * (target.x - position.x) - drag * velocity.x) * step;
    velocity.y += (stiffness * (target.y - position.y) - drag * velocity.y) * step;
    velocity.z += (stiffness * (target.z - position.z) - drag * velocity.z) * step;
    position.x += velocity.x * step;
    position.y += velocity.y * step;
    position.z += velocity.z * step;
}

/// Shortest signed delta from `from` to `to`, wrapped to [-PI, PI].
pub fn angle_delta(from: f32, to: f32) -> f32 {
    let mut delta = to - from;
    while delta > std::f32::consts::PI {
        delta -= std::f32::consts::TAU;
    }
    while delta < -std::f32::consts::PI {
        delta += std::f32::consts::TAU;
    }
    delta
}

/// Framerate-independent easing across the shortest arc.
pub fn angle_damp(current: f32, target: f32, rate: f32, delta_time: f32) -> f32 {
    current + angle_delta(current, target) * (1.0 - (-rate * delta_time).exp())
}

pub fn clamp01(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}

fn hash1(value: f32) -> f32 {
    let scaled = (value * 127.1).sin() * 43758.547;
    scaled - scaled.floor()
}

/// Smooth 1D value noise, deterministic and allocation free.
pub fn noise1(value: f32) -> f32 {
    let cell = value.floor();
    let fraction = value - cell;
    let smooth = fraction * fraction * (3.0 - 2.0 * fraction);
    hash1(cell) * (1.0 - smooth) + hash1(cell + 1.0) * smooth
}

/// Van der Corput radical inverse, the generator behind the Halton jitter.
fn radical_inverse(index: u32, base: u32) -> f32 {
    let mut fraction = 1.0;
    let mut result = 0.0;
    let mut remaining = index;
    while remaining > 0 {
        fraction /= base as f32;
        result += fraction * (remaining % base) as f32;
        remaining /= base;
    }
    result
}

/// Halton(2, 3) on [-0.5, 0.5].
pub fn halton_sequence<const N: usize>() -> [(f32, f32); N] {
    let mut out = [(0.0, 0.0); N];
    for (index, entry) in out.iter_mut().enumerate() {
        let sample = index as u32 + 1;
        *entry = (
            radical_inverse(sample, 2) - 0.5,
            radical_inverse(sample, 3) - 0.5,
        );
    }
    out
}
