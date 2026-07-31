#import snow::snow_uniforms::SnowUniforms
#import snow::noise::{noise2, ign}
#import snow::shading::{wrapDiffuse, shIrradiance}
#import snow::atmosphere::{phaseMie, applyAerial}
#import snow::shadow_lookup::sunShadow
#import snow::spell_lights::spellLightingParticle

// Snow spray.

@group(0) @binding(0) var<uniform> uniforms: SnowUniforms;

@group(1) @binding(0) var sprayTex: texture_2d<f32>;
@group(1) @binding(1) var skyLUT: texture_2d<f32>;
@group(1) @binding(2) var skySamp: sampler;
@group(1) @binding(3) var cascade0: texture_2d<f32>;
@group(1) @binding(4) var cascade1: texture_2d<f32>;
@group(1) @binding(5) var cascade2: texture_2d<f32>;
@group(1) @binding(6) var cascadeSamp: sampler;

struct VertexInput {
    // (particle index, corner across, corner down)
    @location(0) quad: vec3f,
}

struct Varyings {
    @builtin(position) clip: vec4f,
    @location(0) world: vec3f,
    @location(1) corner: vec2f,
    // (aged fraction, seed, kind, opacity)
    @location(2) state: vec4f,
    @location(3) viewDist: f32,
}

@vertex
fn vertexMain(input: VertexInput) -> Varyings {
    let index = i32(input.quad.x);
    let corner = input.quad.yz;

    let place = textureLoad(sprayTex, vec2i(index, 0), 0);
    let state = textureLoad(sprayTex, vec2i(index, 1), 0);

    // A dead grain has a zero radius, which collapses all four corners onto one point.
    let radius = place.w;

    // Spin, hashed off the seed, so a burst is not four hundred identical discs.
    let angle = state.y * 6.28318530718 + state.x * (state.y - 0.5) * 3.0;
    let spun = vec2f(
        corner.x * cos(angle) - corner.y * sin(angle),
        corner.x * sin(angle) + corner.y * cos(angle)
    );

    let camRight = normalize(uniforms.billboard[0].xyz);
    let camUp = normalize(uniforms.billboard[1].xyz);
    let world = place.xyz + (camRight * spun.x + camUp * spun.y) * radius;

    var out: Varyings;
    out.world = world;
    out.corner = corner;
    out.state = state;
    out.viewDist = distance(world, uniforms.camera.xyz);
    out.clip = uniforms.viewProjection * vec4f(world, 1.0);
    return out;
}

@fragment
fn fragmentMain(input: Varyings) -> @location(0) vec4f {
    let radiusSquared = dot(input.corner, input.corner);
    if (radiusSquared > 1.0) { discard; }

    let state = input.state;
    let kind = state.z;

    // Break the disc's edge.
    let around = atan2(input.corner.y, input.corner.x);
    let wobble = 1.0 + 0.34 * noise2(vec2f(cos(around), sin(around)) * 2.4 + state.y * 37.0);
    let radius = sqrt(radiusSquared) / wobble;
    if (radius > 1.0) { discard; }

    // Soft edged for powder, harder for a clod of thrown snow.
    let edge = mix(
        pow(clamp(1.0 - radius * radius, 0.0, 1.0), 1.6),
        1.0 - smoothstep(0.65, 1.0, radius),
        kind
    );
    // Powder is close to transparent on its own.
    let alpha = state.w * edge * mix(0.36, 0.55, kind);
    if (alpha < 0.004) { discard; }

    let world = input.world;
    let view = normalize(uniforms.camera.xyz - world);
    let sun = uniforms.sunDir.xyz;

    // Spherical normal from the billboard's own coordinates.
    let depth = sqrt(max(0.0, 1.0 - radiusSquared));
    let camRight = normalize(uniforms.billboard[0].xyz);
    let camUp = normalize(uniforms.billboard[1].xyz);
    let normal = normalize(camRight * input.corner.x + camUp * input.corner.y + view * depth);

    let rotation = ign(input.clip.xy) * 6.28318530718;
    let shadow = sunShadow(
        cascade0, cascadeSamp, cascade1, cascadeSamp, cascade2, cascadeSamp,
        uniforms.shadow, world, normal, input.viewDist, rotation
    );

    let radiance = uniforms.sunRadiance.xyz;
    const INV_PI: f32 = 0.31830988618;

    // Snow crystals in air scatter almost isotropically at the surface and very
    // strongly forward through the volume, so both terms are needed.
    let albedo = vec3f(0.92, 0.94, 0.98);
    var color = albedo * INV_PI * radiance * wrapDiffuse(dot(normal, sun), 0.75) * shadow;

    // Forward scatter through the puff, at its peak looking straight into the sun.
    let forward = phaseMie(dot(-view, sun), 0.55) * 0.85;
    color += radiance * albedo * forward * mix(0.25, 1.0, shadow) * (1.0 - kind * 0.5);

    // Sky, which is what fills the shadowed side and keeps it blue.
    color += albedo * INV_PI * shIrradiance(normal, uniforms.harmonics) * uniforms.misc.y;

    // Spell light.
    if (uniforms.lights.count.x > 0.5) {
        color += spellLightingParticle(uniforms.lights, world, normal, albedo);
    }

    color = applyAerial(
        color, uniforms.camera.xyz, world, -view, sun,
        skyLUT, skySamp, radiance,
        uniforms.fog.x, uniforms.fog.y, uniforms.fog.z, uniforms.fog.w
    );

    return vec4f(color, alpha);
}
