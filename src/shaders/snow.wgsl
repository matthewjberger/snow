#import snow::snow_uniforms::SnowUniforms
#import snow::terrain_vertex::placeTerrainVertex
#import snow::noise::{noise2, ign}
#import snow::terrain::terrainFineFiltered
#import snow::deform::{deformUV, deformFalloff}
#import snow::shading::{
    normalFromGradient, blendNormalRNM, shIrradiance, distributionGGX,
    visSmithGGXCorrelated, fresnelSchlick, fresnelSchlickRough, wrapDiffuse,
    snowSubsurface, snowGlints
}
#import snow::shadow_lookup::sunShadow
#import snow::spell_lights::spellLighting
#import snow::atmosphere::{applyAerial, dirToLatLong}

// The snow material.

@group(0) @binding(0) var<uniform> uniforms: SnowUniforms;

@group(1) @binding(0) var heightTex: texture_2d<f32>;
@group(1) @binding(1) var heightSamp: sampler;
@group(1) @binding(2) var auxTex: texture_2d<f32>;
@group(1) @binding(3) var auxSamp: sampler;
@group(1) @binding(4) var detailTex: texture_2d<f32>;
@group(1) @binding(5) var detailSamp: sampler;
@group(1) @binding(6) var skyLUT: texture_2d<f32>;
@group(1) @binding(7) var skySamp: sampler;
@group(1) @binding(8) var deformTex: texture_2d<f32>;
@group(1) @binding(9) var deformSamp: sampler;
@group(1) @binding(10) var cascade0: texture_2d<f32>;
@group(1) @binding(11) var cascade1: texture_2d<f32>;
@group(1) @binding(12) var cascade2: texture_2d<f32>;
@group(1) @binding(13) var cascadeSamp: sampler;

struct Varyings {
    @builtin(position) clip: vec4f,
    @location(0) world: vec3f,
    @location(1) heightUV: vec2f,
    @location(2) viewDist: f32,
    @location(3) spacing: f32,
    /// Clipmap grid coordinates, interpolated, which is what the wireframe
    /// overlay measures its edge distances in.
    @location(4) grid: vec2f,
}

// The clipmap addressing is packed into the position attribute as / (gridI,
// ringLevel, gridJ).
@vertex
fn vertexMain(@location(0) packed: vec3f) -> Varyings {
    let placed = placeTerrainVertex(
        heightTex, heightSamp, auxTex, auxSamp, deformTex, deformSamp,
        vec2f(packed.x, packed.z), packed.y,
        uniforms.clipmap, uniforms.field, uniforms.surface,
        uniforms.deform, uniforms.misc.x
    );

    var out: Varyings;
    out.world = placed.world;
    out.heightUV = placed.heightUV;
    out.viewDist = distance(placed.world, uniforms.camera.xyz);
    out.spacing = placed.spacing;
    out.grid = vec2f(packed.x, packed.z);
    out.clip = uniforms.viewProjection * vec4f(placed.world, 1.0);
    return out;
}

// Unpacks a two-channel tangent-space normal.
fn unpackN(rg: vec2f) -> vec3f {
    let xy = rg * 2.0 - 1.0;
    return vec3f(xy, sqrt(max(0.0, 1.0 - dot(xy, xy))));
}

// Triplanar detail-normal fetch.
fn detailNormal(
    world: vec3f, N: vec3f, scale: f32, blendSteep: f32,
    ddxW: vec3f, ddyW: vec3f
) -> vec3f {
    var n = unpackN(textureSampleGrad(
        detailTex, detailSamp, world.xz * scale,
        ddxW.xz * scale, ddyW.xz * scale
    ).xy);

    if (blendSteep > 0.01) {
        let a = unpackN(textureSampleGrad(
            detailTex, detailSamp, world.xy * scale,
            ddxW.xy * scale, ddyW.xy * scale
        ).xy);
        let b = unpackN(textureSampleGrad(
            detailTex, detailSamp, world.zy * scale,
            ddxW.zy * scale, ddyW.zy * scale
        ).xy);
        let w = abs(N);
        let sum = w.x + w.y + w.z;
        n = normalize(mix(n, (a * w.z + b * w.x + n * w.y) / sum, blendSteep));
    }
    return n;
}

