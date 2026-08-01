//! App-wide state the systems read and mutate.
//!
//! Everything here has a fixed population and is written into a data texture
//! each frame: a pool of five thousand snow grains, eight water strands or
//! ninety-six spine samples is one buffer apiece.

use crate::camera::CameraRig;
use crate::systems::character::Character;
use crate::systems::cloth::Cloth;
use crate::systems::contact::Contact;
use crate::systems::deform::Deformation;
use crate::systems::figure::Figure;
use crate::systems::shadows::Shadows;
use crate::systems::sky::Sky;
use crate::systems::snowfall::Snowfall;
use crate::systems::spell::lights::SpellLights;
use crate::systems::spell::water::WaterBody;
use crate::systems::spray::Spray;
use crate::systems::terrain::Heightfield;
use crate::systems::wake::Wake;
use nalgebra_glm::Vec3;
use nightshade::prelude::Entity;

/// The demo's simulation state.
#[derive(Default)]
pub struct SnowResources {
    pub rig: CameraRig,
    pub character: Character,
    pub figure: Figure,
    pub cloth: Cloth,
    pub contact: Contact,
    pub spray: Spray,
    pub snowfall: Snowfall,
    pub wake: Wake,
    pub water: WaterBody,
    pub lights: SpellLights,
    pub deform: Deformation,
    pub sky: Sky,
    pub shadows: Shadows,
    pub heightfield: Heightfield,

    /// What the clipmap rings and the deformation window are centred on: the
    /// character, not the camera.
    pub focus: Vec3,

    /// Seconds since the first frame, frozen when `Settings::freeze_time` is set.
    pub time: f32,

    /// How far into the bending stance the figure is, zero to one.
    pub cast_blend: f32,
    /// Seconds of stance left to hold after the last spell went off.
    pub cast_hold: f32,

    /// The engine camera.
    pub camera_entity: Option<Entity>,
}
