#import snow::post_uniforms::PostUniforms
#import snow::post_common::{POST_FAR, tonemapWeight, tonemapUnweight}

// Temporal anti-aliasing.

@group(0) @binding(0) var<uniform> uniforms: PostUniforms;
@group(1) @binding(0) var sceneTex: texture_2d<f32>;
@group(1) @binding(1) var historyTex: texture_2d<f32>;
@group(1) @binding(2) var depthTex: texture_2d<f32>;
@group(1) @binding(3) var linearSamp: sampler;

// Five-tap Catmull-Rom fetch of the history.
fn historyCatmullRom(uv: vec2f, size: vec2f) -> vec3f {
    let samplePosition = uv * size;
    let centre = floor(samplePosition - 0.5) + 0.5;
    let f = samplePosition - centre;

    let w0 = f * (-0.5 + f * (1.0 - 0.5 * f));
    let w1 = 1.0 + f * f * (-2.5 + 1.5 * f);
    let w2 = f * (0.5 + f * (2.0 - 1.5 * f));
    let w3 = f * f * (-0.5 + 0.5 * f);

    let w12 = w1 + w2;
    let offset12 = w2 / w12;

    let p0 = (centre - 1.0) / size;
    let p3 = (centre + 2.0) / size;
    let p12 = (centre + offset12) / size;

    var accumulated = vec3f(0.0);
    accumulated += textureSampleLevel(historyTex, linearSamp, vec2f(p12.x, p0.y), 0.0).rgb
        * (w12.x * w0.y);
    accumulated += textureSampleLevel(historyTex, linearSamp, vec2f(p0.x, p12.y), 0.0).rgb
        * (w0.x * w12.y);
    accumulated += textureSampleLevel(historyTex, linearSamp, vec2f(p12.x, p12.y), 0.0).rgb
        * (w12.x * w12.y);
    accumulated += textureSampleLevel(historyTex, linearSamp, vec2f(p3.x, p12.y), 0.0).rgb
        * (w3.x * w12.y);
    accumulated += textureSampleLevel(historyTex, linearSamp, vec2f(p12.x, p3.y), 0.0).rgb
        * (w12.x * w3.y);

    // The negative lobes can undershoot past zero on a hard edge, and a negative
    // radiance survives the clip below as a black fringe.
    return max(accumulated, vec3f(0.0));
}

fn resolve(uv: vec2f) -> vec3f {
    let current = textureSampleLevel(sceneTex, linearSamp, uv, 0.0).rgb;
    if (uniforms.toggles.y < 0.5 || uniforms.temporal.z < 0.5) { return current; }

    // The depth stored here was rasterised through the jittered projection, so the ray
    // this pixel actually looked along is the jittered one.
    let projInfo = uniforms.projection.xy;
    let invRes = uniforms.projection.zw;
    let z = textureSampleLevel(depthTex, linearSamp, uv, 0.0).r;
    let ndc = vec2f(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0) - uniforms.temporal.xy;
    let view = vec3f(ndc.x * projInfo.x, ndc.y * projInfo.y, 1.0) * min(z, POST_FAR);
    let world = uniforms.invView * vec4f(view, 1.0);

    let previousClip = uniforms.prevViewProj * vec4f(world.xyz, 1.0);
    if (previousClip.w <= 1e-4) { return current; }

    let projected = previousClip.xy / previousClip.w;
    let previousUv = vec2f(projected.x * 0.5 + 0.5, 0.5 - projected.y * 0.5);

    // Off the edge of last frame is a disocclusion by definition.
    if (any(previousUv < vec2f(0.0)) || any(previousUv > vec2f(1.0))) { return current; }

    // Variance clipping rather than a minimum and maximum box.
    var first = vec3f(0.0);
    var second = vec3f(0.0);
    for (var j = -1; j <= 1; j++) {
        for (var i = -1; i <= 1; i++) {
            let tap = tonemapWeight(textureSampleLevel(
                sceneTex, linearSamp, uv + vec2f(f32(i), f32(j)) * invRes, 0.0
            ).rgb);
            first += tap;
            second += tap * tap;
        }
    }
    let mean = first / 9.0;
    let deviation = sqrt(max(vec3f(0.0), second / 9.0 - mean * mean));
    let low = mean - deviation * 1.35;
    let high = mean + deviation * 1.35;

    var raw = tonemapWeight(historyCatmullRom(previousUv, 1.0 / invRes));
    // Guard the uninitialised-history case a second time.
    if (any(raw != raw)) { raw = mean; }
    let history = clamp(raw, low, high);
    let weighted = tonemapWeight(current);

    // Two things pull the feedback down.
    let travelled = length((previousUv - uv) / invRes);
    let motionFade = 1.0 - clamp(travelled / 64.0, 0.0, 1.0) * 0.35;
    let clipFade = 1.0 - clamp(length(history - raw) * 4.0, 0.0, 1.0) * 0.45;

    let keep = clamp(uniforms.temporal.w * motionFade * clipFade, 0.0, 0.97);
    return tonemapUnweight(mix(weighted, history, keep));
}

@fragment
fn fragmentMain(input: FullscreenVertex) -> @location(0) vec4f {
    return vec4f(resolve(input.uv), 1.0);
}
