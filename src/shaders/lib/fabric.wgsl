#define_import_path snow::fabric

#import snow::noise::{PI, noise2, ign}
#import snow::char_uniforms::CharUniforms
#import snow::shading::{
    wrapDiffuse, backScatter, visSmithGGXCorrelated, fresnelSchlick, shIrradiance
}
#import snow::shadow_lookup::sunShadow
#import snow::spell_lights::spellLightingSurface
#import snow::atmosphere::{applyAerial, dirToLatLong}

// The fabric material, shared by the skinned body and the simulated garments.

// Charlie sheen distribution.
fn dCharlie(NdotH: f32, roughness: f32) -> f32 {
    let inverse = 1.0 / max(0.05, roughness);
    let cos2h = NdotH * NdotH;
    let sin2h = max(1.0 - cos2h, 1e-4);
    return (2.0 + inverse) * pow(sin2h, inverse * 0.5) / (2.0 * PI);
}

// Ashikhmin's visibility term: cheap, and the only one that keeps sheen energy / sane
// at the grazing angles where the whole lobe lives.
fn vAshikhmin(NdotV: f32, NdotL: f32) -> f32 {
    return 1.0 / max(1e-4, 4.0 * (NdotL + NdotV - NdotL * NdotV));
}

fn dGGXAniso(TdotH: f32, BdotH: f32, NdotH: f32, ax: f32, ay: f32) -> f32 {
    let a2 = ax * ay;
    let d = vec3f(ay * TdotH, ax * BdotH, a2 * NdotH);
    let d2 = dot(d, d);
    if (d2 < 1e-9) { return 0.0; }
    let b2 = a2 / d2;
    return a2 * b2 * b2 / PI;
}

// Procedural plain weave: a tangent-space normal in xy and a cavity in z.
fn weave(uv: vec2f) -> vec3f {
    let p = uv * 6.28318530718;
    let warp = sin(p.x);
    let weft = sin(p.y);
    let over = smoothstep(-0.35, 0.35, warp * weft);
    let nx = cos(p.x) * mix(0.30, 1.0, over);
    let ny = cos(p.y) * mix(1.0, 0.30, over);
    // The cavity is deepest where neither thread is at its crown.
    let cavity = 0.55 + 0.45 * max(abs(warp), abs(weft));
    return vec3f(nx, ny, cavity);
}

// Karis' analytic split-sum environment reflectance.
fn envBRDFApprox(f0: vec3f, roughness: f32, NdotV: f32) -> vec3f {
    let c0 = vec4f(-1.0, -0.0275, -0.572, 0.022);
    let c1 = vec4f(1.0, 0.0425, 1.04, -0.04);
    let r = vec4f(roughness) * c0 + c1;
    let a004 = min(r.x * r.x, exp2(-9.28 * NdotV)) * r.x + r.y;
    return f0 * (-1.04 * a004 + r.z) + (1.04 * a004 + r.w);
}

// Screen-space cotangent frame.
fn cotangentFrame(N: vec3f, dp1: vec3f, dp2: vec3f, duv1: vec2f, duv2: vec2f) -> mat3x3f {
    let dp2perp = cross(dp2, N);
    let dp1perp = cross(N, dp1);
    let T = dp2perp * duv1.x + dp1perp * duv2.x;
    let B = dp2perp * duv1.y + dp1perp * duv2.y;
    let scale = inverseSqrt(max(max(dot(T, T), dot(B, B)), 1e-12));
    return mat3x3f(T * scale, B * scale, N);
}

// The per-fragment inputs the fabric shading takes, alongside its bindings.
struct FabricInput {
    world: vec3f,
    normal: vec3f,
    uv: vec2f,
    // (material slot, baked occlusion)
    aux: vec2f,
    viewDist: f32,
    pixel: vec2f,
}

