pub mod bake;
pub mod character;
pub mod character_geometry;
pub mod cloth_geometry;
pub mod crystal;
pub mod deform;
pub mod frame;
pub mod geometry;
pub mod gpu;
pub mod pipelines;
pub mod post;
pub mod readback;
pub mod sky;
pub mod spray;
pub mod terrain;
pub mod uniforms;
pub mod wake;
pub mod water;
pub mod world;

use crate::constants::HDR_FORMAT;
use crate::shaders;
use nightshade::prelude::*;

/// Engine passes the demo replaces outright.
const REPLACED_ENGINE_PASSES: [&str; 8] = [
    "sky_pass",
    "shadow_depth_pass",
    "mesh_pass",
    "decal_pass",
    "water_pass",
    "cloth_pass",
    "scene_overlay_pass",
    "postprocess_pass",
];

pub fn configure_render_graph(
    graph: &mut RenderGraph<RenderInputs>,
    device: &wgpu::Device,
    surface_format: wgpu::TextureFormat,
    resources: RenderResources,
) {
    let width = resources.surface_width.max(1);
    let height = resources.surface_height.max(1);
    let mut library = shaders::new();

    // The demo owns the scene targets, because it renders at its own internal
    // resolution while the graph sizes its transients to the surface for the
    // engine's present and screenshot paths. Only the final image is a graph
    // resource, and it stays full size.
    render_graph_pass(
        graph,
        Box::new(world::new(device, &mut library, HDR_FORMAT)),
    )
    .add()
    .expect("snow world pass");

    render_graph_pass(
        graph,
        Box::new(post::new(
            device,
            &mut library,
            surface_format,
            width,
            height,
        )),
    )
    .slot("output", resources.compute_output)
    .add()
    .expect("snow post chain");

    // With no shared slots left, the ordering is stated outright: the chain
    // resolves the scene the world pass drew.
    render_graph_add_dependency(graph, "snow_post", "snow_world").expect("snow pass ordering");
}

pub fn update_render_graph(graph: &mut RenderGraph<RenderInputs>, _world: &World) {
    for name in REPLACED_ENGINE_PASSES {
        if render_graph_get_pass_mut(graph, name).is_none() {
            continue;
        }
        if let Err(error) = render_graph_set_pass_enabled(graph, name, false) {
            panic!("snow could not disable the {name} pass: {error}");
        }
    }
}
