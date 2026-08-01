#define_import_path snow::spell_lights

#import snow::noise::PI
#import snow::shading::{
    wrapDiffuse, snowSubsurface, distributionGGX, visSmithGGXCorrelated, fresnelSchlick
}

// The light a spell puts into the snow.

const SPELL_LIGHT_MAX: i32 = 4;

// The pool as a material sees it: xyz world position with the radius in w, and / rgb
// colour with the intensity in w.
struct SpellLights {
    positions: array<vec4f, 4>,
    colors: array<vec4f, 4>,
    // (live count, 0, 0, 0)
    count: vec4f,
}

// Windowed inverse square.
fn spellAttenuation(dist2: f32, radius: f32) -> f32 {
    let t2 = dist2 / max(radius * radius, 1e-4);
    if (t2 >= 1.0) { return 0.0; }
    let win = 1.0 - t2 * t2;
    return win * win / (dist2 + 0.25);
}

// Snow's full response to the spell lights: wrapped diffuse plus transmission.
fn spellLighting(
    lights: SpellLights,
    world: vec3f,
    N: vec3f,
    V: vec3f,
    albedo: vec3f,
    thickness: f32,
    sssStrength: f32,
    sssRadius: f32
) -> vec3f {
    var acc = vec3f(0.0);
    let n = i32(lights.count.x);

    for (var i = 0; i < SPELL_LIGHT_MAX; i++) {
        if (i >= n) { break; }

        let p = lights.positions[i];
        let d = p.xyz - world;
        let dist2 = dot(d, d);
        let att = spellAttenuation(dist2, p.w);
        if (att <= 0.0) { continue; }

        let L = d * inverseSqrt(max(dist2, 1e-8));
        let radiance = lights.colors[i].rgb * lights.colors[i].w * att;

        acc += albedo * (1.0 / PI) * wrapDiffuse(dot(N, L), 0.66) * radiance;
        acc += snowSubsurface(N, L, V, radiance, thickness, sssStrength, sssRadius) * albedo;
    }

    return acc;
}

// The same lights, for the other surfaces: fabric, fur, water, ice.
fn spellLightingSurface(
    lights: SpellLights,
    world: vec3f,
    N: vec3f,
    V: vec3f,
    albedo: vec3f,
    f0: vec3f,
    roughness: f32,
    wrap: f32
) -> vec3f {
    var acc = vec3f(0.0);
    let n = i32(lights.count.x);
    let NdotV = clamp(dot(N, V), 1e-4, 1.0);

    for (var i = 0; i < SPELL_LIGHT_MAX; i++) {
        if (i >= n) { break; }

        let p = lights.positions[i];
        let d = p.xyz - world;
        let dist2 = dot(d, d);
        let att = spellAttenuation(dist2, p.w);
        if (att <= 0.0) { continue; }

        let L = d * inverseSqrt(max(dist2, 1e-8));
        let radiance = lights.colors[i].rgb * lights.colors[i].w * att;

        acc += albedo * (1.0 / PI) * wrapDiffuse(dot(N, L), wrap) * radiance;

        let NdotL = dot(N, L);
        if (NdotL > 0.0) {
            let H = normalize(V + L);
            let D = distributionGGX(clamp(dot(N, H), 0.0, 1.0), roughness);
            let Vis = visSmithGGXCorrelated(NdotV, NdotL, roughness);
            let F = fresnelSchlick(clamp(dot(V, H), 0.0, 1.0), f0);
            acc += radiance * D * Vis * F * NdotL;
        }
    }

    return acc;
}

// Lights on airborne snow: a billboarded grain has no thickness worth / modelling, so
// this is a single wide-wrap term.
fn spellLightingParticle(
    lights: SpellLights,
    world: vec3f,
    N: vec3f,
    albedo: vec3f
) -> vec3f {
    var acc = vec3f(0.0);
    let n = i32(lights.count.x);

    for (var i = 0; i < SPELL_LIGHT_MAX; i++) {
        if (i >= n) { break; }

        let p = lights.positions[i];
        let d = p.xyz - world;
        let dist2 = dot(d, d);
        let att = spellAttenuation(dist2, p.w);
        if (att <= 0.0) { continue; }

        let L = d * inverseSqrt(max(dist2, 1e-8));
        acc += albedo * (1.0 / PI) * wrapDiffuse(dot(N, L), 0.8)
             * lights.colors[i].rgb * lights.colors[i].w * att;
    }

    return acc;
}