fn shadeFabric(
    uniforms: CharUniforms,
    skyLUT: texture_2d<f32>,
    skySamp: sampler,
    cascade0: texture_2d<f32>,
    cascade1: texture_2d<f32>,
    cascade2: texture_2d<f32>,
    cascadeSamp: sampler,
    input: FabricInput
) -> vec3f {
    let world = input.world;
    let V = normalize(uniforms.camera.xyz - world);
    let L = uniforms.sunDir.xyz;

    // Garments are open sheets and the hood is a shell, so the camera sees both sides
    // of nearly everything.
    var N = normalize(input.normal);
    if (dot(N, V) < 0.0) { N = -N; }
    let geoN = N;

    let slot = clamp(i32(input.aux.x + 0.5), 0, 7);
    let albedoRoughness = uniforms.matAlbedo[slot];
    let params = uniforms.matParams[slot];
    var albedo = albedoRoughness.rgb;
    var roughness = albedoRoughness.a;
    let sheenAmount = params.x;
    let anisotropy = params.y;
    let transmit = params.z;
    let weaveDepth = params.w;

    // ------------------------------------------------------------ weave detail
    let weaveUv = input.uv * uniforms.misc.z;
    let dp1 = dpdx(world);
    let dp2 = dpdy(world);
    let duv1 = dpdx(weaveUv);
    let duv2 = dpdy(weaveUv);
    let tbn = cotangentFrame(N, dp1, dp2, duv1, duv2);

    // Fade the weave out once a thread is under a pixel, or it aliases into a crawling
    // moire, which is the same footprint logic the snow's detail layers use.
    let footprint = max(length(duv1), length(duv2));
    let weaveFade = 1.0 - smoothstep(0.10, 0.45, footprint);
    var cavity = 1.0;
    if (weaveDepth > 0.001 && weaveFade > 0.001) {
        let w = weave(weaveUv);
        N = normalize(N + (tbn[0] * w.x + tbn[1] * w.y) * weaveDepth * weaveFade * 0.5);
        cavity = mix(1.0, w.z, weaveFade * 0.8);
    }

    // Slub: real yarn varies along its length, and a little variation in the base tone does more
    // for "this is a woven thing" than another specular term.
    let slub = noise2(input.uv * vec2f(9.0, 26.0)) * 0.5 + 0.5;
    albedo *= 0.90 + 0.20 * slub;
    roughness = clamp(roughness * (0.94 + 0.12 * slub), 0.05, 1.0);

    // Baked at the vertex, times the weave cavity.
    let ao = input.aux.y * cavity;

    let NdotL = dot(N, L);
    let NdotV = clamp(dot(N, V), 1e-4, 1.0);
    let noiseRot = ign(input.pixel) * 6.28318530718;

    var shadow = 1.0;
    if (NdotL > -0.4) {
        shadow = sunShadow(
            cascade0, cascadeSamp, cascade1, cascadeSamp, cascade2, cascadeSamp,
            uniforms.shadow, world, geoN, input.viewDist, noiseRot
        );
    }

    let sun = uniforms.sunRadiance.rgb;
    const INV_PI: f32 = 0.31830988618;

    // Wrapped a little: fabric passes some light at fibre scale, and the terminator on a
    // sleeve is genuinely soft.
    let diffuse = wrapDiffuse(NdotL, 0.18);
    var color = albedo * INV_PI * sun * diffuse * shadow;

    if (transmit > 0.001) {
        let back = backScatter(N, L, V, 0.4, 4.0, 1.0);
        color += sun * albedo * back * transmit * uniforms.misc.y * mix(0.35, 1.0, shadow);
    }

    if (NdotL > 0.0) {
        let H = normalize(V + L);
        let NdotH = clamp(dot(N, H), 0.0, 1.0);
        let VdotH = clamp(dot(V, H), 0.0, 1.0);

        let base = max(0.04, roughness * roughness);
        let ax = base * (1.0 + anisotropy);
        let ay = base / (1.0 + anisotropy);
        let d = dGGXAniso(dot(tbn[0], H), dot(tbn[1], H), NdotH, ax, ay);
        let vis = visSmithGGXCorrelated(NdotV, max(NdotL, 1e-4), roughness);
        let fresnel = fresnelSchlick(VdotH, vec3f(0.035));
        color += sun * d * vis * fresnel * NdotL * shadow;

        // Sheen, tinted toward the albedo but desaturated: fibre scatter is closer to
        // white than the bulk colour, which is why a navy robe rims pale blue.
        let sheenTint = mix(vec3f(1.0), normalize(albedo + 1e-4), 0.35);
        let charlie = dCharlie(NdotH, 0.42);
        let graze = 0.16 + 0.84 * pow(1.0 - NdotV, 2.0);
        let lobe = min(charlie * vAshikhmin(NdotV, max(NdotL, 1e-4)) * NdotL, 0.25);
        color += sun * sheenTint * lobe * graze * sheenAmount * shadow;
    }

    var irradiance = shIrradiance(N, uniforms.harmonics) * uniforms.misc.x;
    // Bounce off the snow.
    let up = clamp(-N.y * 0.5 + 0.5, 0.0, 1.0);
    irradiance +=
        shIrradiance(vec3f(0.0, 1.0, 0.0), uniforms.harmonics) * uniforms.misc.x * 0.40 * up;

    color += albedo * INV_PI * irradiance * ao;

    // Ambient sheen: the sky wrapping around a fuzzy silhouette.
    let rim = pow(1.0 - NdotV, 4.0);
    let skyAmbient = shIrradiance(N, uniforms.harmonics) * uniforms.misc.x * INV_PI;
    color += skyAmbient * rim * sheenAmount * 0.55 * ao;

    let reflection = reflect(-V, N);
    let mip = sqrt(roughness) * 6.0;
    let skyReflection =
        textureSampleLevel(skyLUT, skySamp, dirToLatLong(reflection), mip).rgb;
    color += skyReflection * envBRDFApprox(vec3f(0.035), roughness, NdotV) * uniforms.misc.x * ao;

    // The caster is standing inside the thing they are casting, so this is the one
    // material where the spell lights are almost always the dominant source: a low sun
    // is behind the figure for most of the framing this demo uses, and a robe lit only
    // by sky ambient is a silhouette.
    if (uniforms.lights.count.x > 0.5) {
        color += spellLightingSurface(
            uniforms.lights, world, N, V, albedo, vec3f(0.035), roughness, 0.35
        ) * ao;
    }

    return applyAerial(
        color, uniforms.camera.xyz, world, -V, L,
        skyLUT, skySamp, sun,
        uniforms.fog.x, uniforms.fog.y, uniforms.fog.z, uniforms.fog.w
    );
}
