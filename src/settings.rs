/// Tone curve applied by the display transform.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tonemap {
    Agx,
    Aces,
    None,
}

pub fn tonemap_index(tonemap: Tonemap) -> f32 {
    match tonemap {
        Tonemap::Agx => 0.0,
        Tonemap::Aces => 1.0,
        Tonemap::None => 2.0,
    }
}

pub fn tonemap_label(tonemap: Tonemap) -> &'static str {
    match tonemap {
        Tonemap::Agx => "agx",
        Tonemap::Aces => "aces",
        Tonemap::None => "none",
    }
}

pub const TONEMAPS: [Tonemap; 3] = [Tonemap::Agx, Tonemap::Aces, Tonemap::None];

/// What the snow material outputs instead of the beauty pass.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DebugView {
    Beauty,
    Deform,
    Normals,
    Depth,
    Cascades,
    Footprint,
    FineNormals,
    Shadow,
    NdotL,
    ShadowMap,
    Albedo,
}

pub fn debug_view_index(view: DebugView) -> f32 {
    match view {
        DebugView::Beauty => 0.0,
        DebugView::Deform => 1.0,
        DebugView::Normals => 2.0,
        DebugView::Depth => 3.0,
        DebugView::Cascades => 4.0,
        DebugView::Footprint => 5.0,
        DebugView::FineNormals => 6.0,
        DebugView::Shadow => 7.0,
        DebugView::NdotL => 8.0,
        DebugView::ShadowMap => 9.0,
        DebugView::Albedo => 10.0,
    }
}

pub fn debug_view_label(view: DebugView) -> &'static str {
    match view {
        DebugView::Beauty => "beauty",
        DebugView::Deform => "deform",
        DebugView::Normals => "normals",
        DebugView::Depth => "depth",
        DebugView::Cascades => "cascades",
        DebugView::Footprint => "footprint",
        DebugView::FineNormals => "fineNormals",
        DebugView::Shadow => "shadow",
        DebugView::NdotL => "ndotl",
        DebugView::ShadowMap => "shadowMap",
        DebugView::Albedo => "albedo",
    }
}

pub const DEBUG_VIEWS: [DebugView; 11] = [
    DebugView::Beauty,
    DebugView::Deform,
    DebugView::Normals,
    DebugView::Depth,
    DebugView::Cascades,
    DebugView::Footprint,
    DebugView::FineNormals,
    DebugView::Shadow,
    DebugView::NdotL,
    DebugView::ShadowMap,
    DebugView::Albedo,
];

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    Ultra,
    High,
    Balanced,
}

pub fn preset_label(preset: Preset) -> &'static str {
    match preset {
        Preset::Ultra => "ultra",
        Preset::High => "high",
        Preset::Balanced => "balanced",
    }
}

pub const PRESETS: [Preset; 3] = [Preset::Ultra, Preset::High, Preset::Balanced];

/// Every art and quality parameter, read directly by the systems each frame.
#[derive(Clone, Copy)]
pub struct Settings {
    pub preset: Preset,
    pub resolution_scale: f32,

    pub sun_azimuth: f32,
    pub sun_elevation: f32,
    pub sun_intensity: f32,
    pub sun_temp_warm: f32,
    pub ambient_intensity: f32,
    pub ambient_blue: f32,

    pub fog_density: f32,
    pub fog_height_falloff: f32,
    pub fog_start: f32,
    pub aerial_strength: f32,
    pub wind_direction: f32,
    pub wind_strength: f32,
    pub show_mountains: bool,
    pub mountain_height: f32,
    pub shaft_strength: f32,

    pub glint_intensity: f32,
    pub glint_grazing: f32,
    pub sss_strength: f32,
    pub sss_radius: f32,
    pub detail_normal_strength: f32,
    pub macro_height_scale: f32,
    pub sastrugi_strength: f32,

    pub deform_depth: f32,
    pub deform_berm: f32,
    pub refill_rate: f32,
    pub deform_resolution: u32,

    pub wake_height: f32,
    pub wake_spray: f32,
    pub wind_streaks: bool,
    pub streak_strength: f32,

    pub show_spells: bool,
    pub spell_light: f32,
    pub spell_spray: f32,
    pub water_depth_tint: f32,

    pub taa: bool,
    pub ssr: bool,
    pub dof: bool,
    pub bloom: bool,
    pub grain: bool,
    pub sharpen: bool,
    pub tonemap: Tonemap,
    pub exposure: f32,
    pub contrast: f32,
    pub bloom_strength: f32,
    pub grain_strength: f32,
    pub sharpen_strength: f32,

    pub show_terrain: bool,
    pub show_character: bool,
    pub show_wake: bool,
    pub show_light_shafts: bool,
    pub wireframe: bool,
    pub freeze_time: bool,

    pub debug_view: DebugView,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            preset: Preset::Ultra,
            resolution_scale: 1.0,

            sun_azimuth: 118.0,
            sun_elevation: 13.0,
            sun_intensity: 4.2,
            sun_temp_warm: 1.0,
            ambient_intensity: 1.0,
            ambient_blue: 1.0,

            fog_density: 0.0072,
            fog_height_falloff: 0.045,
            fog_start: 24.0,
            aerial_strength: 1.0,
            wind_direction: 42.0,
            wind_strength: 1.0,
            show_mountains: true,
            mountain_height: 2150.0,
            shaft_strength: 0.30,

            glint_intensity: 0.55,
            glint_grazing: 0.72,
            sss_strength: 1.0,
            sss_radius: 1.0,
            detail_normal_strength: 1.0,
            macro_height_scale: 1.0,
            sastrugi_strength: 1.0,

            deform_depth: 1.0,
            deform_berm: 1.0,
            refill_rate: 1.0,
            deform_resolution: 2048,

            wake_height: 1.0,
            wake_spray: 1.0,
            wind_streaks: true,
            streak_strength: 1.0,

            show_spells: true,
            spell_light: 1.0,
            spell_spray: 1.0,
            water_depth_tint: 1.0,

            taa: false,
            ssr: true,
            dof: true,
            bloom: true,
            grain: true,
            sharpen: true,
            tonemap: Tonemap::Agx,
            exposure: 0.105,
            contrast: 1.14,
            bloom_strength: 0.22,
            grain_strength: 0.022,
            sharpen_strength: 0.55,

            show_terrain: true,
            show_character: true,
            show_wake: true,
            show_light_shafts: true,
            wireframe: false,
            freeze_time: false,

            debug_view: DebugView::Beauty,
        }
    }
}

/// Wind bearing in radians.
pub fn wind_angle(settings: &Settings) -> f32 {
    settings.wind_direction.to_radians()
}

/// Applies a quality preset.
pub fn apply_preset(settings: &mut Settings, preset: Preset) {
    settings.preset = preset;
    match preset {
        Preset::Ultra => {}
        Preset::High => {
            settings.deform_resolution = 2048;
            settings.resolution_scale = 1.0;
            settings.ssr = true;
            settings.dof = true;
        }
        Preset::Balanced => {
            settings.deform_resolution = 1024;
            settings.resolution_scale = 0.85;
            settings.ssr = false;
            settings.dof = false;
        }
    }
}
