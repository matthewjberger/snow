#import snow::post_uniforms::BloomUniforms
#import snow::post_common::lumaPost

// Bloom downsample, with an optional bright pass on the first level.

@group(0) @binding(0) var<uniform> uniforms: BloomUniforms;
@group(1) @binding(0) var sourceTex: texture_2d<f32>;
@group(1) @binding(1) var linearSamp: sampler;

fn tap(uv: vec2f) -> vec3f {
    return textureSampleLevel(sourceTex, linearSamp, uv, 0.0).rgb;
}

// Soft knee threshold.
fn brightPass(color: vec3f, curve: vec4f) -> vec3f {
    let brightest = max(color.r, max(color.g, color.b));
    let over = clamp(brightest - curve.y, 0.0, curve.z);
    let soft = over * over * curve.w;
    return color * max(soft, brightest - curve.x) / max(brightest, 1e-5);
}

@fragment
fn fragmentMain(input: FullscreenVertex) -> @location(0) vec4f {
    let uv = input.uv;
    // The tap spacing is twice a source texel, not one.
    let t = uniforms.source.xy;

    let a = tap(uv + vec2f(-t.x, -t.y));
    let b = tap(uv + vec2f(t.x, -t.y));
    let c = tap(uv + vec2f(-t.x, t.y));
    let d = tap(uv + vec2f(t.x, t.y));

    let e = tap(uv + vec2f(-2.0 * t.x, -2.0 * t.y));
    let f = tap(uv + vec2f(0.0, -2.0 * t.y));
    let g = tap(uv + vec2f(2.0 * t.x, -2.0 * t.y));
    let h = tap(uv + vec2f(-2.0 * t.x, 0.0));
    let i = tap(uv);
    let j = tap(uv + vec2f(2.0 * t.x, 0.0));
    let k = tap(uv + vec2f(-2.0 * t.x, 2.0 * t.y));
    let l = tap(uv + vec2f(0.0, 2.0 * t.y));
    let m = tap(uv + vec2f(2.0 * t.x, 2.0 * t.y));

    let g0 = (a + b + c + d) * 0.25;
    let g1 = (e + f + h + i) * 0.25;
    let g2 = (f + g + i + j) * 0.25;
    let g3 = (h + i + k + l) * 0.25;
    let g4 = (i + j + l + m) * 0.25;

    var color: vec3f;
    if (uniforms.source.z > 0.5) {
        let w0 = 1.0 / (1.0 + lumaPost(g0));
        let w1 = 1.0 / (1.0 + lumaPost(g1));
        let w2 = 1.0 / (1.0 + lumaPost(g2));
        let w3 = 1.0 / (1.0 + lumaPost(g3));
        let w4 = 1.0 / (1.0 + lumaPost(g4));
        let total = w0 * 0.5 + (w1 + w2 + w3 + w4) * 0.125;
        color = (g0 * w0 * 0.5 + (g1 * w1 + g2 * w2 + g3 * w3 + g4 * w4) * 0.125)
            / max(total, 1e-5);
        color = brightPass(color, uniforms.curve);
    } else {
        color = g0 * 0.5 + (g1 + g2 + g3 + g4) * 0.125;
    }

    return vec4f(color, 1.0);
}
