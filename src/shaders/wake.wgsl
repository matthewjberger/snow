#import snow::snow_uniforms::SnowUniforms
#import snow::noise::{noise2, noised, ign}
#import snow::wake::{wakePoint, wakeScalars, wakeEroded}
#import snow::shading::{
    wrapDiffuse, snowSubsurface, snowGlints, shIrradiance,
    distributionGGX, visSmithGGXCorrelated, fresnelSchlick, fresnelSchlickRough
}
#import snow::atmosphere::{applyAerial, dirToLatLong}
#import snow::shadow_lookup::sunShadow
#import snow::spell_lights::spellLighting

// The snow-surf wake.

@group(0) @binding(0) var<uniform> uniforms: SnowUniforms;

@group(1) @binding(0) var wakeTex: texture_2d<f32>;
@group(1) @binding(1) var skyLUT: texture_2d<f32>;
@group(1) @binding(2) var skySamp: sampler;
@group(1) @binding(3) var cascade0: texture_2d<f32>;
@group(1) @binding(4) var cascade1: texture_2d<f32>;
@group(1) @binding(5) var cascade2: texture_2d<f32>;
@group(1) @binding(6) var cascadeSamp: sampler;

struct VertexInput {
    // (column, row, side)
    @location(0) lattice: vec3f,
}

struct Varyings {
    @builtin(position) clip: vec4f,
    @location(0) world: vec3f,
    @location(1) normal: vec3f,
    // Section parameter, from the base to the tip of the lip.
    @location(2) q: f32,
    // Distance behind the bow, in metres.
    @location(3) along: f32,
    @location(4) age: f32,
    @location(5) amplitude: f32,
    @location(6) curl: f32,
    @location(7) viewDist: f32,
}

@vertex
fn vertexMain(input: VertexInput) -> Varyings {
    let count = uniforms.wake.x;
    let time = uniforms.wake.w;
    let side = input.lattice.z;
    let u = input.lattice.x / max(uniforms.wake.y - 1.0, 1.0);
    let q = input.lattice.y / max(uniforms.wake.z - 1.0, 1.0);

    let point = wakePoint(wakeTex, count, u, q, side, time);

    // Nearly central differences.
    let du = 0.65 / max(uniforms.wake.y - 1.0, 1.0);
    let dq = 0.65 / max(uniforms.wake.z - 1.0, 1.0);
    let su = select(1.0, -1.0, u > 0.5);
    let sq = select(1.0, -1.0, q > 0.5);

    let alongTangent = (wakePoint(wakeTex, count, u + du * su, q, side, time) - point) * su;
    let acrossTangent = (wakePoint(wakeTex, count, u, q + dq * sq, side, time) - point) * sq;

    // The multiply by side is not cosmetic, and leaving it out is a bug that hides.
    var normal = cross(acrossTangent, alongTangent) * side;
    let length = length(normal);
    // Degenerate where the amplitude envelope has collapsed the strip onto its own
    // spine: the tail, and the frames just after the player lets go.
    normal = select(vec3f(0.0, 1.0, 0.0), normal / max(length, 1e-8), length > 1e-7);

    let scalars = wakeScalars(wakeTex, count, u, side);

    var out: Varyings;
    out.world = point;
    out.normal = normal;
    out.q = q;
    out.along = scalars.z;
    out.age = scalars.w;
    out.amplitude = scalars.x;
    out.curl = scalars.y;
    out.viewDist = distance(point, uniforms.camera.xyz);
    out.clip = uniforms.viewProjection * vec4f(point, 1.0);
    return out;
}

