mod components;
mod resources;

pub use components::*;
pub use resources::*;

use nightshade::prelude::nightshade_ecs;

nightshade_ecs::dynamic_schema! {
    pub fn register_snow_components {
        crystal: Crystal => CRYSTAL,
        sweep: Sweep => SWEEP,
        ribbon: Ribbon => RIBBON,
        bloom: Bloom => BLOOM,
        crystallize: Crystallize => CRYSTALLIZE,
        vortex: Vortex => VORTEX,
    }
}

nightshade_ecs::impl_component!(Crystal, Sweep, Ribbon, Bloom, Crystallize, Vortex);
