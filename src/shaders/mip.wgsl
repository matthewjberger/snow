
// One level of a mip chain: a bilinear reduction of the level above.

@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var sourceSampler: sampler;

@fragment
fn fragmentMain(input: FullscreenVertex) -> @location(0) vec4f {
    return textureSampleLevel(source, sourceSampler, input.uv, 0.0);
}
