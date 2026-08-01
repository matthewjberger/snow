#import snow::snow_uniforms::SnowUniforms
#import snow::noise::{noise2, ign}
#import snow::crystal::crystalPoint
#import snow::shading::{
    wrapDiffuse, snowSubsurface, snowGlints, shIrradiance, backScatter,
    distributionGGX, visSmithGGXCorrelated, fresnelSchlick
}
#import snow::atmosphere::{applyAerial, dirToLatLong}
#import snow::shadow_lookup::sunShadow
#import snow::spell_lights::spellLightingSurface

// The ice formations Crystallise grows.
//
// No normal is emitted from the vertex stage. The fragment stage takes it from
// the derivatives of the world position, which gives exact flat facets for free,
// and a facet is what an ice crystal is. Interpolated vertex normals would round
// the edges off and turn a crystal into a lumpy cone.

@group(0) @binding(0) var<uniform> uniforms: SnowUniforms;

@group(1) @binding(0) var crystalTex: texture_2d<f32>;
@group(1) @binding(1) var skyLUT: texture_2d<f32>;
@group(1) @binding(2) var skySamp: sampler;
@group(1) @binding(3) var cascade0: texture_2d<f32>;
@group(1) @binding(4) var cascade1: texture_2d<f32>;
@group(1) @binding(5) var cascade2: texture_2d<f32>;
@group(1) @binding(6) var cascadeSamp: sampler;

const ICE_ABSORB: vec3f = vec3f(2.35, 0.60, 0.24);

struct VertexInput {
    // (crystal index, vertex index, unused)
    @location(0) lattice: vec3f,
}

struct Varyings {
    @builtin(position) clip: vec4f,
    @location(0) world: vec3f,
    /// Fraction of the way up the crystal: the base is buried in the drift and
    /// milky, the tip is clear and lit through.
    @location(1) height01: f32,
    @location(2) seed: f32,
    @location(3) viewDist: f32,
}

@vertex
fn vertexMain(input: VertexInput) -> Varyings {
    let index = i32(input.lattice.x);
    let vertex = i32(input.lattice.y);

    let place = textureLoad(crystalTex, vec2i(index, 0), 0);
    let state = textureLoad(crystalTex, vec2i(index, 2), 0);
    let point = crystalPoint(crystalTex, index, vertex);

    var out: Varyings;
    out.world = point;
    out.height01 = clamp((point.y - place.y) / max(place.w, 1e-3), 0.0, 1.0);
    out.seed = state.y;
    out.viewDist = distance(point, uniforms.camera.xyz);
    out.clip = uniforms.viewProjection * vec4f(point, 1.0);
    return out;
}

