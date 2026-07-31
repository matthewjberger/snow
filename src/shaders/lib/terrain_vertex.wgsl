#define_import_path snow::terrain_vertex

#import snow::clipmap::{placeClipmapVertex, sampleHeightBicubic, worldToHeightUV}
#import snow::terrain::terrainFine
#import snow::deform::deformHeight

// Where a terrain vertex actually lands.

struct TerrainVertex {
    world: vec3f,
    heightUV: vec2f,
    // This vertex's effective sample spacing, post-morph.
    spacing: f32,
}

fn placeTerrainVertex(
    heightTex: texture_2d<f32>,
    heightSamp: sampler,
    auxTex: texture_2d<f32>,
    auxSamp: sampler,
    deformTex: texture_2d<f32>,
    deformSamp: sampler,
    grid: vec2f,
    level: f32,
    // (ring centre xz, base spacing, grid half extent)
    clipmap: vec4f,
    // (world origin xz, world size, height resolution)
    field: vec4f,
    // (wind angle, macro amplitude, sastrugi amplitude, unused)
    surface: vec4f,
    // (deform centre xz, deform size, deform texel)
    deform: vec4f,
    deformScale: f32
) -> TerrainVertex {
    let cv = placeClipmapVertex(grid, level, clipmap.xy, clipmap.z, clipmap.w);

    let worldXZ = cv.worldXZ;
    let hUV = worldToHeightUV(worldXZ, field.xy, field.z);

    var h = sampleHeightBicubic(heightTex, heightSamp, hUV, field.w);

    // Displaced only where the ring is fine enough to resolve it.
    let exposure = textureSampleLevel(auxTex, auxSamp, hUV, 0.0).a;
    if (cv.spacing < 0.42) {
        let fade = 1.0 - smoothstep(0.16, 0.42, cv.spacing);
        h += terrainFine(worldXZ, surface.x, exposure, surface.z).x * fade;
    }

    // Real displacement, not a normal-map trick: a trench the player can see the far
    // wall of, and berms that break the silhouette against the sky.
    if (cv.spacing < 1.0) {
        let dfade = 1.0 - smoothstep(0.5, 1.0, cv.spacing);
        h += deformHeight(
            deformTex, deformSamp, worldXZ,
            deform.xy, deform.z, deformScale, cv.spacing
        ) * dfade;
    }

    var out: TerrainVertex;
    out.world = vec3f(worldXZ.x, h, worldXZ.y);
    out.heightUV = hUV;
    out.spacing = cv.spacing;
    return out;
}
