#import snow::post_uniforms::PostUniforms
#import snow::post_common::{isBackground, ignPost}

// Depth of field, very restrained.

@group(0) @binding(0) var<uniform> uniforms: PostUniforms;
@group(1) @binding(0) var sceneTex: texture_2d<f32>;
@group(1) @binding(1) var depthTex: texture_2d<f32>;
@group(1) @binding(2) var linearSamp: sampler;

const TAPS: i32 = 16;
const GOLDEN: f32 = 2.39996323;

// Where the far defocus starts and where it saturates, in metres.
const FAR_START: f32 = 130.0;
const FAR_FULL: f32 = 620.0;

// Signed circle of confusion, minus one near to plus one far, before the pixel /
// scale.
fn circleOfConfusion(z: f32, focus: f32) -> f32 {
    if (isBackground(z)) { return 1.0; }
    let far = smoothstep(FAR_START, FAR_FULL, z);
    // The near side stays keyed to the focal distance, because that is the right anchor
    // for it: the near limit is a property of the subject distance, and it is the one
    // place this effect earns its keep.
    let near = smoothstep(focus * 0.55, focus * 0.16, z);
    return far - near;
}

// The gather, weighted by each tap's own circle of confusion.
fn gather(uv: vec2f, pixel: vec2f, radius: f32, centre: vec3f) -> vec3f {
    let rotation = ignPost(pixel) * 6.28318530718;
    let invRes = uniforms.projection.zw;
    let focus = uniforms.focus.x;
    let maxCoc = uniforms.focus.y;

    var accumulated = centre;
    var total = 1.0;
    for (var index = 0; index < TAPS; index++) {
        let step = f32(index) + 0.5;
        let angle = rotation + step * GOLDEN;
        let offset = radius * sqrt(step / f32(TAPS));
        let uvTap = uv + vec2f(cos(angle), sin(angle)) * offset * invRes;

        let z = textureSampleLevel(depthTex, linearSamp, uvTap, 0.0).r;
        let coc = circleOfConfusion(z, focus);
        // A tap only contributes if its own blur circle is wide enough to reach this
        // pixel.
        let weight = clamp(abs(coc) * maxCoc - offset + 1.0, 0.0, 1.0);
        accumulated += textureSampleLevel(sceneTex, linearSamp, uvTap, 0.0).rgb * weight;
        total += weight;
    }
    return accumulated / total;
}

@fragment
fn fragmentMain(input: FullscreenVertex) -> @location(0) vec4f {
    let centre = textureSampleLevel(sceneTex, linearSamp, input.uv, 0.0);

    var color = centre.rgb;
    if (uniforms.focus.z > 0.5) {
        let z = textureSampleLevel(depthTex, linearSamp, input.uv, 0.0).r;
        let radius = abs(circleOfConfusion(z, uniforms.focus.x)) * uniforms.focus.y;
        // Under a pixel and a half there is nothing a gather can do that the display
        // transform will not throw away, and this is the branch almost the whole frame
        // takes.
        if (radius >= 1.5) {
            color = gather(input.uv, input.clip.xy, radius, centre.rgb);
        }
    }
    return vec4f(color, centre.a);
}
