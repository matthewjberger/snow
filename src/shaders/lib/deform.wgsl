#define_import_path snow::deform

// The read side of the terrain state buffer.

// World XZ to deformation-buffer UV. The sampler must be in wrap mode.
fn deformUV(worldXZ: vec2f, size: f32) -> vec2f {
    return fract(worldXZ / size);
}

// How much of the buffer's authority applies here, 0..1.
fn deformFalloff(worldXZ: vec2f, centre: vec2f, size: f32) -> f32 {
    let d = abs(worldXZ - centre) / (size * 0.5);
    return 1.0 - smoothstep(0.80, 0.96, max(d.x, d.y));
}

// Net height offset in metres: piled mass minus depression, band-limited to / what a
// lattice of `spacing` metres can actually carry.
fn deformHeight(
    tex: texture_2d<f32>, samp: sampler,
    worldXZ: vec2f, centre: vec2f, size: f32, scale: f32, spacing: f32
) -> f32 {
    let w = deformFalloff(worldXZ, centre, size);
    if (w <= 0.0) { return 0.0; }

    let base = deformUV(worldXZ, size);
    let r = spacing / size;

    var acc = 0.0;
    for (var j = -1; j <= 1; j++) {
        for (var i = -1; i <= 1; i++) {
            // Binomial [1,2,1] x [1,2,1] / 16.
            let wt = f32((2 - abs(i)) * (2 - abs(j))) * (1.0 / 16.0);
            let uv = base + vec2f(f32(i), f32(j)) * r;
            let s = textureSampleLevel(tex, samp, uv, 0.0);
            acc += (s.g - s.r) * wt;
        }
    }
    return acc * scale * w;
}
