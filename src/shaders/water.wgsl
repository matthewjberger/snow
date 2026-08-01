#import snow::snow_uniforms::SnowUniforms
#import snow::noise::{noise2, noised, ign}
#import snow::water::{waterPoint, waterRow, waterSpine}
#import snow::shading::{
    wrapDiffuse, snowSubsurface, snowGlints, shIrradiance, backScatter,
    distributionGGX, visSmithGGXCorrelated, fresnelSchlick
}
#import snow::atmosphere::{applyAerial, dirToLatLong}
#import snow::shadow_lookup::sunShadow
#import snow::spell_lights::spellLightingSurface

// Spell water: one mesh, one material, one draw, eight strands.

@group(0) @binding(0) var<uniform> uniforms: SnowUniforms;

@group(1) @binding(0) var waterTex: texture_2d<f32>;
@group(1) @binding(1) var skyLUT: texture_2d<f32>;
@group(1) @binding(2) var skySamp: sampler;
@group(1) @binding(3) var cascade0: texture_2d<f32>;
@group(1) @binding(4) var cascade1: texture_2d<f32>;
@group(1) @binding(5) var cascade2: texture_2d<f32>;
@group(1) @binding(6) var cascadeSamp: sampler;

struct VertexInput {
    // (column, ring, strand)
    @location(0) lattice: vec3f,
}

struct Varyings {
    @builtin(position) clip: vec4f,
    @location(0) world: vec3f,
    @location(1) normal: vec3f,
    @location(2) radius: f32,
    @location(3) foam: f32,
    @location(4) milk: f32,
    @location(5) alpha: f32,
    @location(6) viewDist: f32,
}

/// Absorption per metre, exaggerated well past real water.
///
/// Clear water tints by a few percent over the decimetre a spell body is thick,
/// which is invisible. This is glacial melt full of entrained snow, and it has to
/// be strongly coloured at arm's length.
const WATER_ABSORB: vec3f = vec3f(3.40, 0.72, 0.34);

@vertex
fn vertexMain(input: VertexInput) -> Varyings {
    let strand = i32(input.lattice.z);
    let params = uniforms.strands[strand];
    let count = max(params.w, 2.0);
    let base = strand * 3;
    let time = uniforms.water.z;

    let u = input.lattice.x / max(uniforms.water.x - 1.0, 1.0);
    let q = input.lattice.y / max(uniforms.water.y - 1.0, 1.0);

    // A dead strand does none of the work below. Every vertex of the lattice runs
    // this shader whether or not its strand is in use, and the eight together are
    // tens of thousands of vertices each evaluating a swept surface four times.
    // The branch is perfectly wave-coherent, since a strand is thousands of
    // contiguous vertices.
    let alive = params.z > 0.001 && params.w >= 2.0;

    var point = vec3f(0.0);
    var normal = vec3f(0.0, 1.0, 0.0);
    var radius = 0.0;
    var foam = 0.0;

    if (alive) {
        point = waterPoint(waterTex, base, count, params.x, u, q, time);

        let du = 0.65 / max(uniforms.water.x - 1.0, 1.0);
        let dq = 0.65 / max(uniforms.water.y - 1.0, 1.0);
        let su = select(1.0, -1.0, u > 0.5);
        let sq = select(1.0, -1.0, q > 0.5);

        let alongTangent =
            (waterPoint(waterTex, base, count, params.x, u + du * su, q, time) - point) * su;
        let acrossTangent =
            (waterPoint(waterTex, base, count, params.x, u, q + dq * sq, time) - point) * sq;

        var swept = cross(acrossTangent, alongTangent);
        let length = length(swept);
        swept = select(vec3f(0.0, 1.0, 0.0), swept / max(length, 1e-8), length > 1e-7);

        // A tube is closed, and the sign of a differenced cross product on it
        // depends on how the transported frame happens to wind, so it is resolved
        // against the one thing that cannot be ambiguous: the vector from the
        // spine out to the surface. The same test on a sheet is meaningless, and
        // the fragment stage turns that normal toward the eye instead.
        let axis = point - waterSpine(waterTex, base, count, u);
        let outward = select(swept, -swept, dot(swept, axis) < 0.0);
        normal = select(swept, outward, params.x < 0.5);

        radius = waterRow(waterTex, base, count, u).w;
        foam = waterRow(waterTex, base + 2, count, u).z;
    }

    var out: Varyings;
    out.world = point;
    out.normal = normal;
    out.radius = radius;
    out.foam = foam;
    out.milk = params.y;
    out.alpha = select(0.0, params.z, alive);
    out.viewDist = distance(point, uniforms.camera.xyz);
    out.clip = uniforms.viewProjection * vec4f(point, 1.0);
    return out;
}