// Diagnostic: how far the depth map and the receiver disagree, in metres.
fn shadowMapDelta(world: vec3f, geoN: vec3f, viewDist: f32) -> f32 {
    let sp = uniforms.shadow.splits;
    var m = uniforms.shadow.matrices[2];
    var params = uniforms.shadow.cascade[2];
    var index = 2;
    if (viewDist < sp.x) {
        m = uniforms.shadow.matrices[0];
        params = uniforms.shadow.cascade[0];
        index = 0;
    } else if (viewDist < sp.y) {
        m = uniforms.shadow.matrices[1];
        params = uniforms.shadow.cascade[1];
        index = 1;
    }

    let lf = -uniforms.sunDir.xyz;
    let lr = normalize(cross(vec3f(0.0, 1.0, 0.0), lf));
    let nl = vec3f(dot(geoN, lr), dot(geoN, cross(lf, lr)), dot(geoN, lf));
    let sinL = sqrt(clamp(1.0 - nl.z * nl.z, 0.0, 1.0));
    let biased = world + geoN * (params.y * uniforms.shadow.filterParams.x * 1.5 * max(sinL, 0.2));

    let clip = m * vec4f(biased, 1.0);
    let ndc = clip.xyz / clip.w;
    // A large sentinel flags "this point is not inside the cascade at all".
    if (any(abs(ndc.xy) > vec2f(1.0)) || ndc.z < 0.0 || ndc.z > 1.0) { return 1e9; }

    let uv = vec2f(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
    var d = 0.0;
    if (index == 0) { d = textureSampleLevel(cascade0, cascadeSamp, uv, 0.0).r; }
    else if (index == 1) { d = textureSampleLevel(cascade1, cascadeSamp, uv, 0.0).r; }
    else { d = textureSampleLevel(cascade2, cascadeSamp, uv, 0.0).r; }

    return (d - ndc.z) * params.x;
}

@fragment
fn fragmentMain(input: Varyings) -> @location(0) vec4f {
    let world = input.world;
    let viewDist = input.viewDist;
    let V = normalize(uniforms.camera.xyz - world);
    let L = uniforms.sunDir.xyz;

    // World-space size of this pixel, which drives every filtering decision below.
    let ddxW = dpdx(world);
    let ddyW = dpdy(world);
    let footprint = max(length(vec2f(length(ddxW.xz), length(ddyW.xz))), 1e-4);

    // The narrow axis of that footprint, which is a very different number.
    let footprintMin = max(min(length(ddxW.xz), length(ddyW.xz)), 1e-4);

    // ---------------------------------------------------------------- slopes
    let aux = textureSampleLevel(auxTex, auxSamp, input.heightUV, 0.0);
    var grad = aux.xy;
    let rockMask = aux.z;
    let exposure = aux.w;

    let fine = terrainFineFiltered(
        world.xz, uniforms.surface.x, exposure, uniforms.surface.z, footprint
    );
    grad += fine.yz;

    // ------------------------------------------------------------ deformation
    // Depression, displaced berm mass and compression, written by feet, the surf wake
    // and every spell.
    var compression = 0.0;
    var iceAmount = 0.0;
    var deformDepth = 0.0;
    var deformBerm = 0.0;

    let dWeight = deformFalloff(world.xz, uniforms.deform.xy, uniforms.deform.z);
    if (dWeight > 0.001) {
        let dUV = deformUV(world.xz, uniforms.deform.z);
        let c = textureSampleLevel(deformTex, deformSamp, dUV, 0.0);

        // Gradient of berm minus depression, by central difference.
        let step = max(uniforms.deform.w * 2.0, footprintMin * 1.4);
        let eUV = step / uniforms.deform.z;

        let dxA = textureSampleLevel(deformTex, deformSamp, dUV + vec2f(eUV, 0.0), 0.0);
        let dxB = textureSampleLevel(deformTex, deformSamp, dUV - vec2f(eUV, 0.0), 0.0);
        let dzA = textureSampleLevel(deformTex, deformSamp, dUV + vec2f(0.0, eUV), 0.0);
        let dzB = textureSampleLevel(deformTex, deformSamp, dUV - vec2f(0.0, eUV), 0.0);
        let sx = (dxA.g - dxA.r) - (dxB.g - dxB.r);
        let sz = (dzA.g - dzA.r) - (dzB.g - dzB.r);

        // The four neighbours are already fetched, so blending them into the state
        // channels once the pixel is wider than a texel costs nothing and stops a
        // distant trail breaking into a dotted line.
        let wide = clamp(footprintMin / (uniforms.deform.w * 4.0), 0.0, 1.0) * 0.8;
        let df = mix(c, (c + dxA + dxB + dzA + dzB) * 0.2, wide);

        deformDepth = df.r * dWeight;
        deformBerm = df.g * dWeight;
        compression = clamp(df.b, 0.0, 1.0) * dWeight;
        iceAmount = clamp(df.a, 0.0, 1.0) * dWeight;

        grad += vec2f(sx, sz) / (2.0 * step) * uniforms.misc.x * dWeight;
    }

    var N = normalFromGradient(grad);

    // The surface the depth pass rendered: macro landform, the analytic fine layer and
    // carved snow, but nothing finer.
    let geoN = N;

    // ---------------------------------------------------------- detail normals Three
    // tiling scales, each faded by footprint so the finest only exists when it is
    // actually resolvable, and cross-faded so no scale ever pops in.
    let steep = smoothstep(0.55, 0.9, 1.0 - N.y);
    if (uniforms.surface.w > 0.001) {
        var acc = vec3f(0.0, 0.0, 1.0);

        let f0Fade = 1.0 - smoothstep(0.004, 0.02, footprint);
        if (f0Fade > 0.001) {
            let d = detailNormal(world, N, 7.5, steep, ddxW, ddyW);
            acc = blendNormalRNM(acc, mix(vec3f(0.0, 0.0, 1.0), d, f0Fade));
        }
        let f1Fade = 1.0 - smoothstep(0.02, 0.12, footprint);
        if (f1Fade > 0.001) {
            let d = detailNormal(world, N, 1.7, steep, ddxW, ddyW);
            acc = blendNormalRNM(acc, mix(vec3f(0.0, 0.0, 1.0), d, f1Fade * 0.85));
        }
        let f2Fade = 1.0 - smoothstep(0.1, 0.7, footprint);
        if (f2Fade > 0.001) {
            let d = detailNormal(world, N, 0.31, steep, ddxW, ddyW);
            acc = blendNormalRNM(acc, mix(vec3f(0.0, 0.0, 1.0), d, f2Fade * 0.6));
        }

        // Lift the tangent-space result onto the geometric normal.
        let up = select(vec3f(0.0, 1.0, 0.0), vec3f(1.0, 0.0, 0.0), abs(N.y) > 0.99);
        let T = normalize(cross(up, N));
        let B = cross(N, T);
        let s = uniforms.surface.w * mix(1.0, 0.45, compression);
        N = normalize(N + (T * acc.x + B * acc.y) * s);
    }

    let cavity = textureSampleGrad(
        detailTex, detailSamp, world.xz * 1.7,
        ddxW.xz * 1.7, ddyW.xz * 1.7
    ).z;

    // ------------------------------------------------------------- material Snow
    // albedo sits in a narrow, high, slightly blue band.
    var albedo = vec3f(0.855, 0.885, 0.945);
    var roughness = 0.62;
    var f0 = vec3f(0.028);
    var thickness = 1.0;

    // Compressed snow: denser, darker, tighter specular, scatters less.
    albedo = mix(albedo, vec3f(0.62, 0.665, 0.755), compression * 0.85);
    roughness = mix(roughness, 0.34, compression);
    thickness = mix(thickness, 0.35, compression);

    // Refrozen ice: smooth and genuinely reflective.
    albedo = mix(albedo, vec3f(0.42, 0.56, 0.70), iceAmount * 0.8);
    roughness = mix(roughness, 0.07, iceAmount);
    f0 = mix(f0, vec3f(0.045), iceAmount);
    thickness = mix(thickness, 0.15, iceAmount);

    // Exposed rock.
    let rockExposed = rockMask * smoothstep(0.32, 0.66, 1.0 - N.y);
    if (rockExposed > 0.001) {
        let rn = noise2(world.xz * 2.3) * 0.5 + 0.5;
        let rockCol = mix(vec3f(0.055, 0.058, 0.068), vec3f(0.115, 0.112, 0.118), rn);
        albedo = mix(albedo, rockCol, rockExposed);
        roughness = mix(roughness, 0.85, rockExposed);
        thickness = mix(thickness, 0.0, rockExposed);
    }

    // --- carved-snow surface state ----------------------------------------- Freshly
    // displaced mass is the opposite of trodden snow: it has just been broken up and
    // thrown, so it is loose, bright and rough.
    if (deformBerm > 0.002) {
        let loose = clamp(deformBerm * 5.0, 0.0, 1.0);
        albedo = mix(albedo, vec3f(0.895, 0.920, 0.965), loose * 0.55);
        roughness = mix(roughness, 0.78, loose * 0.7);
        thickness = mix(thickness, 1.0, loose * 0.6);
        // Broken snow has crystal faces pointing everywhere, which is where the chunky
        // granular read at a trail edge actually comes from.
        let chunk = noise2(world.xz * 34.0) * 0.5 + 0.5;
        albedo *= 1.0 - loose * 0.10 * chunk;
    }

    // Micro-occlusion in the grain crevices, stronger in carved edges.
    let ao = mix(1.0, cavity, 0.35 * (1.0 - smoothstep(0.02, 0.25, footprint)))
           * (1.0 - clamp(deformDepth * 1.9, 0.0, 1.0) * 0.38);

    // ------------------------------------------------------------- lighting
    let NdotL = dot(N, L);
    let NdotV = clamp(dot(N, V), 1e-4, 1.0);

    // Stable per-pixel rotation for the shadow filter.
    let noiseRot = ign(input.clip.xy) * 6.28318530718;

    var shadow = 1.0;
    if (NdotL > -0.35) {
        shadow = sunShadow(
            cascade0, cascadeSamp, cascade1, cascadeSamp, cascade2, cascadeSamp,
            uniforms.shadow, world, geoN, viewDist, noiseRot
        );
    }

    let sunRadiance = uniforms.sunRadiance.rgb;
    const INV_PI: f32 = 0.31830988618;

    // --- direct diffuse, wrapped ------------------------------------------- Snow's
    // mean free path is millimetres, so light wraps well past the geometric terminator.
    let wrapAmount = mix(0.62, 0.15, max(compression, rockExposed));
    let diff = wrapDiffuse(NdotL, wrapAmount);
    var direct = albedo * INV_PI * sunRadiance * diff * shadow;

    // --- subsurface --------------------------------------------------------
    let sss = snowSubsurface(
        N, L, V, sunRadiance, thickness,
        uniforms.snow.z * (1.0 - rockExposed), uniforms.snow.w
    );
    // Only partly shadowed: scattered light arrives through the snow, so a shadowed
    // drift lip still glows.
    direct += sss * albedo * mix(0.42, 1.0, shadow);

    // --- direct specular ---------------------------------------------------
    if (NdotL > 0.0) {
        let H = normalize(V + L);
        let NdotH = clamp(dot(N, H), 0.0, 1.0);
        let VdotH = clamp(dot(V, H), 0.0, 1.0);
        let D = distributionGGX(NdotH, roughness);
        let Vis = visSmithGGXCorrelated(NdotV, NdotL, roughness);
        let F = fresnelSchlick(VdotH, f0);
        direct += sunRadiance * D * Vis * F * NdotL * shadow;
    }

    // --- ambient ----------------------------------------------------------- Sky
    // irradiance from the harmonics.
    var irradiance = shIrradiance(N, uniforms.harmonics) * uniforms.misc.y;

    // Snow bounces onto itself: a huge, bright, near-white surround.
    let bounceUp = clamp(-N.y * 0.5 + 0.5, 0.0, 1.0);
    irradiance += shIrradiance(vec3f(0.0, 1.0, 0.0), uniforms.harmonics)
                * uniforms.misc.y * 0.28 * bounceUp * albedo;

    var ambient = albedo * INV_PI * irradiance;

    // Ambient specular from the sky, at a roughness-selected mip.
    let R = reflect(-V, N);
    let mip = sqrt(roughness) * 6.0;
    let skyRefl = textureSampleLevel(skyLUT, skySamp, dirToLatLong(R), mip).rgb;
    let Fr = fresnelSchlickRough(NdotV, f0, roughness);
    ambient += skyRefl * Fr * uniforms.misc.y * mix(1.0, 2.6, iceAmount);

    var color = direct + ambient;

    // --- spell light ------------------------------------------------------- Same
    // wrapped diffuse and the same transmission lobe the sun drives, so a ribbon of lit
    // water lying across a berm glows through the crest instead of merely putting a
    // bright patch on the near face.
    if (uniforms.lights.count.x > 0.5) {
        color += spellLighting(
            uniforms.lights, world, N, V, albedo, thickness,
            uniforms.snow.z * (1.0 - rockExposed), uniforms.snow.w
        );
    }

    // --- glints ------------------------------------------------------------ Last, and
    // added as radiance rather than modulated into the reflectance, because a glint is
    // a specular highlight from a crystal facet the shading normal does not represent.
    if (uniforms.snow.x > 0.001 && rockExposed < 0.5) {
        let g = snowGlints(
            world.xz, N, V, L, footprint, uniforms.snow.x, uniforms.snow.y
        );
        color += sunRadiance * g * shadow * (1.0 - iceAmount * 0.6) * 0.55;
    }

    // ---- occlusion, applied last and to everything -------------------------
    let caveTint = mix(vec3f(1.0), vec3f(0.55, 0.72, 1.0), (1.0 - ao) * 0.95);
    color *= ao * caveTint;

    // ------------------------------------------------------- aerial perspective
    color = applyAerial(
        color, uniforms.camera.xyz, world, -V, L,
        skyLUT, skySamp, sunRadiance,
        uniforms.fog.x, uniforms.fog.y, uniforms.fog.z, uniforms.fog.w
    );

    // ------------------------------------------------------------------ debug
    let debugMode = uniforms.misc.z;
    if (debugMode > 0.5) {
        if (debugMode < 1.5) {
            // Depression and berm are metres and berms are the shallower of the two, so
            // both are scaled to fill the range rather than shown raw.
            color = vec3f(deformDepth * 2.5, deformBerm * 5.0, compression * 0.6);
        } else if (debugMode < 2.5) {
            color = N * 0.5 + 0.5;
        } else if (debugMode < 3.5) {
            color = vec3f(viewDist / 400.0);
        } else if (debugMode > 4.5 && debugMode < 5.5) {
            // Pixel footprint, log scaled: green about a centimetre, yellow ten, red a
            // metre.
            let lf = log2(footprint);
            color = vec3f(
                clamp((lf + 3.3) / 3.3, 0.0, 1.0),
                clamp(1.0 - abs(lf + 4.6) / 2.0, 0.0, 1.0),
                clamp(-(lf + 5.0) / 2.0, 0.0, 1.0)
            );
        } else if (debugMode > 5.5 && debugMode < 6.5) {
            // Fine and detail normal only, with the macro landform removed, so the
            // high-frequency content can be judged on its own.
            color = normalFromGradient(fine.yz) * 0.5 + 0.5;
        } else if (debugMode > 6.5 && debugMode < 7.5) {
            // The sun visibility term on its own: cast shadow only, with no cosine, no
            // albedo, no ambient and no fog.
            color = select(vec3f(shadow), vec3f(0.35, 0.06, 0.06), NdotL <= 0.0);
        } else if (debugMode > 7.5 && debugMode < 8.5) {
            // The cosine term alone: the other half of why a pixel is dark.
            color = vec3f(max(NdotL, 0.0));
        } else if (debugMode > 9.5) {
            // Albedo alone, before a single lighting term touches it.
            color = albedo;
        } else if (debugMode > 8.5) {
            // Depth-map agreement, in metres.
            let dz = shadowMapDelta(world, geoN, viewDist);
            if (dz > 1e8) {
                color = vec3f(0.0, 0.15, 0.6);
            } else {
                let mag = clamp(abs(dz) / 12.0, 0.0, 1.0);
                let agree = 1.0 - smoothstep(0.0, 0.5, abs(dz));
                color = vec3f(agree * 0.45)
                      + select(vec3f(0.0, mag, 0.0), vec3f(mag, 0.0, 0.0), dz < 0.0);
            }
        } else {
            let c = vec3f(
                f32(viewDist < uniforms.shadow.splits.x),
                f32(viewDist < uniforms.shadow.splits.y),
                f32(viewDist < uniforms.shadow.splits.z)
            );
            color = color * 0.6 + c * 0.25;
        }
    }

    // -------------------------------------------------------------- wireframe
    // Drawn from the interpolated grid coordinates rather than by a line-mode
    // pipeline, because polygon-mode-line is a native-only device feature and
    // this has to run on the web too. It is the same set of edges: the two axis
    // families the clipmap is built on, and the diagonal the cell was split
    // along, whose direction flips with cell parity.
    if (uniforms.misc.w > 0.5) {
        let cell = floor(input.grid);
        let flipped = (i32(cell.x) + i32(cell.y)) % 2 != 0;
        let diagonal = select(input.grid.x + input.grid.y, input.grid.x - input.grid.y, flipped);
        let edges = vec3f(input.grid.x, input.grid.y, diagonal);
        let width = fwidth(edges);
        let distance = abs(fract(edges + 0.5) - 0.5) / max(width, vec3f(1e-6));
        let line = 1.0 - smoothstep(0.0, 1.0, min(min(distance.x, distance.y), distance.z));
        color = mix(color, vec3f(0.04, 0.05, 0.07), line * 0.85);
    }

    return vec4f(color, 1.0);
}
