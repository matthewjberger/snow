#define_import_path snow::atmosphere

#import snow::noise::PI

// Sky model and aerial perspective.

const EARTH_R: f32 = 6360000.0;
const ATMOS_R: f32 = 6420000.0;
const H_RAYLEIGH: f32 = 8000.0;
const H_MIE: f32 = 1200.0;

// Sea-level scattering coefficients, per metre.
const BETA_R: vec3f = vec3f(5.8e-6, 13.5e-6, 33.1e-6);
const BETA_M: vec3f = vec3f(21e-6, 21e-6, 21e-6);
const MIE_G: f32 = 0.76;

// Strength of the isotropic multiple-scattering approximation, relative to / single-
// scattered Rayleigh.
const MS_BOOST: f32 = 1.5;

// Distance to the far intersection of a ray with a sphere centred on the / origin.
fn raySphereFar(origin: vec3f, dir: vec3f, radius: f32) -> f32 {
    let b = dot(origin, dir);
    let c = dot(origin, origin) - radius * radius;
    let d = b * b - c;
    if (d < 0.0) { return -1.0; }
    return -b + sqrt(d);
}

fn phaseRayleigh(mu: f32) -> f32 {
    return (3.0 / (16.0 * PI)) * (1.0 + mu * mu);
}

fn phaseMie(mu: f32, g: f32) -> f32 {
    let g2 = g * g;
    let n = (1.0 - g2) * (1.0 + mu * mu);
    let d = (2.0 + g2) * pow(1.0 + g2 - 2.0 * g * mu, 1.5);
    return (3.0 / (8.0 * PI)) * n / d;
}

// Full single-scattering sky radiance for a view direction.
fn nishitaSky(rayDir: vec3f, sunDir: vec3f, sunIntensity: f32, groundBounce: vec3f) -> vec3f {
    // Stand just above the surface so the horizon resolves cleanly.
    let origin = vec3f(0.0, EARTH_R + 800.0, 0.0);

    let atmosDist = raySphereFar(origin, rayDir, ATMOS_R);
    if (atmosDist < 0.0) { return vec3f(0.0); }

    // Rays heading into the planet are clipped at the surface, which is what produces
    // the dark, dense band right below the horizon.
    let groundDist = raySphereFar(origin, rayDir, EARTH_R);
    let bIn = dot(origin, rayDir);
    let cIn = dot(origin, origin) - EARTH_R * EARTH_R;
    let discr = bIn * bIn - cIn;
    var march = atmosDist;
    if (discr > 0.0) {
        let near = -bIn - sqrt(discr);
        if (near > 0.0) { march = near; }
    }

    const STEPS: i32 = 32;
    const LIGHT_STEPS: i32 = 8;

    // View samples are distributed by a power law, not uniformly, and this is the
    // single most important line in the integral.
    const DIST_POWER: f32 = 2.5;

    let mu = dot(rayDir, sunDir);
    let pr = phaseRayleigh(mu);
    let pm = phaseMie(mu, MIE_G);

    var sumR = vec3f(0.0);
    var sumM = vec3f(0.0);
    // The same two sums, over the samples that have no direct view of the sun.
    var shadR = vec3f(0.0);
    var shadM = vec3f(0.0);
    var odR = 0.0;
    var odM = 0.0;

    var tPrev = 0.0;
    for (var i = 0; i < STEPS; i++) {
        let tNext = march * pow(f32(i + 1) / f32(STEPS), DIST_POWER);
        let stepLen = tNext - tPrev;
        let p = origin + rayDir * (tPrev + stepLen * 0.5);
        tPrev = tNext;
        let h = length(p) - EARTH_R;

        let dR = exp(-h / H_RAYLEIGH) * stepLen;
        let dM = exp(-h / H_MIE) * stepLen;
        odR += dR;
        odM += dM;

        // Optical depth from this sample toward the sun.
        let lightDist = raySphereFar(p, sunDir, ATMOS_R);
        let lStep = lightDist / f32(LIGHT_STEPS);
        var lR = 0.0;
        var lM = 0.0;
        var occluded = false;

        for (var j = 0; j < LIGHT_STEPS; j++) {
            let lp = p + sunDir * (lStep * (f32(j) + 0.5));
            let lh = length(lp) - EARTH_R;
            if (lh < 0.0) { occluded = true; break; }
            lR += exp(-lh / H_RAYLEIGH) * lStep;
            lM += exp(-lh / H_MIE) * lStep;
        }

        if (occluded) {
            // Not thrown away.
            let attenV = exp(-(BETA_R * odR + BETA_M * 1.1 * odM));
            shadR += attenV * dR;
            shadM += attenV * dM;
            continue;
        }

        let tau = BETA_R * (odR + lR) + BETA_M * 1.1 * (odM + lM);
        let atten = exp(-tau);
        sumR += atten * dR;
        sumM += atten * dM;
    }

    var col = sunIntensity * (sumR * BETA_R * pr + sumM * BETA_M * pm);

    // --- multiple scattering ------------------------------------------------ Single
    // scattering alone underestimates a clear sky by roughly a factor of three, and it
    // underestimates blue the most, because a blue photon is the one most likely to
    // scatter again rather than to be absorbed.
    const SHADOW_FILL: f32 = 0.5;
    let msPhase = 1.0 / (4.0 * PI);
    col += sunIntensity * (
              (sumR + shadR * SHADOW_FILL) * BETA_R * MS_BOOST
            + (sumM + shadM * SHADOW_FILL) * BETA_M * 0.4
          ) * msPhase;

    // Below the horizon the sky is snow.
    if (discr > 0.0 && groundDist > 0.0) {
        let downT = 1.0 - smoothstep(-0.030, -0.005, rayDir.y);
        col = mix(col, groundBounce, downT);
    }

    // --- the optically thick horizon --------------------------------------- A
    // horizontal path through the atmosphere is hundreds of kilometres long, and single
    // scattering treats that as a coloured filter: blue is extinguished outright, green
    // mostly, and what is left is a saturated olive band sitting between the blue dome
    // and the warm sun.
    let grazing = 1.0 - smoothstep(0.0, 0.26, abs(rayDir.y));
    let pale = dot(col, vec3f(0.30, 0.42, 0.28));
    col = mix(col, vec3f(pale) * vec3f(0.97, 1.0, 1.06), grazing * 0.82);

    return col;
}

