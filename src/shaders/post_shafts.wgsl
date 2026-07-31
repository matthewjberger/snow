#import snow::post_uniforms::PostUniforms
#import snow::post_common::{isBackground, ignPost}

// Volumetric light shafts.

@group(0) @binding(0) var<uniform> uniforms: PostUniforms;
@group(1) @binding(0) var depthTex: texture_2d<f32>;
@group(1) @binding(1) var linearSamp: sampler;

const STEPS: i32 = 24;
// How far along the ray to the sun the march reaches.
const REACH: f32 = 0.82;
// Per-step attenuation, which sets how far a shaft runs before it dies out.
const DECAY: f32 = 0.955;

// Sky visibility integrated along the ray toward the sun.
fn marchShaft(uv: vec2f, pixel: vec2f, radial: f32) -> f32 {
    let delta = (uniforms.sun.xy - uv) * (REACH / f32(STEPS));

    // Dither the start, or twenty-four steps quantise into visible rings around the
    // sun.
    var point = uv + delta * ignPost(pixel);

    var illumination = 1.0;
    var accumulated = 0.0;
    for (var step = 0; step < STEPS; step++) {
        let z = textureSampleLevel(depthTex, linearSamp, point, 0.0).r;
        accumulated += select(0.0, illumination, isBackground(z));
        illumination *= DECAY;
        point += delta;
    }
    accumulated /= f32(STEPS);

    // Squared, so a beam that is half occluded reads as clearly dimmer than one that is
    // not.
    return accumulated * accumulated * radial * uniforms.sunColor.w;
}

@fragment
fn fragmentMain(input: FullscreenVertex) -> @location(0) vec4f {
    // Angular weight.
    let offset = (input.uv - uniforms.sun.xy) * vec2f(uniforms.sun.w, 1.0);
    let radial = 1.0 - smoothstep(0.03, 0.68, length(offset));

    var amount = 0.0;
    if (uniforms.focus.w > 0.5 && uniforms.sun.z > 0.5 && radial > 0.001) {
        amount = marchShaft(input.uv, input.clip.xy, radial);
    }
    return vec4f(uniforms.sunColor.rgb * amount, 1.0);
}
