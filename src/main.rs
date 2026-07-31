mod camera;
mod constants;
mod ecs;
mod input;
mod math;
mod plugin;
mod render;
mod rig;
mod settings;
mod shaders;
mod systems;

use nightshade::prelude::*;
use plugin::SnowPlugin;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugin(EguiPlugin)
        .add_plugin(GamepadPlugin)
        .add_plugin(SnowPlugin)
        .run()
}