@fragment
fn fragmentMain(input: Varyings) -> @location(0) vec4f {
    if (input.alpha <= 0.003 || input.radius <= 0.0005) { discard; }

    let world = input.world;
    let view = normalize(uniforms.camera.xyz - world);
    let sun = uniforms.sunDir.xyz;

    // Both faces are visible, since the body is transparent and the sheet profile
    // is genuinely open, so the winding says nothing.
    let geometric = normalize(input.normal);
    var normal = select(-geometric, geometric, dot(geometric, view) >= 0.0);
    let shadowNormal = normal;

    let footprint = max(
        length(vec2f(length(dpdx(world).xz), length(dpdy(world).xz))),
        1e-4
    );
    // Two oblique slices rather than the horizontal plane: the body is as often
    // vertical as horizontal, and a planar lookup bands the vertical parts.
    let flowPoint = vec2f(
        dot(world, vec3f(0.88, 0.31, -0.36)),
        dot(world, vec3f(0.24, 0.79, 0.56))
    );
    let up = select(vec3f(0.0, 1.0, 0.0), vec3f(1.0, 0.0, 0.0), abs(normal.y) > 0.99);
    let tangent = normalize(cross(up, normal));
    let bitangent = cross(normal, tangent);

    // All of the fine detail lives here, where the sampling rate is the pixel
    // rather than the vertex, with each octave faded out by footprint before it
    // can shimmer.
    let time = uniforms.water.z;
    let rippleFade = 1.0 - smoothstep(0.03, 0.22, footprint);
    if (rippleFade > 0.002) {
        let coarse = noised(flowPoint * 8.5 + vec2f(time * 0.7, -time * 0.5));
        let mid = noised(flowPoint * 21.0 + vec2f(-time * 1.6, time * 1.1));
        normal = normalize(
            normal
                + (tangent * (coarse.y * 0.085 + mid.y * 0.055)
                    + bitangent * (coarse.z * 0.085 + mid.z * 0.055))
                    * rippleFade
        );
    }
    let fineFade = 1.0 - smoothstep(0.006, 0.045, footprint);
    if (fineFade > 0.002) {
        let fine = noised(flowPoint * 62.0 + vec2f(time * 3.1, time * 2.2));
        normal = normalize(normal + (tangent * fine.y + bitangent * fine.z) * 0.030 * fineFade);
    }

    let ndotv = clamp(dot(normal, view), 1e-4, 1.0);
    let ndotl = dot(normal, sun);
    let rotation = ign(input.clip.xy) * 6.28318530718;
    let shadow = sunShadow(
        cascade0, cascadeSamp, cascade1, cascadeSamp, cascade2, cascadeSamp,
        uniforms.shadow, world, shadowNormal, input.viewDist, rotation
    );

    let radiance = uniforms.sunRadiance.xyz;
    const INV_PI: f32 = 0.31830988618;

    // How far the light travelled through the body. Grazing views cut a long
    // chord and head-on views a short one, which is most of what makes a tube
    // read as a volume. The constant term matters as much: keying the path purely
    // off the view angle puts all the colour at the silhouette, which is exactly
    // where the reflection is strongest, and the body comes out white.
    let path = clamp(
        input.radius * (1.25 + 1.9 * (1.0 - ndotv)) * uniforms.water.w,
        0.01,
        3.0
    );
    let transmit = exp(-WATER_ABSORB * path);

    // Refraction with dispersion. The spread is small, but on a surface this
    // curved the ray fans far enough to put a fringe on the rim, which is exactly
    // where the eye looks for it. Total internal reflection returns zero, so the
    // mirror direction stands in, which is what actually happens.
    let mirror = reflect(-view, normal);
    let bentRed = refract(-view, normal, 1.0 / 1.3300);
    let bentGreen = refract(-view, normal, 1.0 / 1.3330);
    let bentBlue = refract(-view, normal, 1.0 / 1.3400);
    let behind = vec3f(
        textureSampleLevel(
            skyLUT, skySamp,
            dirToLatLong(select(mirror, bentRed, dot(bentRed, bentRed) > 0.5)), 1.6
        ).r,
        textureSampleLevel(
            skyLUT, skySamp,
            dirToLatLong(select(mirror, bentGreen, dot(bentGreen, bentGreen) > 0.5)), 1.6
        ).g,
        textureSampleLevel(
            skyLUT, skySamp,
            dirToLatLong(select(mirror, bentBlue, dot(bentBlue, bentBlue) > 0.5)), 1.6
        ).b
    );
    var color = behind * transmit;

    // Light that entered the body, bounced off entrained air and snow, and came
    // back out toward the eye, tinted by what the water did not absorb on the way.
    // The reciprocal pi is load bearing: a scattering lobe is a distribution,
    // and dropping it overstates the peak threefold.
    let inScatter = backScatter(normal, sun, view, 0.55, 2.6, 1.0);
    let scatterTint =
        mix(vec3f(0.40, 0.80, 1.0), vec3f(0.72, 0.94, 1.0), exp(-path * 1.6));
    color += radiance * INV_PI * scatterTint * inScatter * (0.55 + 1.3 * input.milk)
        * uniforms.snow.z * mix(0.30, 1.0, shadow);

    // Sky filling the body from above, without which the shadowed side of an arc
    // is left with the refraction alone and goes dead.
    color += shIrradiance(normal, uniforms.harmonics) * uniforms.misc.y * INV_PI
        * scatterTint * (0.35 + 0.5 * input.milk);

    // Slush is an opaque diffuse population inside the body,
    // so it fills in behind the transparency rather than tinting it.
    if (input.milk > 0.002) {
        let slushAlbedo = vec3f(0.86, 0.90, 0.96);
        var slush = slushAlbedo * INV_PI * radiance * wrapDiffuse(ndotl, 0.62) * shadow;
        slush += slushAlbedo * INV_PI * shIrradiance(normal, uniforms.harmonics)
            * uniforms.misc.y;
        slush += snowSubsurface(normal, sun, view, radiance, 0.45, uniforms.snow.z * 0.8, 1.2)
            * slushAlbedo * mix(0.35, 1.0, shadow);
        color = mix(color, slush, input.milk * 0.85);
    }

    // The leading edge, where the body is tearing itself apart against the air and
    // the snow, broken up by a drifting noise so it is froth rather than a band.
    var foam = input.foam;
    if (foam > 0.002) {
        let coarse = noise2(flowPoint * 22.0 + vec2f(time * 1.7, -time * 1.1)) * 0.5 + 0.5;
        let fine = noise2(flowPoint * 61.0 - vec2f(time * 3.3, time * 2.1)) * 0.5 + 0.5;
        foam = clamp(foam * (0.35 + 1.5 * coarse * (0.5 + 0.7 * fine)), 0.0, 1.0);
        let foamAlbedo = vec3f(0.93, 0.955, 0.99);
        var froth = foamAlbedo * INV_PI * radiance * wrapDiffuse(ndotl, 0.72) * shadow;
        froth += foamAlbedo * INV_PI * shIrradiance(normal, uniforms.harmonics)
            * uniforms.misc.y;
        froth += snowSubsurface(normal, sun, view, radiance, 0.25, uniforms.snow.z, 1.4)
            * foamAlbedo * mix(0.4, 1.0, shadow);
        color = mix(color, froth, foam);
    }

    // Fresnel is the whole reason water looks wet, applied after the body terms
    // because it sits on the surface and what it returns never went through the
    // water. Capped short of a perfect mirror: that limit assumes a surface you
    // cannot see the far side of, and letting it reach unity deletes the volume
    // exactly at the silhouette. Milkiness takes the surface out as well as
    // filling the body in, because a mass of ice crystals in air has no specular
    // surface at all.
    let fresnel = min(fresnelSchlick(ndotv, vec3f(0.02)), vec3f(0.72));
    let skyReflection = textureSampleLevel(skyLUT, skySamp, dirToLatLong(mirror), 0.7).rgb;
    color = mix(
        color,
        skyReflection,
        fresnel * (1.0 - foam * 0.7) * (1.0 - input.milk * 0.88)
    );

    if (ndotl > 0.0) {
        let half = normalize(view + sun);
        let roughness = mix(0.055, 0.68, max(foam * 0.55, input.milk));
        let distribution = distributionGGX(clamp(dot(normal, half), 0.0, 1.0), roughness);
        let visibility = visSmithGGXCorrelated(ndotv, ndotl, roughness);
        let surface = fresnelSchlick(clamp(dot(view, half), 0.0, 1.0), vec3f(0.02));
        color += radiance * distribution * visibility * surface * ndotl * shadow;
    }

    // Shed droplets on the outer skin, from the snow's own glint field, so the
    // sparkle on the water and the sparkle on the field are the same effect.
    if (uniforms.snow.x > 0.001) {
        let glints = snowGlints(
            flowPoint, normal, view, sun, footprint,
            uniforms.snow.x * (0.6 + 0.8 * max(foam, input.milk)),
            uniforms.snow.y
        );
        color += radiance * glints * shadow * 0.7;
    }

    if (uniforms.lights.count.x > 0.5) {
        color += spellLightingSurface(
            uniforms.lights, world, normal, view,
            mix(vec3f(0.35, 0.62, 0.78), vec3f(0.9), input.milk),
            vec3f(0.02), 0.12, 0.55
        );
    }

    // Nearly opaque, which is the opposite of the obvious answer. Running the
    // alpha off Fresnel counts the background twice, once through the refracted
    // lookup and again through the blend, and over a snow field the undistorted
    // one is white and wins. A high alpha leaves the refraction as the only path
    // the background takes. What is left for the alpha to do is close the ends,
    // where the radius tapers to nothing.
    let taper = clamp(input.radius / 0.055, 0.0, 1.0);
    let clear = taper * mix(0.74, 0.97, 1.0 - ndotv);
    let alpha = mix(clear, taper, max(foam, input.milk * 0.9)) * input.alpha;
    if (alpha < 0.004) { discard; }

    color = applyAerial(
        color, uniforms.camera.xyz, world, -view, sun,
        skyLUT, skySamp, radiance,
        uniforms.fog.x, uniforms.fog.y, uniforms.fog.z, uniforms.fog.w
    );
    return vec4f(color, alpha);
}
