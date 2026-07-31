#import snow::noise::hash22

// Bakes the tileable snow detail map used at three world scales by the snow material.

struct DetailBakeUniforms {
    // (resolution, grainScale, 0, 0)
    params: vec4f,
}

@group(0) @binding(0) var<uniform> uniforms: DetailBakeUniforms;

// Tileable hash: cell indices wrap at `period`, so the field is seamless.
fn hashTile(id: vec2f, period: f32) -> vec2f {
    let w = id - floor(id / period) * period;
    return hash22(w);
}

// Packed-grain height field.
fn grainHeight(p: vec2f, cells: f32, period: f32) -> vec2f {
    let gp = p * cells;
    let gi = floor(gp);

    var h = 0.0;
    var cav = 1.0;

    for (var dy = -1; dy <= 1; dy++) {
        for (var dx = -1; dx <= 1; dx++) {
            let id = gi + vec2f(f32(dx), f32(dy));
            let r = hashTile(id, period);
            let r2 = hashTile(id + vec2f(37.0, 91.0), period);

            let centre = id + 0.25 + r * 0.5;
            let radius = 0.30 + r2.x * 0.26;
            let d = length(gp - centre) / radius;

            if (d < 1.0) {
                // Spherical cap profile, which gives a real rounded highlight rather
                // than the soft blob a smoothstep would.
                let dome = sqrt(max(0.0, 1.0 - d * d)) * (0.55 + r2.y * 0.45);
                h = max(h, dome);
                cav = min(cav, 1.0 - (1.0 - d) * 0.5);
            }
        }
    }
    return vec2f(h, cav);
}

fn detailHeight(uv: vec2f) -> vec2f {
    // Three grain sizes stacked, each tiling on its own period.
    let a = grainHeight(uv, 26.0, 26.0);
    let b = grainHeight(uv + vec2f(0.37, 0.11), 61.0, 61.0);
    let c = grainHeight(uv + vec2f(0.71, 0.53), 137.0, 137.0);

    let h = a.x * 1.0 + b.x * 0.42 + c.x * 0.17;
    let cav = a.y * 0.55 + b.y * 0.30 + c.y * 0.15;
    return vec2f(h, cav);
}

@fragment
fn fragmentMain(input: FullscreenVertex) -> @location(0) vec4f {
    let uv = input.uv;
    let e = 1.0 / uniforms.params.x;

    let c = detailHeight(uv);
    let hL = detailHeight(uv - vec2f(e, 0.0)).x;
    let hR = detailHeight(uv + vec2f(e, 0.0)).x;
    let hD = detailHeight(uv - vec2f(0.0, e)).x;
    let hU = detailHeight(uv + vec2f(0.0, e)).x;

    // Slope of the height field in UV space, then tilted into a normal.
    let dx = (hR - hL) / (2.0 * e);
    let dz = (hU - hD) / (2.0 * e);
    let n = normalize(vec3f(-dx * uniforms.params.y, -dz * uniforms.params.y, 1.0));

    return vec4f(n.x * 0.5 + 0.5, n.y * 0.5 + 0.5, c.y, c.x);
}
