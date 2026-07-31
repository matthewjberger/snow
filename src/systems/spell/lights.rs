/// Must match `SPELL_LIGHT_MAX` in `snow::spell_lights`.
pub const MAX_SPELL_LIGHTS: usize = 4;

/// The dynamic lights spells emit.
///
/// Nothing is retained between frames, so a spell that stops updating stops
/// lighting with no teardown. Every material that shades something the player can
/// see reads the same pool, which is the point: a spell has to light the snow,
/// the robe, the wake and the airborne spray out of one description, or it reads
/// as a glow pasted over a scene rather than as a light in it.
#[derive(Default)]
pub struct SpellLights {
    /// (x, y, z, radius) per slot.
    pub positions: [[f32; 4]; MAX_SPELL_LIGHTS],
    /// (r, g, b, intensity) per slot.
    pub colors: [[f32; 4]; MAX_SPELL_LIGHTS],
    pub count: usize,
    /// Multiplier the overlay drives, so the whole effect can be compared.
    pub scale: f32,
}

/// Drops last frame's declarations. Called once, before the spells update.
pub fn begin(lights: &mut SpellLights) {
    lights.count = 0;
}

/// Declares a light for this frame, dropped silently once the pool is full.
///
/// That is the right failure: the fifth light in a frame is by definition the
/// least important one on screen, and growing the array means a shader loop
/// the whole snow field pays for.
pub fn add(
    lights: &mut SpellLights,
    position: [f32; 3],
    radius: f32,
    color: [f32; 3],
    intensity: f32,
) {
    if lights.count >= MAX_SPELL_LIGHTS || intensity <= 0.0 || radius <= 0.0 {
        return;
    }
    let slot = lights.count;
    lights.count += 1;
    lights.positions[slot] = [position[0], position[1], position[2], radius];
    lights.colors[slot] = [color[0], color[1], color[2], intensity * lights.scale];
}
