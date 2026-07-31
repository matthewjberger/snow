#define_import_path snow::post_common

// The conventions every screen-space pass shares.

// Cleared value of the depth prepass, which is past the camera's far plane.
const POST_FAR: f32 = 9000.0;

// True where the prepass wrote nothing: sky, or a discarded fragment.
fn isBackground(z: f32) -> bool {
    return z > POST_FAR * 0.5;
}

fn ndcFromUv(uv: vec2f) -> vec2f {
    return vec2f(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
}

fn uvFromNdc(ndc: vec2f) -> vec2f {
    return vec2f(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
}

// View-space position of a pixel.
fn viewFromDepth(uv: vec2f, z: f32, projInfo: vec2f) -> vec3f {
    let ndc = ndcFromUv(uv);
    return vec3f(ndc.x * projInfo.x, ndc.y * projInfo.y, 1.0) * z;
}

// Screen UV of a view-space position, the inverse of `viewFromDepth`.
fn uvFromView(p: vec3f, projInfo: vec2f) -> vec2f {
    return uvFromNdc(vec2f(p.x / (projInfo.x * p.z), p.y / (projInfo.y * p.z)));
}

// Interleaved gradient noise, the cheapest per-pixel dither the temporal / resolve
// integrates cleanly, because its spectrum is close to blue over a / three by three
// neighbourhood.
fn ignPost(p: vec2f) -> f32 {
    return fract(52.9829189 * fract(dot(p, vec2f(0.06711056, 0.00583715))));
}

// Rec.
fn lumaPost(c: vec3f) -> f32 {
    return dot(c, vec3f(0.2126, 0.7152, 0.0722));
}

// Karis' tonemap and inverse pair.
fn tonemapWeight(c: vec3f) -> vec3f {
    return c / (1.0 + lumaPost(c));
}

fn tonemapUnweight(c: vec3f) -> vec3f {
    return c / max(1e-4, 1.0 - lumaPost(c));
}
