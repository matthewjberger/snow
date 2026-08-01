#import snow::post_uniforms::PostUniforms
#import snow::post_common::{POST_FAR, isBackground, viewFromDepth, uvFromView, ignPost}

// Screen-space reflections, on ice only.

@group(0) @binding(0) var<uniform> uniforms: PostUniforms;
@group(1) @binding(0) var sceneTex: texture_2d<f32>;
@group(1) @binding(1) var depthTex: texture_2d<f32>;
@group(1) @binding(2) var linearSamp: sampler;

// Coarse steps along the ray, then a short binary refine on the hit.
const STEPS: i32 = 28;
const REFINE: i32 = 5;
// Thickness the depth buffer is assumed to have, in metres.
const THICKNESS: f32 = 0.55;

// The reflection, or a negative weight when the march found nothing.
fn reflectionAt(uv: vec2f, pixel: vec2f, z: f32, mask: f32) -> vec4f {
    let miss = vec4f(0.0, 0.0, 0.0, -1.0);
    let projInfo = uniforms.projection.xy;
    let origin = viewFromDepth(uv, z, projInfo);

    // Facet normal from the depth buffer.
    let texel = uniforms.projection.zw;
    let right = textureSampleLevel(depthTex, linearSamp, uv + vec2f(texel.x, 0.0), 0.0).r;
    let up = textureSampleLevel(depthTex, linearSamp, uv + vec2f(0.0, texel.y), 0.0).r;
    if (isBackground(right) || isBackground(up)) { return miss; }

    let dx = viewFromDepth(uv + vec2f(texel.x, 0.0), right, projInfo) - origin;
    let dy = viewFromDepth(uv + vec2f(0.0, texel.y), up, projInfo) - origin;
    let normal = normalize(cross(dx, dy));

    let view = normalize(origin);
    let ray = reflect(view, normal);
    // A ray heading back toward the eye leaves the screen behind it.
    if (ray.z < 0.02) { return miss; }

    // Step length set so the ray crosses roughly one pixel per step near the surface,
    // jittered to break the banding a fixed step leaves on a flat facet.
    let stride = max(0.06, z * 0.035);
    var travel = stride * (0.5 + ignPost(pixel));
    var previous = 0.0;
    var hit = -1.0;

    for (var step = 0; step < STEPS; step++) {
        let point = origin + ray * travel;
        let screen = uvFromView(point, projInfo);
        if (any(screen < vec2f(0.0)) || any(screen > vec2f(1.0))) { break; }

        let sampled = textureSampleLevel(depthTex, linearSamp, screen, 0.0).r;
        let behind = point.z - sampled;
        if (behind > 0.0 && behind < THICKNESS) {
            var low = previous;
            var high = travel;
            for (var refine = 0; refine < REFINE; refine++) {
                let middle = (low + high) * 0.5;
                let probe = origin + ray * middle;
                let probeDepth = textureSampleLevel(
                    depthTex, linearSamp, uvFromView(probe, projInfo), 0.0
                ).r;
                if (probe.z - probeDepth > 0.0) { high = middle; } else { low = middle; }
            }
            hit = high;
            break;
        }
        previous = travel;
        // Geometric growth: the near field needs fine steps, and the far field is where
        // the ray runs out of screen anyway.
        travel += stride * (1.0 + f32(step) * 0.16);
    }

    if (hit < 0.0) { return miss; }

    let hitUv = uvFromView(origin + ray * hit, projInfo);

    // Fade at the screen edge, or the reflection ends in a hard line wherever the ray
    // ran out of buffer.
    let edge = min(min(hitUv.x, 1.0 - hitUv.x), min(hitUv.y, 1.0 - hitUv.y));
    let edgeFade = smoothstep(0.0, 0.10, edge);

    // Schlick against ice.
    let fresnel = 0.045 + 0.955 * pow(1.0 - clamp(dot(-view, normal), 0.0, 1.0), 5.0);

    let reflected = textureSampleLevel(sceneTex, linearSamp, hitUv, 0.0).rgb;
    return vec4f(
        reflected,
        clamp(mask * fresnel * edgeFade * uniforms.toggles.w, 0.0, 1.0)
    );
}

@fragment
fn fragmentMain(input: FullscreenVertex) -> @location(0) vec4f {
    let source = textureSampleLevel(sceneTex, linearSamp, input.uv, 0.0);
    let prepass = textureSampleLevel(depthTex, linearSamp, input.uv, 0.0);

    var color = source.rgb;
    if (uniforms.toggles.x > 0.5 && prepass.g >= 0.02 && !isBackground(prepass.r)) {
        let reflection = reflectionAt(input.uv, input.clip.xy, prepass.r, prepass.g);
        if (reflection.w > 0.0) { color = mix(source.rgb, reflection.rgb, reflection.w); }
    }
    return vec4f(color, source.a);
}
