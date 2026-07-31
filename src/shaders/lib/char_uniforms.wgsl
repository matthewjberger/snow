#define_import_path snow::char_uniforms

#import snow::shadow_lookup::ShadowParams
#import snow::spell_lights::SpellLights

// The character's uniform block, shared by the body, the garments, the fur and /
// their depth variants, so the four cannot disagree about where a vertex goes / or what
// light is falling on it.
struct CharUniforms {
    // The beauty pass and the prepass carry the jittered camera matrix; each /
    // cascade carries its own light matrix.
    viewProjection: mat4x4f,
    // (camera position, 0)
    camera: vec4f,
    // (direction toward the sun, 0)
    sunDir: vec4f,
    // (direct solar irradiance at the ground, 0)
    sunRadiance: vec4f,
    // (fog density, height falloff, fog start, aerial strength)
    fog: vec4f,
    // (ambient intensity, subsurface strength, weave threads per metre, 0)
    misc: vec4f,
    // (world displacement applied to a strand tip, strand cells per metre)
    fur: vec4f,
    // (fur colour, 0)
    furColor: vec4f,
    // (render target size in pixels, 0, 0)
    screen: vec4f,
    harmonics: array<vec4f, 9>,
    // Per material slot: rgb albedo, a base roughness.
    matAlbedo: array<vec4f, 8>,
    // Per material slot: sheen, anisotropy, transmission, weave depth.
    matParams: array<vec4f, 8>,
    // Per garment panel: (first texture row, columns, rows, 0).
    panels: array<vec4f, 6>,
    shadow: ShadowParams,
    lights: SpellLights,
}
