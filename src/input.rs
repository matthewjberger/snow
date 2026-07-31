use nightshade::prelude::*;

/// Radians of look per pixel of raw mouse motion.
const LOOK_SCALE: f32 = 0.0022;

/// Zoom per wheel notch.
const ZOOM_SCALE: f32 = 0.16;

const SPELL_KEYS: [KeyCode; 5] = [
    KeyCode::Digit1,
    KeyCode::Digit2,
    KeyCode::Digit3,
    KeyCode::Digit4,
    KeyCode::Digit5,
];

/// Face and shoulder buttons cast, in the same order as the number row.
const SPELL_BUTTONS: [gilrs::Button; 5] = [
    gilrs::Button::South,
    gilrs::Button::West,
    gilrs::Button::North,
    gilrs::Button::East,
    gilrs::Button::LeftTrigger,
];

/// The right shoulder jumps. The face buttons are spent on the spells.
const JUMP_BUTTON: gilrs::Button = gilrs::Button::RightTrigger;

/// Radians of look per second at full right-stick deflection.
const STICK_LOOK_RATE: f32 = 2.6;

/// Below this the stick is treated as centred.
const STICK_DEADZONE: f32 = 0.18;

/// How far the right trigger has to travel to count as held.
const TRIGGER_THRESHOLD: f32 = 0.4;

/// Applies a radial deadzone and rescales what is left to the full range, so speed
/// ramps from a standstill at the threshold.
fn deaden(x: f32, y: f32) -> (f32, f32) {
    let length = (x * x + y * y).sqrt();
    if length < STICK_DEADZONE {
        return (0.0, 0.0);
    }
    let scale = ((length - STICK_DEADZONE) / (1.0 - STICK_DEADZONE)).min(1.0) / length;
    (x * scale, y * scale)
}

/// Resolved input for the frame.
#[derive(Default, Clone, Copy)]
pub struct SnowInput {
    /// Movement axes, camera relative, clamped to a unit disc.
    pub move_x: f32,
    pub move_z: f32,
    pub moving: bool,

    /// Mouse look for this frame, in radians.
    pub look_x: f32,
    pub look_y: f32,

    /// Zoom, consumed by the camera rig.
    pub zoom_delta: f32,

    /// Right mouse held: snow-surf.
    pub surf: bool,
    pub sprint: bool,
    /// True for the one frame the jump was pressed.
    pub jump: bool,

    /// 0 for none, otherwise 1..=5.
    pub spell_pressed: u32,
    /// Spell 2 (Ribbon) is a held cast.
    pub spell_held_2: bool,

    /// True while the pointer is captured.
    pub locked: bool,

    lock_requested: bool,
}

/// Resolves held keys and accumulated mouse motion into the frame's axes, and owns
/// pointer capture.
pub fn poll_input_system(snow_input: &mut SnowInput, world: &mut World) {
    let overlay_toggled = {
        let keyboard = &world.res::<Input>().keyboard;
        keyboard.just_pressed(KeyCode::F1) || keyboard.just_pressed(KeyCode::Backquote)
    };

    if overlay_toggled {
        let overlay_open = !world.plugin_resource::<EguiState>().enabled;
        world.plugin_resource_mut::<EguiState>().enabled = overlay_open;
        snow_input.lock_requested = !overlay_open;
        set_cursor_locked(world, !overlay_open);
        world.res_mut::<Window>().show_cursor = overlay_open;
    }

    let overlay_open = world.plugin_resource::<EguiState>().enabled;

    if world.res::<Input>().keyboard.just_pressed(KeyCode::Escape) {
        if snow_input.lock_requested {
            snow_input.lock_requested = false;
            set_cursor_locked(world, false);
            world.res_mut::<Window>().show_cursor = true;
        } else {
            world.res_mut::<Window>().should_exit = true;
        }
    }

    let clicked = world
        .res::<Input>()
        .mouse
        .state
        .contains(MouseState::LEFT_JUST_PRESSED);
    if clicked && !snow_input.lock_requested && !overlay_open {
        snow_input.lock_requested = true;
        set_cursor_locked(world, true);
        world.res_mut::<Window>().show_cursor = false;
    }

    let focused = world.res::<Window>().is_focused;
    if snow_input.lock_requested && !focused {
        snow_input.lock_requested = false;
        set_cursor_locked(world, false);
        world.res_mut::<Window>().show_cursor = true;
    }

    snow_input.locked = snow_input.lock_requested && focused;

    let pad = read_gamepad(world, overlay_open);

    if !snow_input.locked && !pad.active {
        *snow_input = SnowInput {
            lock_requested: snow_input.lock_requested,
            ..Default::default()
        };
        return;
    }

    if !snow_input.locked {
        *snow_input = SnowInput {
            lock_requested: snow_input.lock_requested,
            locked: snow_input.locked,
            ..Default::default()
        };
        apply_gamepad(snow_input, &pad);
        return;
    }

    let input = world.res::<Input>();
    let keyboard = &input.keyboard;

    let mut axis_x = 0.0_f32;
    let mut axis_z = 0.0_f32;
    if keyboard.is_key_pressed(KeyCode::KeyW) || keyboard.is_key_pressed(KeyCode::ArrowUp) {
        axis_z += 1.0;
    }
    if keyboard.is_key_pressed(KeyCode::KeyS) || keyboard.is_key_pressed(KeyCode::ArrowDown) {
        axis_z -= 1.0;
    }
    if keyboard.is_key_pressed(KeyCode::KeyD) || keyboard.is_key_pressed(KeyCode::ArrowRight) {
        axis_x += 1.0;
    }
    if keyboard.is_key_pressed(KeyCode::KeyA) || keyboard.is_key_pressed(KeyCode::ArrowLeft) {
        axis_x -= 1.0;
    }
    let length = (axis_x * axis_x + axis_z * axis_z).sqrt();
    if length > 1.0 {
        axis_x /= length;
        axis_z /= length;
    }

    snow_input.move_x = axis_x;
    snow_input.move_z = axis_z;
    snow_input.moving = length > 0.001;
    snow_input.sprint =
        keyboard.is_key_pressed(KeyCode::ShiftLeft) || keyboard.is_key_pressed(KeyCode::ShiftRight);
    snow_input.jump = keyboard.just_pressed(KeyCode::Space);

    snow_input.spell_pressed = 0;
    for (index, key) in SPELL_KEYS.iter().enumerate() {
        if keyboard.just_pressed(*key) {
            snow_input.spell_pressed = index as u32 + 1;
        }
    }
    snow_input.spell_held_2 = keyboard.is_key_pressed(KeyCode::Digit2);

    snow_input.look_x = input.mouse.raw_mouse_delta.x * LOOK_SCALE;
    snow_input.look_y = input.mouse.raw_mouse_delta.y * LOOK_SCALE;
    snow_input.zoom_delta = -input.mouse.wheel_delta.y * ZOOM_SCALE;
    snow_input.surf = input.mouse.state.contains(MouseState::RIGHT_CLICKED);

    apply_gamepad(snow_input, &pad);
}