@fragment
fn fragmentMain(input: Varyings) -> @location(0) vec4f {
    let q = input.q;
    if (wakeEroded(input.along, q, input.age, uniforms.wake.w)) { discard; }

    let world = input.world;
    let view = normalize(uniforms.camera.xyz - world);
    let sun = uniforms.sunDir.xyz;

    // The wake is an open sheet with a curl in it, so both faces are visible and the
    // winding says nothing useful.
    let geometric = normalize(input.normal);
    let facing = select(-1.0, 1.0, dot(geometric, view) >= 0.0);
    var normal = geometric * facing;
    let shadowNormal = normal;

    // The swept normal points to the concave side, so this is true exactly when the eye
    // is inside the curl.
    let inside = facing > 0.0;

    // Broken snow grain.
    let footprint = max(
        length(vec2f(length(dpdx(world).xz), length(dpdy(world).xz))),
        1e-4
    );
    // Two oblique projections of the world position rather than the horizontal plane.
    let grainPoint = vec2f(
        dot(world, vec3f(0.91, 0.23, -0.35)),
        dot(world, vec3f(0.28, 0.84, 0.46))
    );
    let up = select(vec3f(0.0, 1.0, 0.0), vec3f(1.0, 0.0, 0.0), abs(normal.y) > 0.99);
    let tangent = normalize(cross(up, normal));
    let bitangent = cross(normal, tangent);

    // Two scales, each faded out by the pixel footprint, mirroring what the snow
    // material does over three.
    let fineFade = 1.0 - smoothstep(0.012, 0.09, footprint);
    if (fineFade > 0.002) {
        let g = noised(grainPoint * 26.0);
        normal = normalize(normal + (tangent * g.y + bitangent * g.z) * 0.15 * fineFade);
    }
    let coarseFade = 1.0 - smoothstep(0.09, 0.55, footprint);
    if (coarseFade > 0.002) {
        let g = noised(grainPoint * 5.5);
        normal = normalize(normal + (tangent * g.y + bitangent * g.z) * 0.10 * coarseFade);
    }

    // Freshly displaced snow: brighter and rougher than the pack it came out of.
    let albedo = vec3f(0.895, 0.920, 0.965);
    let roughness = 0.80;
    let f0 = vec3f(0.026);

    // Thin at the lip, deep at the base.
    let thickness = mix(0.92, 0.32, smoothstep(0.15, 0.95, q));

    let ndotl = dot(normal, sun);
    let ndotv = clamp(dot(normal, view), 1e-4, 1.0);
    let rotation = ign(input.clip.xy) * 6.28318530718;
    let shadow = sunShadow(
        cascade0, cascadeSamp, cascade1, cascadeSamp, cascade2, cascadeSamp,
        uniforms.shadow, world, shadowNormal, input.viewDist, rotation
    );

    let radiance = uniforms.sunRadiance.xyz;
    const INV_PI: f32 = 0.31830988618;

    // Occlusion, analytic, because the shadow map cannot supply it.
    let barrel = select(
        0.0,
        smoothstep(0.05, 0.75, q) * (0.45 + 0.55 * input.curl),
        inside
    );
    let occlusion = mix(1.0, 0.30, barrel);

    var color = albedo * INV_PI * radiance * wrapDiffuse(ndotl, 0.66) * shadow;

    // Transmission, coupled much harder to the shadow than the snow field's is.
    let transmission = snowSubsurface(
        normal, sun, view, radiance, thickness, uniforms.snow.z * 0.45, 1.5
    );
    color += transmission * albedo * mix(0.18, 1.0, shadow);

    if (ndotl > 0.0) {
        let half = normalize(view + sun);
        let distribution = distributionGGX(clamp(dot(normal, half), 0.0, 1.0), roughness);
        let visibility = visSmithGGXCorrelated(ndotv, ndotl, roughness);
        let fresnel = fresnelSchlick(clamp(dot(view, half), 0.0, 1.0), f0);
        color += radiance * distribution * visibility * fresnel * ndotl * shadow;
    }

    // Ambient, plus the bounce off the enormous white surface underneath it.
    var irradiance = shIrradiance(normal, uniforms.harmonics) * uniforms.misc.y;
    irradiance += shIrradiance(vec3f(0.0, 1.0, 0.0), uniforms.harmonics)
        * uniforms.misc.y * 0.30 * clamp(-normal.y * 0.5 + 0.5, 0.0, 1.0) * albedo;
    color += albedo * INV_PI * irradiance;

    let reflected = reflect(-view, normal);
    let skyReflection = textureSampleLevel(
        skyLUT, skySamp, dirToLatLong(reflected), sqrt(roughness) * 6.0
    ).rgb;
    color += skyReflection * fresnelSchlickRough(ndotv, f0, roughness) * uniforms.misc.y;

    // Spell light, above the occlusion so the barrel darkens it along with everything
    // else: a spell cast into the inside of a curl should light the cave, not shine
    // through the wall of it.
    if (uniforms.lights.count.x > 0.5) {
        color += spellLighting(
            uniforms.lights, world, normal, view, albedo,
            thickness, uniforms.snow.z * 0.45, 1.5
        );
    }

    // The occlusion is applied last and to everything, under two rules that are both
    // about hue rather than brightness.
    let caveTint = mix(vec3f(1.0), vec3f(0.55, 0.72, 1.0), (1.0 - occlusion) * 0.95);
    color *= occlusion * caveTint;

    if (uniforms.snow.x > 0.001) {
        let glints = snowGlints(
            world.xz, normal, view, sun, footprint, uniforms.snow.x, uniforms.snow.y
        );
        color += radiance * glints * shadow * 0.5;
    }

    color = applyAerial(
        color, uniforms.camera.xyz, world, -view, sun,
        skyLUT, skySamp, radiance,
        uniforms.fog.x, uniforms.fog.y, uniforms.fog.z, uniforms.fog.w
    );
    return vec4f(color, 1.0);
}
