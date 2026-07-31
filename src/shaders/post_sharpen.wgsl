#import snow::post_uniforms::PostUniforms

// Contrast adaptive sharpen, after the display transform.

@group(0) @binding(0) var<uniform> uniforms: PostUniforms;
@group(1) @binding(0) var sourceTex: texture_2d<f32>;
@group(1) @binding(1) var linearSamp: sampler;

@fragment
fn fragmentMain(input: FullscreenVertex) -> @location(0) vec4f {
    let uv = input.uv;
    let centre = textureSampleLevel(sourceTex, linearSamp, uv, 0.0);

    var color = centre.rgb;
    let amount = uniforms.toggles.z;
    if (amount >= 0.001) {
        let t = uniforms.projection.zw;
        let left = textureSampleLevel(sourceTex, linearSamp, uv - vec2f(t.x, 0.0), 0.0).rgb;
        let right = textureSampleLevel(sourceTex, linearSamp, uv + vec2f(t.x, 0.0), 0.0).rgb;
        let down = textureSampleLevel(sourceTex, linearSamp, uv - vec2f(0.0, t.y), 0.0).rgb;
        let up = textureSampleLevel(sourceTex, linearSamp, uv + vec2f(0.0, t.y), 0.0).rgb;

        let low = min(centre.rgb, min(min(left, right), min(down, up)));
        let high = max(centre.rgb, max(max(left, right), max(down, up)));

        let k = amount * 0.32;
        color = clamp(
            centre.rgb * (1.0 + 4.0 * k) - (left + right + down + up) * k,
            low,
            high
        );
    }
    return vec4f(color, centre.a);
}
