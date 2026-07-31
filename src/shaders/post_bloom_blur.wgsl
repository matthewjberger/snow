#import snow::post_uniforms::BloomUniforms

// Nine tap tent blur, at the bottom of the bloom chain.

@group(0) @binding(0) var<uniform> uniforms: BloomUniforms;
@group(1) @binding(0) var sourceTex: texture_2d<f32>;
@group(1) @binding(1) var linearSamp: sampler;

fn tap(uv: vec2f) -> vec3f {
    return textureSampleLevel(sourceTex, linearSamp, uv, 0.0).rgb;
}

@fragment
fn fragmentMain(input: FullscreenVertex) -> @location(0) vec4f {
    let uv = input.uv;
    let t = uniforms.source.xy;

    var color = tap(uv + vec2f(-t.x, t.y));
    color += tap(uv + vec2f(0.0, t.y)) * 2.0;
    color += tap(uv + vec2f(t.x, t.y));
    color += tap(uv + vec2f(-t.x, 0.0)) * 2.0;
    color += tap(uv) * 4.0;
    color += tap(uv + vec2f(t.x, 0.0)) * 2.0;
    color += tap(uv + vec2f(-t.x, -t.y));
    color += tap(uv + vec2f(0.0, -t.y)) * 2.0;
    color += tap(uv + vec2f(t.x, -t.y));

    return vec4f(color * (1.0 / 16.0), 1.0);
}
