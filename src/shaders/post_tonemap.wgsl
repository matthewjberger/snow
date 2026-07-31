#import snow::post_uniforms::PostUniforms

// The composite: shafts, bloom, exposure, the display transform, grain.

@group(0) @binding(0) var<uniform> uniforms: PostUniforms;
@group(1) @binding(0) var sceneTex: texture_2d<f32>;
// Quarter resolution bright pass: the tight glow around a glint or the sun.
@group(1) @binding(1) var bloomNear: texture_2d<f32>;
// Sixteenth resolution and blurred: the broad halo that reads as atmosphere.
@group(1) @binding(2) var bloomFar: texture_2d<f32>;
@group(1) @binding(3) var shaftsTex: texture_2d<f32>;
@group(1) @binding(4) var linearSamp: sampler;

const AGX_IN = mat3x3f(
    0.842479062253094, 0.0423282422610123, 0.0423756549057051,
    0.0784335999999992, 0.878468636469772, 0.0784336,
    0.0792237451477643, 0.0791661274605434, 0.879142973793104
);

const AGX_OUT = mat3x3f(
    1.19687900512017, -0.0528968517574562, -0.0529716355144438,
    -0.0980208811401368, 1.15190312990417, -0.0980434501171241,
    -0.0990297440797205, -0.0989611768448433, 1.15107367264116
);

// Sixth order fit of the AgX contrast curve.
fn agxContrast(x: vec3f) -> vec3f {
    let x2 = x * x;
    let x4 = x2 * x2;
    return 15.5 * x4 * x2
        - 40.14 * x4 * x
        + 31.96 * x4
        - 6.868 * x2 * x
        + 0.4298 * x2
        + 0.1191 * x
        - 0.00232;
}

fn agx(color: vec3f) -> vec3f {
    const MIN_EV: f32 = -12.47393;
    const MAX_EV: f32 = 4.026069;

    var v = AGX_IN * max(color, vec3f(0.0));
    v = clamp(log2(max(v, vec3f(1e-10))), vec3f(MIN_EV), vec3f(MAX_EV));
    v = (v - MIN_EV) / (MAX_EV - MIN_EV);
    return agxContrast(v);
}

// Gentle saturation recovery.
fn agxLook(color: vec3f, saturation: f32) -> vec3f {
    let weights = vec3f(0.2126, 0.7152, 0.0722);
    let luminance = dot(color, weights);
    return max(vec3f(0.0), luminance + (color - luminance) * saturation);
}

fn acesFitted(x: vec3f) -> vec3f {
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), vec3f(0.0), vec3f(1.0));
}

fn linearToSrgb(c: vec3f) -> vec3f {
    let low = c * 12.92;
    let high = 1.055 * pow(max(c, vec3f(0.0)), vec3f(1.0 / 2.4)) - 0.055;
    return select(high, low, c <= vec3f(0.0031308));
}

// Speed streaks: two effects, both gated on the same value and both confined to the
// periphery, because that is where speed is actually read.

fn streakStrands(offset: vec2f, radius: f32, time: f32) -> f32 {
    let angle = atan2(offset.y, offset.x) * 96.0;
    let cell = floor(angle);
    let hash = fract(sin(cell * 12.9898 + 4.1) * 43758.5453);
    // Only a fraction of the angular cells carry a strand.
    if (hash > 0.34) { return 0.0; }

    let across = abs(fract(angle) - 0.5) * 2.0;
    // The radial frequency is the number that decides whether this reads as blowing
    // snow or as scratches on the lens.
    let phase = fract(radius * (11.0 + hash * 24.0) - time * (7.0 + hash * 22.0));
    let segment = smoothstep(0.55, 0.86, phase) * (1.0 - smoothstep(0.86, 1.0, phase));
    return pow(1.0 - across, 20.0) * segment;
}