// ------------------------------------------------------- lat-long projection

// The sky is stored as an equirectangular 2D LUT rather than a cubemap.

fn dirToLatLong(d: vec3f) -> vec2f {
    let u = atan2(d.x, d.z) / (2.0 * PI) + 0.5;
    let v = acos(clamp(d.y, -1.0, 1.0)) / PI;
    return vec2f(u, v);
}

fn latLongToDir(uv: vec2f) -> vec3f {
    let phi = (uv.x - 0.5) * 2.0 * PI;
    let theta = uv.y * PI;
    let st = sin(theta);
    return vec3f(st * sin(phi), cos(theta), st * cos(phi));
}

// ------------------------------------------------------------------- runtime

// Height-falloff extinction.
fn aerialTransmittance(
    camPos: vec3f,
    worldPos: vec3f,
    density: f32,
    heightFalloff: f32,
    fogStart: f32
) -> f32 {
    let d = worldPos - camPos;
    let dist = max(0.0, length(d) - fogStart);
    if (dist <= 0.0) { return 1.0; }

    let dy = d.y;
    var integral: f32;
    if (abs(dy) < 0.01) {
        integral = exp(-heightFalloff * camPos.y) * dist;
    } else {
        let k = heightFalloff;
        integral = (exp(-k * camPos.y) - exp(-k * worldPos.y)) / (k * dy) * length(d);
        integral = integral * (dist / max(1e-4, length(d)));
    }

    return exp(-density * max(0.0, integral));
}

// The colour that fills a short, ground-level path.
fn aerialNearSky(tex: texture_2d<f32>, samp: sampler, viewDir: vec3f) -> vec3f {
    let d = normalize(viewDir + vec3f(0.0, 0.42, 0.0));
    return textureSampleLevel(tex, samp, dirToLatLong(d), 3.0).rgb;
}

// The inscatter colour for a path of a given total extinction.
fn aerialInscatterSky(
    tex: texture_2d<f32>, samp: sampler, viewDir: vec3f,
    sunDir: vec3f, sunColor: vec3f, ext: f32
) -> vec3f {
    // Mip 0 and no tilt: this has to match the sky material's own lookup exactly, or
    // fully hazed and sky are two different colours again.
    let exact = textureSampleLevel(tex, samp, dirToLatLong(normalize(viewDir)), 0.0).rgb;

    let mu = dot(viewDir, sunDir);
    let fwd = phaseMie(mu, 0.62) * 5.5;
    let near = aerialNearSky(tex, samp, viewDir) + sunColor * fwd * 0.16;

    // Ramps across roughly 100 m to 700 m on the default fog settings: the near field
    // keeps the cool dome and the warm sun-facing haze it is tuned for, and everything
    // past the middle distance is already on its way to the sky.
    return mix(near, exact, smoothstep(0.55, 0.995, ext));
}

// Fold aerial perspective into a shaded colour.
fn applyAerial(
    color: vec3f,
    camPos: vec3f,
    worldPos: vec3f,
    viewDir: vec3f,
    sunDir: vec3f,
    skyTex: texture_2d<f32>,
    skySamp: sampler,
    sunColor: vec3f,
    density: f32,
    heightFalloff: f32,
    fogStart: f32,
    strength: f32
) -> vec3f {
    let t = aerialTransmittance(camPos, worldPos, density, heightFalloff, fogStart);
    let ext = clamp(1.0 - pow(t, strength), 0.0, 1.0);
    let inscatter = aerialInscatterSky(skyTex, skySamp, viewDir, sunDir, sunColor, ext);
    return mix(color, inscatter, ext);
}
