use crate::ecs::{SnowResources, register_snow_components};
use crate::input::{SnowInput, poll_input_system};
use crate::render;
use crate::settings::Settings;
use crate::systems::Perf;
use crate::systems::overlay::overlay_system;
use crate::systems::simulate::{present_system, simulate_system};
use crate::systems::spell::cast_system;
use nightshade::prelude::*;

pub struct SnowPlugin;

impl Plugin for SnowPlugin {
    fn build(&self, app: &mut App) {
        app.world.res_mut::<Window>().title = "Snow".to_string();

        app.insert_resource(Settings::default());
        app.insert_resource(SnowInput::default());
        app.insert_resource(Perf::default());
        app.insert_resource(SnowResources::default());

        app.add_system(Stage::Startup, initialize);
        app.add_systems(
            Stage::Update,
            (
                poll_input_system,
                simulate_system,
                cast_system,
                present_system,
            ),
        );
        app.add_system(Stage::LateUpdate, overlay_system);

        app.add_render_graph_config(render::configure_render_graph);
        app.on_update_render_graph(render::update_render_graph);
        app.on_pre_render(render::frame::pre_render);
    }
}

fn initialize(snow: &mut SnowResources, world: &mut World) {
    world.ecs.add_world_at(GAME, register_snow_components());

    world.res_mut::<Window>().use_fullscreen = true;
    world.res_mut::<DebugDraw>().show_grid = false;

    // The demo owns its whole post chain, so the engine's is switched off at
    // the settings rather than at the graph: the engine syncs the conditional
    // passes from these after the game hook has run, and a pass disabled by
    // hand is turned straight back on.
    let settings = world.res_mut::<RenderSettings>();
    settings.taa_enabled = false;
    settings.bloom_enabled = false;
    settings.ssao_enabled = false;
    settings.depth_of_field.enabled = false;

    let camera = spawn_camera(world, Vec3::new(0.0, 3.0, -6.0), "Snow Camera".to_string());
    world.res_mut::<ActiveCamera>().0 = Some(camera);
    snow.camera_entity = Some(camera);

    world.plugin_resource_mut::<EguiState>().enabled = false;
}