/// What the pad contributed this frame.
#[derive(Default)]
struct Pad {
    /// True when anything on the pad is off centre or pressed, which is what lets the
    /// sticks drive the character without a pointer capture.
    active: bool,
    move_x: f32,
    move_z: f32,
    look_x: f32,
    look_y: f32,
    zoom_delta: f32,
    surf: bool,
    sprint: bool,
    jump: bool,
    spell_pressed: u32,
    spell_held_2: bool,
}

fn read_gamepad(world: &mut World, overlay_open: bool) -> Pad {
    let mut pad = Pad::default();
    if overlay_open {
        return pad;
    }
    let delta_time = world.res::<Time>().delta_time.min(0.1);

    let pressed = world
        .res::<nightshade::platform::input::gamepad::Gamepad>()
        .just_pressed_buttons
        .clone();
    for (index, button) in SPELL_BUTTONS.iter().enumerate() {
        if pressed.contains(button) {
            pad.spell_pressed = index as u32 + 1;
            pad.active = true;
        }
    }
    if pressed.contains(&JUMP_BUTTON) {
        pad.jump = true;
        pad.active = true;
    }

    let read = with_active_gamepad(world, |gamepad| {
        let (move_x, move_z) = deaden(
            gamepad.value(gilrs::Axis::LeftStickX),
            gamepad.value(gilrs::Axis::LeftStickY),
        );
        let (look_x, look_y) = deaden(
            gamepad.value(gilrs::Axis::RightStickX),
            gamepad.value(gilrs::Axis::RightStickY),
        );
        let surf = gamepad.value(gilrs::Axis::RightZ) > TRIGGER_THRESHOLD
            || gamepad.is_pressed(gilrs::Button::RightTrigger2);
        let sprint = gamepad.is_pressed(gilrs::Button::LeftThumb)
            || gamepad.value(gilrs::Axis::LeftZ) > TRIGGER_THRESHOLD;
        let zoom = f32::from(gamepad.is_pressed(gilrs::Button::DPadDown))
            - f32::from(gamepad.is_pressed(gilrs::Button::DPadUp));
        let held_ribbon = gamepad.is_pressed(gilrs::Button::West);
        (
            move_x,
            move_z,
            look_x,
            look_y,
            surf,
            sprint,
            zoom,
            held_ribbon,
        )
    });

    let Some((move_x, move_z, look_x, look_y, surf, sprint, zoom, held_ribbon)) = read else {
        return pad;
    };

    pad.move_x = move_x;
    pad.move_z = move_z;
    pad.look_x = look_x * STICK_LOOK_RATE * delta_time;
    pad.look_y = -look_y * STICK_LOOK_RATE * delta_time;
    pad.zoom_delta = zoom * ZOOM_SCALE * 8.0 * delta_time;
    pad.surf = surf;
    pad.sprint = sprint;
    pad.spell_held_2 = held_ribbon;
    pad.active |= move_x != 0.0
        || move_z != 0.0
        || look_x != 0.0
        || look_y != 0.0
        || surf
        || sprint
        || zoom != 0.0;
    pad
}

fn apply_gamepad(snow_input: &mut SnowInput, pad: &Pad) {
    let mut move_x = snow_input.move_x + pad.move_x;
    let mut move_z = snow_input.move_z + pad.move_z;
    let length = (move_x * move_x + move_z * move_z).sqrt();
    if length > 1.0 {
        move_x /= length;
        move_z /= length;
    }
    snow_input.move_x = move_x;
    snow_input.move_z = move_z;
    snow_input.moving = length > 0.001;

    snow_input.look_x += pad.look_x;
    snow_input.look_y += pad.look_y;
    snow_input.zoom_delta += pad.zoom_delta;
    snow_input.surf |= pad.surf;
    snow_input.sprint |= pad.sprint;
    snow_input.jump |= pad.jump;
    snow_input.spell_held_2 |= pad.spell_held_2;
    if pad.spell_pressed != 0 {
        snow_input.spell_pressed = pad.spell_pressed;
    }
}
