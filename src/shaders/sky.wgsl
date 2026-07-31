#import snow::noise::rot2
#import snow::noise::fbmd
#import snow::atmosphere::{dirToLatLong, aerialTransmittance, aerialInscatterSky}
#import snow::shading::{wrapDiffuse, snowSubsurface, shIrradiance}
#import snow::ridge::{RidgeHit, ridgeMarch, ridgeShadow}

// The skybox, drawn as a cube pinned to the camera and pushed to the far plane so it
// fills exactly whatever the terrain does not.

struct SkyUniforms {
    viewProjection: mat4x4f,
    // (camera position, half the far plane, which is how large the cube is)
    camera: vec4f,
    // (direction toward the sun, the shared radiometric scale)
    sun: vec4f,
    // (normalised sun hue, ambient intensity)
    sunColor: vec4f,
    // (direct solar irradiance at the ground, peak height of the far range)
    sunRadiance: vec4f,
    // (seconds, cloud amount, wind direction xz)
    weather: vec4f,
    // (fog density, height falloff, fog start, aerial strength)
    fog: vec4f,
    harmonics: array<vec4f, 9>,
}

@group(0) @binding(0) var<uniform> uniforms: SkyUniforms;
@group(1) @binding(0) var skyLUT: texture_2d<f32>;
@group(1) @binding(1) var skySamp: sampler;

struct Varyings {
    @builtin(position) clip: vec4f,
    @location(0) direction: vec3f,
}

@vertex
fn vertexMain(@location(0) position: vec3f) -> Varyings {
    let world = position * uniforms.camera.w + uniforms.camera.xyz;

    var out: Varyings;
    out.direction = position;
    var clip = uniforms.viewProjection * vec4f(world, 1.0);
    // Force to the far plane so nothing the terrain draws can lose to it.
    clip.z = clip.w * 0.999999;
    out.clip = clip;
    return out;
}

// Shade a point on the far range.
fn shadeRidge(hit: RidgeHit, dir: vec3f) -> vec3f {
    let N = hit.normal;
    let L = uniforms.sun.xyz;
    let ridgeAmp = uniforms.sunRadiance.w;
    let sunRadiance = uniforms.sunRadiance.rgb;
    let ambient = uniforms.sunColor.w;

    // Snow almost everywhere, rock only on the faces too steep to hold it.
    let steep = 1.0 - N.y;
    let snowMask = clamp(1.0 - smoothstep(0.46, 0.80, steep), 0.0, 1.0);

    let rock = vec3f(0.052, 0.055, 0.066);
    let snow = vec3f(0.855, 0.885, 0.945);
    let albedo = mix(rock, snow, snowMask);

    let shadow = ridgeShadow(hit.pos, hit.height, L, ridgeAmp);

    const INV_PI: f32 = 0.31830988618;
    let diff = wrapDiffuse(dot(N, L), mix(0.15, 0.62, snowMask));
    var col = albedo * INV_PI * sunRadiance * diff * shadow;

    // --- subsurface --------------------------------------------------------- Snow is
    // translucent, and a mountain of snow with the sun behind it glows rather than
    // going to a dark silhouette.
    let V = -dir;
    col += snowSubsurface(N, L, V, sunRadiance, 0.45, snowMask, 1.0)
         * albedo * mix(0.5, 1.0, shadow);

    // Sky fill.
    col += albedo * INV_PI * shIrradiance(N, uniforms.harmonics) * ambient;

    // Bounce off the range's own snow, exactly as the field does off itself.
    col += albedo * INV_PI * shIrradiance(vec3f(0.0, 1.0, 0.0), uniforms.harmonics)
         * ambient * 0.30 * clamp(-N.y * 0.5 + 0.5, 0.0, 1.0) * snowMask;

    // ---- aerial perspective ------------------------------------------------
    let hitPos = vec3f(hit.pos.x, hit.height, hit.pos.y);
    let t = aerialTransmittance(
        uniforms.camera.xyz, hitPos, uniforms.fog.x, uniforms.fog.y, uniforms.fog.z
    );
    let ext = clamp(1.0 - pow(t, uniforms.fog.w), 0.0, 1.0);

    // The identical inscatter the ground converges to.
    let inscatter = aerialInscatterSky(skyLUT, skySamp, dir, L, sunRadiance, ext);

    return mix(col, inscatter, ext);
}

@fragment
fn fragmentMain(input: Varyings) -> @location(0) vec4f {
    let dir = normalize(input.direction);
    let sunDir = uniforms.sun.xyz;
    let sunIntensity = uniforms.sun.w;
    let sunColor = uniforms.sunColor.rgb;
    let ridgeAmp = uniforms.sunRadiance.w;

    var col = textureSampleLevel(skyLUT, skySamp, dirToLatLong(dir), 0.0).rgb;

    // ------------------------------------------------------- far-field range Above the
    // band the march's ceiling test rejects immediately, so the upper bound is only
    // there to skip the call.
    if (ridgeAmp > 1.0 && dir.y < 0.230 && dir.y > -0.050) {
        let hit = ridgeMarch(uniforms.camera.xyz, dir, ridgeAmp);
        if (hit.hit) {
            col = shadeRidge(hit, dir);
        }
    }

    // ---------------------------------------------------------- solar disc About half
    // a degree across, with limb darkening.
    let mu = dot(dir, sunDir);
    let discCos = cos(0.0046);
    if (mu > discCos) {
        let r = sqrt(max(0.0, 1.0 - mu * mu)) / 0.0046;
        let limb = pow(max(0.0, 1.0 - r * r * 0.72), 0.42);
        col += sunColor * sunIntensity * 42.0 * limb;
    }
    let aureole = pow(max(0.0, mu), 1400.0) * 5.5 + pow(max(0.0, mu), 64.0) * 0.28;
    col += sunColor * sunIntensity * aureole * 0.5;

    // ------------------------------------------------------------- cirrus Thin, high
    // and wind-aligned.
    let cloudAmount = uniforms.weather.y;
    if (cloudAmount > 0.001 && dir.y > 0.0) {
        let windDir = uniforms.weather.zw;
        // Project onto a high plane so bands converge at the horizon.
        let planeY = 1.0 / max(0.06, dir.y);
        var cp = dir.xz * planeY * 0.5 + windDir * uniforms.weather.x * 0.004;

        // Stretch across the wind so the streaks run with it.
        let a = atan2(windDir.x, windDir.y);
        cp = rot2(a) * cp;
        cp.x *= 0.28;

        let n = fbmd(cp, 4, 2.13, 0.52).x;
        var cloud = smoothstep(0.06, 0.34, n);
        // Fade out at the horizon and at the zenith.
        cloud *= smoothstep(0.0, 0.22, dir.y) * (1.0 - smoothstep(0.55, 1.0, dir.y) * 0.45);
        cloud *= cloudAmount;

        // Lit from below by a low sun, so the underside catches the warmth.
        let sunLit = pow(max(0.0, mu * 0.5 + 0.5), 3.0);
        let cloudCol = mix(vec3f(0.52, 0.60, 0.74), sunColor * 1.35, sunLit * 0.75);
        col = mix(col, cloudCol * (0.55 + sunIntensity * 0.06), cloud * 0.62);
    }

    return vec4f(col, 1.0);
}