@fragment
fn fragmentMain(input: FullscreenVertex) -> @location(0) vec4f {
    let uv = input.uv;
    var color = textureSampleLevel(sceneTex, linearSamp, uv, 0.0).rgb;

    let toCentre = uv - vec2f(0.5, 0.5);
    let radius = length(toCentre) * 2.0;
    let streak = uniforms.look.z * smoothstep(0.34, 1.05, radius);
    if (streak > 0.002) {
        var accumulated = color;
        for (var index = 1; index <= 6; index++) {
            let step = f32(index) / 6.0 * streak * 0.026;
            accumulated += textureSampleLevel(
                sceneTex, linearSamp, uv - toCentre * step, 0.0
            ).rgb;
        }
        color = mix(color, accumulated / 7.0, 0.88);
    }

    // Light shafts, in scene radiance so the tone curve rolls them off with everything
    // else.
    if (uniforms.focus.w > 0.0001) {
        color += textureSampleLevel(shaftsTex, linearSamp, uv, 0.0).rgb * uniforms.focus.w;
    }

    color *= uniforms.tone.x;

    // Bloom.
    if (uniforms.look.w > 0.0001) {
        let near = textureSampleLevel(bloomNear, linearSamp, uv, 0.0).rgb;
        let far = textureSampleLevel(bloomFar, linearSamp, uv, 0.0).rgb;
        // Weighted toward the wide level: a tight halo on a snow field reads as a
        // rendering artefact, a broad one reads as glare in the air.
        color += (near * 0.35 + far * 0.65) * uniforms.look.w;
    }

    // Blown snow, added in exposed linear so its brightness is stated relative to
    // middle grey rather than to whatever the scene happens to be sitting at.
    if (streak > 0.002) {
        let strands = streakStrands(toCentre, radius, uniforms.look.x);
        color += vec3f(0.88, 0.94, 1.06) * strands * streak * 0.16;
    }

    // Contrast about middle grey, applied in linear before the curve so it pushes into
    // the tonemapper's shoulder rather than clipping after it.
    let contrast = uniforms.tone.y;
    if (abs(contrast - 1.0) > 0.001) {
        color = 0.18 * pow(max(color / 0.18, vec3f(1e-5)), vec3f(contrast));
    }

    // Every branch below emits display linear, ready for one encode.
    var mapped: vec3f;
    if (uniforms.tone.z < 0.5) {
        // AgX's contrast polynomial already emits display encoded values, so it needs
        // its transfer function applied before the shared encode at the bottom.
        var v = agx(color);
        v = agxLook(v, 1.14);
        mapped = pow(max(AGX_OUT * v, vec3f(0.0)), vec3f(2.2));
    } else if (uniforms.tone.z < 1.5) {
        mapped = acesFitted(color);
    } else {
        mapped = clamp(color, vec3f(0.0), vec3f(1.0));
    }

    // Vignette, very slight: enough to keep the eye centred on a scene with no
    // interface to anchor it.
    let vignette = uniforms.look.y;
    if (vignette > 0.001) {
        let distance = length(uv - vec2f(0.5)) * 1.414;
        // Written the ascending way round on purpose: WGSL leaves smoothstep
        // undefined when low is above high, and the browser rejects it outright
        // where a native driver quietly accepts it.
        let falloff = 1.0 - smoothstep(0.35, 1.05, distance);
        mapped *= mix(1.0, falloff, vignette);
    }

    var encoded = linearToSrgb(mapped);

    // Grain, added after the encode so it reads evenly across the range instead of
    // vanishing in the shadows.
    let grain = uniforms.tone.w;
    if (grain > 0.0001) {
        let time = uniforms.look.x;
        let noise = fract(sin(dot(
            uv * vec2f(1920.0, 1080.0) + vec2f(time * 91.7, time * 43.3),
            vec2f(12.9898, 78.233)
        )) * 43758.5453);
        encoded += (noise - 0.5) * grain;
    }

    return vec4f(encoded, 1.0);
}
