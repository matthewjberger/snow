#import snow::atmosphere::{latLongToDir, nishitaSky}

// Bakes the atmospheric scattering integral into an equirectangular LUT. Re-run only
// when the sun moves, never per frame.

struct SkyBakeUniforms {
    // (sunDir.xyz, sunIntensity)
    sun: vec4f,
    // (groundBounce.rgb, 0)
    bounce: vec4f,
}

@group(0) @binding(0) var<uniform> uniforms: SkyBakeUniforms;

@fragment
fn fragmentMain(input: FullscreenVertex) -> @location(0) vec4f {
    let dir = latLongToDir(input.uv);
    let col = nishitaSky(dir, uniforms.sun.xyz, uniforms.sun.w, uniforms.bounce.rgb);
    return vec4f(col, 1.0);
}