@fragment
fn fragmentMain(input: Varyings) -> @location(0) vec4f {
    let world = input.world;
    let view = normalize(uniforms.camera.xyz - world);
    let sun = uniforms.sunDir.xyz;

    // Flat facet normal, from the geometry itself.
    let dx = dpdx(world);
    let dy = dpdy(world);
    var normal = normalize(cross(dx, dy));
    if (dot(normal, view) < 0.0) { normal = -normal; }
    let shadowNormal = normal;

    let ndotv = clamp(dot(normal, view), 1e-4, 1.0);
    let ndotl = dot(normal, sun);
    let rotation = ign(input.clip.xy) * 6.28318530718;
    let shadow = sunShadow(
        cascade0, cascadeSamp, cascade1, cascadeSamp, cascade2, cascadeSamp,
        uniforms.shadow, world, shadowNormal, input.viewDist, rotation
    );

    let radiance = uniforms.sunRadiance.xyz;
    const INV_PI: f32 = 0.31830988618;

    // Where the crystal comes out of the drift it is packed with the snow it
    // grew through, and that gradient is what attaches it to the ground. Confined
    // to the bottom fifth: any more and it is a white prism with a clear tip
    // rather than an ice prism standing in snow.
    let grain = noise2(world.xz * 34.0 + input.seed * 19.0) * 0.5 + 0.5;
    let frost = clamp(
        (1.0 - smoothstep(0.01, 0.22, input.height01)) * (0.45 + 0.6 * grain),
        0.0,
        1.0
    );

    // Optical path: long across a facet seen edge on, short through one seen face
    // on, and longer near the thick base. The constant term carries the colour
    // through the middle of the prism, because a path that only opens up at
    // grazing puts all of the blue on the silhouette, where the reflection then
    // replaces it with sky.
    let path = clamp(
        (0.16 + 0.42 * (1.0 - input.height01)) * (0.7 + 2.0 * (1.0 - ndotv)),
        0.02,
        1.4
    );
    let transmit = exp(-ICE_ABSORB * path);

    let mirror = reflect(-view, normal);
    let bentRed = refract(-view, normal, 1.0 / 1.3050);
    let bentGreen = refract(-view, normal, 1.0 / 1.3090);
    let bentBlue = refract(-view, normal, 1.0 / 1.3170);
    let behind = vec3f(
        textureSampleLevel(
            skyLUT, skySamp,
            dirToLatLong(select(mirror, bentRed, dot(bentRed, bentRed) > 0.5)), 0.9
        ).r,
        textureSampleLevel(
            skyLUT, skySamp,
            dirToLatLong(select(mirror, bentGreen, dot(bentGreen, bentGreen) > 0.5)), 0.9
        ).g,
        textureSampleLevel(
            skyLUT, skySamp,
            dirToLatLong(select(mirror, bentBlue, dot(bentBlue, bentBlue) > 0.5)), 0.9
        ).b
    );
    var color = behind * transmit;

    // A crystal with the sun behind it lights along its whole length: the light
    // enters the far facet, scatters off inclusions, and leaves toward the eye,
    // tinted by everything it did not survive.
    let through = backScatter(normal, sun, view, 0.42, 2.2, 1.0);
    let deepTint =
        mix(vec3f(0.42, 0.74, 1.0), vec3f(0.86, 0.95, 1.0), exp(-path * 2.5));
    color += radiance * INV_PI * deepTint * through * uniforms.snow.z * 1.6
        * mix(0.25, 1.0, shadow);

    // Sky through the body, which keeps a crystal standing in shadow alive
    // rather than black.
    color += shIrradiance(normal, uniforms.harmonics) * uniforms.misc.y * INV_PI
        * deepTint * 0.9;

    if (frost > 0.002) {
        let frostAlbedo = vec3f(0.88, 0.915, 0.965);
        var skin = frostAlbedo * INV_PI * radiance * wrapDiffuse(ndotl, 0.62) * shadow;
        skin += frostAlbedo * INV_PI * shIrradiance(normal, uniforms.harmonics)
            * uniforms.misc.y;
        skin += snowSubsurface(normal, sun, view, radiance, 0.4, uniforms.snow.z, 1.3)
            * frostAlbedo * mix(0.4, 1.0, shadow);
        color = mix(color, skin, frost * 0.9);
    }

    let roughness = mix(0.045, 0.42, frost);
    let fresnel = fresnelSchlick(ndotv, vec3f(0.021));
    let skyReflection =
        textureSampleLevel(skyLUT, skySamp, dirToLatLong(mirror), roughness * 6.0).rgb;
    color = mix(color, skyReflection, fresnel * (1.0 - frost * 0.75));

    if (ndotl > 0.0) {
        let half = normalize(view + sun);
        let distribution = distributionGGX(clamp(dot(normal, half), 0.0, 1.0), roughness);
        let visibility = visSmithGGXCorrelated(ndotv, ndotl, roughness);
        let surface = fresnelSchlick(clamp(dot(view, half), 0.0, 1.0), vec3f(0.021));
        color += radiance * distribution * visibility * surface * ndotl * shadow;
    }

    if (uniforms.snow.x > 0.001) {
        let glints = snowGlints(
            world.xz, normal, view, sun,
            max(length(dx.xz) + length(dy.xz), 1e-4),
            uniforms.snow.x * (0.4 + 1.2 * frost),
            uniforms.snow.y
        );
        color += radiance * glints * shadow * 0.6;
    }

    if (uniforms.lights.count.x > 0.5) {
        color += spellLightingSurface(
            uniforms.lights, world, normal, view,
            mix(vec3f(0.3, 0.6, 0.85), vec3f(0.88), frost),
            vec3f(0.021), roughness, 0.5
        );
    }

    color = applyAerial(
        color, uniforms.camera.xyz, world, -view, sun,
        skyLUT, skySamp, radiance,
        uniforms.fog.x, uniforms.fog.y, uniforms.fog.z, uniforms.fog.w
    );

    // Three things decide how much of a real crystal you can see through: the
    // path, since a thin tip is nearly clear and a thick base is dense; the grazing
    // angle, since a facet seen edge on presents a long path and a strong
    // reflection; and the frost, where the prism is packed with the snow it grew
    // through and reads as solid. The floor is high enough to keep a crystal
    // legible against the field behind it.
    let alpha = clamp(
        0.46 + 0.34 * (1.0 - exp(-path * 2.2)) + 0.26 * (1.0 - ndotv) + frost * 0.55,
        0.0,
        1.0
    );
    return vec4f(color, alpha);
}
