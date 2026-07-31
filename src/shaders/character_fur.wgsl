#import snow::char_uniforms::CharUniforms
#import snow::char_skin::{skinPoint1, skinDir1}
#import snow::noise::{hash21, hash22, ign}
#import snow::shading::{
    wrapDiffuse, backScatter, distributionGGX, shIrradiance
}
#import snow::shadow_lookup::sunShadow
#import snow::atmosphere::applyAerial

// Shell fur.

@group(0) @binding(0) var<uniform> uniforms: CharUniforms;

@group(1) @binding(0) var charTex: texture_2d<f32>;
@group(1) @binding(1) var skyLUT: texture_2d<f32>;
@group(1) @binding(2) var skySamp: sampler;
@group(1) @binding(3) var cascade0: texture_2d<f32>;
@group(1) @binding(4) var cascade1: texture_2d<f32>;
@group(1) @binding(5) var cascade2: texture_2d<f32>;
@group(1) @binding(6) var cascadeSamp: sampler;

struct VertexInput {
    // Bind-pose world position, with the shell offset already included.
    @location(0) position: vec3f,
    // Shell direction, unit length.
    @location(1) normal: vec3f,
    // Strand field coordinates, in metres of surface.
    @location(2) uv: vec2f,
    // (shell parameter, baked occlusion)
    @location(3) aux: vec2f,
    @location(4) boneIndex: vec4f,
    @location(5) boneWeight: vec4f,
}

struct Varyings {
    @builtin(position) clip: vec4f,
    @location(0) world: vec3f,
    @location(1) normal: vec3f,
    @location(2) uv: vec2f,
    @location(3) aux: vec2f,
    @location(4) viewDist: f32,
}

// The shell offset is baked into the vertex at build time, so all the vertex / stage
// adds is droop: gravity, wind and the character's own acceleration, / applied in world
// space and scaled by the square of the shell parameter.
@vertex
fn vertexMain(input: VertexInput) -> Varyings {
    let bone = i32(input.boneIndex.x);
    var world = skinPoint1(charTex, bone, input.position);
    let normal = normalize(skinDir1(charTex, bone, input.normal));

    let shell = input.aux.x;
    world += uniforms.fur.xyz * (shell * shell);

    var out: Varyings;
    out.world = world;
    out.normal = normal;
    out.uv = input.uv;
    out.aux = input.aux;
    out.viewDist = distance(world, uniforms.camera.xyz);
    out.clip = uniforms.viewProjection * vec4f(world, 1.0);
    return out;
}

@fragment
fn fragmentMain(input: Varyings) -> @location(0) vec4f {
    let shell = input.aux.x;

    let grid = input.uv * uniforms.fur.w;
    let cell = floor(grid);
    let hashed = hash21(cell);
    let jitter = hash22(cell + vec2f(11.3, 5.7)) - 0.5;

    // How far up this strand reaches.
    let strandLength = 0.30 + 0.70 * hashed;
    if (shell > strandLength) { discard; }

    // Distance to the strand's own axis, in cell units, and a taper that is full width
    // at the root and a point at the tip.
    let distance = length(fract(grid) - 0.5 - jitter * 0.55);
    let taper = 1.0 - (shell / strandLength);
    let radius =
        0.46 * (0.55 + 0.45 * hash21(cell + vec2f(3.1, 9.4))) * sqrt(max(taper, 0.0));
    if (distance > radius) { discard; }

    let world = input.world;
    let V = normalize(uniforms.camera.xyz - world);
    let L = uniforms.sunDir.xyz;
    var N = normalize(input.normal);
    if (dot(N, V) < 0.0) { N = -N; }

    let noiseRot = ign(input.clip.xy) * 6.28318530718;
    let shadow = sunShadow(
        cascade0, cascadeSamp, cascade1, cascadeSamp, cascade2, cascadeSamp,
        uniforms.shadow, world, N, input.viewDist, noiseRot
    );

    // Self-occlusion down the stack.
    let depth = shell / max(strandLength, 1e-3);
    let selfOcclusion = 0.16 + 0.84 * depth * depth;

    const INV_PI: f32 = 0.31830988618;
    let sun = uniforms.sunRadiance.rgb;
    let furColor = uniforms.furColor.rgb;
    let NdotL = dot(N, L);

    // Fibres wrap light almost all the way round.
    let diffuse = wrapDiffuse(NdotL, 0.65);
    var color = furColor * INV_PI * sun * diffuse * shadow * selfOcclusion;

    // The term that makes a fur rim light up against a low sun.
    let back = backScatter(N, L, V, 0.5, 3.0, 1.0);
    color += sun * furColor * back * 0.85 * mix(0.4, 1.0, shadow) * selfOcclusion;

    // A dim, wide specular.
    if (NdotL > 0.0) {
        let H = normalize(V + L);
        let d = distributionGGX(clamp(dot(N, H), 0.0, 1.0), 0.75);
        color += sun * d * 0.05 * NdotL * shadow * selfOcclusion;
    }

    let irradiance = shIrradiance(N, uniforms.harmonics) * uniforms.misc.x;
    color += furColor * INV_PI * irradiance * selfOcclusion * input.aux.y * 1.4;

    color = applyAerial(
        color, uniforms.camera.xyz, world, -V, L,
        skyLUT, skySamp, sun,
        uniforms.fog.x, uniforms.fog.y, uniforms.fog.z, uniforms.fog.w
    );

    return vec4f(color, 1.0);
}
