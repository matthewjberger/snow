#define_import_path snow::snow_uniforms

#import snow::shadow_lookup::ShadowParams
#import snow::spell_lights::SpellLights

// The terrain's uniform block, shared by the beauty pass, the depth prepass and the
// shadow cascades so the four cannot disagree about where a vertex goes.
struct SnowUniforms {
    // Beauty and prepass carry the jittered camera matrix; each cascade carries / its
    // own light matrix.
    viewProjection: mat4x4f,
    // (camera position, 0)
    camera: vec4f,
    // (ring centre xz, base spacing, grid half extent)
    clipmap: vec4f,
    // (world origin xz, world size, height resolution)
    field: vec4f,
    // (wind angle, macro amplitude, sastrugi amplitude, detail strength)
    surface: vec4f,
    // (glint intensity, glint gate, subsurface strength, subsurface radius)
    snow: vec4f,
    // (fog density, height falloff, fog start, aerial strength)
    fog: vec4f,
    // (deform centre xz, deform size, deform texel)
    deform: vec4f,
    // (deform depth scale, ambient intensity, debug mode, wireframe)
    misc: vec4f,
    // (render target size in pixels, 0, 0)
    screen: vec4f,
    // (direction toward the sun, 0)
    sunDir: vec4f,
    // (direct solar irradiance at the ground, 0)
    sunRadiance: vec4f,
    // (live spine samples, lattice columns, lattice rows, wake clock)
    wake: vec4f,
    // (lattice columns, section rings, clock, depth tint)
    water: vec4f,
    // Per strand: (profile, milkiness, alpha, live column count)
    strands: array<vec4f, 8>,
    // The camera's world-space right and up, which the spray billboards face.
    billboard: array<vec4f, 2>,
    harmonics: array<vec4f, 9>,
    shadow: ShadowParams,
    lights: SpellLights,
}
