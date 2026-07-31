#define_import_path snow::post_uniforms

// Everything the screen-space chain derives from the camera and the settings, /
// written once a frame and bound by every stage.
struct PostUniforms {
    // Last frame's view-projection, unjittered.
    prevViewProj: mat4x4f,
    // This frame's view to world.
    invView: mat4x4f,
    projection: vec4f,
    temporal: vec4f,
    // (where the sun lands on screen, whether it is in front, aspect ratio)
    sun: vec4f,
    // Sun radiance, with the shaft strength in w.
    sunColor: vec4f,
    // (exposure, contrast, display transform, grain amount)
    tone: vec4f,
    // (seconds, vignette, speed streak, bloom amount)
    look: vec4f,
    focus: vec4f,
    // (reflections on, temporal resolve on, sharpen amount, reflection strength)
    toggles: vec4f,
}

// The bloom levels differ only in where they sample from and whether they /
// threshold, so they carry their own small block rather than three copies of /
// everything above.
struct BloomUniforms {
    // (one texel of the source in uv, times the spread; the threshold switch)
    source: vec4f,
    // Knee curve: (threshold, threshold minus knee, twice the knee, / a quarter over
    // the knee).
    curve: vec4f,
}
