#import snow::noise::noise2

// The terrain state buffer.

struct DeformUniforms {
    // (window centre this frame, window centre last frame).
    centres: vec4f,
    // (coverage in metres, texels across, seconds of relaxation, brush count)
    window: vec4f,
    // (refill rate, maximum depression, maximum berm, wind angle)
    relax: vec4f,
}

@group(0) @binding(0) var<uniform> uniforms: DeformUniforms;
@group(0) @binding(1) var prevTex: texture_2d<f32>;
@group(0) @binding(2) var prevSamp: sampler;
// Brush parameters for this frame.
@group(0) @binding(3) var brushTex: texture_2d<f32>;

// Recovers the world position a texel stands for.
fn texelWorld(uv: vec2f, centre: vec2f, size: f32) -> vec2f {
    let base = uv * size;
    return base + size * round((centre - base) / size);
}

@fragment
fn fragmentMain(input: FullscreenVertex) -> @location(0) vec4f {
    let uv = input.uv;
    let size = uniforms.window.x;
    let dt = uniforms.window.z;
    let centre = uniforms.centres.xy;
    let prevCentre = uniforms.centres.zw;
    let world = texelWorld(uv, centre, size);

    var dep = 0.0;
    var berm = 0.0;
    var comp = 0.0;
    var ice = 0.0;

    // ---------------------------------------------------------------- scroll Inside
    // last frame's window? If not, this texel just wrapped in from the trailing edge
    // and holds state from the far side of the field.
    let wasInside = all(abs(world - prevCentre) <= vec2f(size * 0.5));

    if (wasInside) {
        let t = 1.0 / uniforms.window.y;
        let c = textureSampleLevel(prevTex, prevSamp, uv, 0.0);
        let xl = textureSampleLevel(prevTex, prevSamp, uv - vec2f(t, 0.0), 0.0);
        let xr = textureSampleLevel(prevTex, prevSamp, uv + vec2f(t, 0.0), 0.0);
        let zd = textureSampleLevel(prevTex, prevSamp, uv - vec2f(0.0, t), 0.0);
        let zu = textureSampleLevel(prevTex, prevSamp, uv + vec2f(0.0, t), 0.0);

        dep = c.r;
        berm = c.g;
        comp = c.b;
        ice = c.a;

        // --- diffusion ----------------------------------------------------- Explicit
        // five-point Laplacian, so the coefficient has to stay under a quarter or it
        // goes unstable and the buffer rings.
        let refill = uniforms.relax.x;
        let k = clamp(refill * dt, 0.0, 1.0);
        let kDep = min(0.22, 0.004 * k);
        let kBerm = min(0.22, 0.012 * k);

        let lapDep = (xl.r + xr.r + zd.r + zu.r) - 4.0 * dep;
        let lapBerm = (xl.g + xr.g + zd.g + zu.g) - 4.0 * berm;
        dep += lapDep * kDep;
        berm += lapBerm * kBerm;

        // --- wind infill ---------------------------------------------------- Drift
        // blows into the trench from upwind, so pull a little of the upwind neighbour's
        // state across.
        let windAngle = uniforms.relax.w;
        let wdir = vec2f(sin(windAngle), cos(windAngle));
        let upwind = uv - wdir * (t * 1.6);
        let uw = textureSampleLevel(prevTex, prevSamp, upwind, 0.0);
        let kAdv = min(0.2, 0.002 * k);
        dep = mix(dep, uw.r, kAdv * 0.6);
        berm = mix(berm, uw.g, kAdv);

        // --- slump ---------------------------------------------------------- Piled
        // mass falls back into the hole it came out of.
        let slump = min(berm, dep) * min(0.6, 0.002 * refill * dt);
        dep -= slump;
        berm -= slump;

        // --- decay ---------------------------------------------------------- Time
        // constants in seconds, at a refill rate of one.
        dep *= exp(-dt * refill / 400.0);
        berm *= exp(-dt * refill / 250.0);
        comp *= exp(-dt * refill / 300.0);
        // Ice is the one thing here meant to feel permanent within a session: a spell
        // that permanently alters the surface should not visibly melt while the player
        // watches it.
        ice *= exp(-dt * refill / 900.0);
    }

    // ----------------------------------------------------------------- splat
    let n = i32(uniforms.window.w);
    for (var i = 0; i < n; i++) {
        let a = textureLoad(brushTex, vec2i(i, 0), 0);
        let b = textureLoad(brushTex, vec2i(i, 1), 0);
        let c = textureLoad(brushTex, vec2i(i, 2), 0);

        let radius = a.z;
        if (radius <= 0.0) { continue; }

        // Wrap the offset too, so a brush written near the seam still reaches the
        // texels on the far side of it.
        var p = world - a.xy;
        p -= size * round(p / size);

        // Cheap reject before the trigonometry.
        let reach = radius * max(a.w, 1.0) * 1.6;
        if (abs(p.x) > reach || abs(p.y) > reach) { continue; }

        // Into brush space: rotate by the brush yaw, then squash the long axis.
        let q = vec2f(
            (p.x * b.x + p.y * b.y) / (radius * a.w),
            (-p.x * b.y + p.y * b.x) / radius
        );
        let d = length(q);
        if (d > 1.55) { continue; }

        // Contact detail.
        let ang = atan2(q.y, q.x);
        let wob = 1.0 + c.z * 0.22 * noise2(vec2f(cos(ang), sin(ang)) * 2.7 + c.w);
        let dn = d / wob;

        // Depression: a flattish floor, then a fast shoulder.
        let core = 1.0 - smoothstep(0.42, 1.0, dn);

        // Berm: a ring sitting just outside the depression rim, where the displaced
        // mass actually ends up.
        let ringD = (dn - 1.04) * 3.4;
        let ring = exp(-ringD * ringD);
        let grain = 0.72 + 0.56 * (noise2(q * 7.5 + c.w * 3.1) * 0.5 + 0.5);

        dep += b.z * core;
        berm += b.w * ring * grain;
        comp += c.x * core;
        ice = max(ice, c.y * core);
    }

    // ----------------------------------------------------------------- clamp The
    // depression bottoms out: below about half a metre it is packed snow and nothing
    // more moves.
    dep = clamp(dep, 0.0, uniforms.relax.y);
    berm = clamp(berm, 0.0, uniforms.relax.z);
    comp = clamp(comp, 0.0, 1.0);
    ice = clamp(ice, 0.0, 1.0);

    return vec4f(dep, berm, comp, ice);
}
