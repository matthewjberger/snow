use crate::ecs::SnowResources;
use crate::settings;
use crate::settings::Settings;
use crate::systems::Perf;
use crate::systems::perf;
use crate::systems::spray::SPRAY_CAPACITY;
use nightshade::prelude::*;

/// The settings and performance overlay, hidden by default and toggled with either
/// function key one or the backtick.
pub fn overlay_system(world: &mut World) {
    if !world.plugin_resource::<EguiState>().enabled {
        return;
    }
    let Some(context) = egui_context(world) else {
        return;
    };

    egui::Window::new("snow_overlay")
        .title_bar(false)
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(0.0, 0.0))
        .default_width(336.0)
        .resizable(false)
        .show(&context, |ui| {
            ui.set_min_width(320.0);
            performance(ui, world);
            ui.add_space(8.0);
            groups(ui, world);
        });
}

fn performance(ui: &mut egui::Ui, world: &mut World) {
    let perf = world.res::<Perf>();
    let (median, low, spikes) = (
        perf.frames_per_second,
        perf.frames_per_second_low,
        perf.spike_count,
    );
    let (last, p95, p99, max) = (perf.last, perf.p95, perf.p99, perf.max);
    let history: Vec<f32> = perf::history(perf).collect();
    let live = world
        .ecs
        .resource::<SnowResources>()
        .map(|snow| snow.spray.live)
        .unwrap_or(0);

    ui.horizontal(|ui| {
        ui.strong("SNOW");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.weak("F1 to close");
        });
    });

    frame_graph(ui, &history);

    egui::Grid::new("snow_perf")
        .num_columns(4)
        .spacing([12.0, 2.0])
        .show(ui, |ui| {
            reading(ui, "fps", format!("{median:.0}"), median < 55.0);
            reading(ui, "1% low", format!("{low:.0}"), low < 40.0);
            ui.end_row();
            reading(ui, "frame", format!("{last:.2} ms"), last > 16.7);
            reading(ui, "p95", format!("{p95:.2} ms"), p95 > 16.7);
            ui.end_row();
            reading(ui, "p99", format!("{p99:.2} ms"), p99 > 20.0);
            reading(ui, "worst", format!("{max:.2} ms"), max > 33.0);
            ui.end_row();
            reading(ui, "spikes", format!("{spikes}"), spikes > 0);
            reading(
                ui,
                "grains",
                format!("{live} / {SPRAY_CAPACITY}"),
                live * 10 > SPRAY_CAPACITY * 9,
            );
            ui.end_row();
        });

    let systems = world.res::<Perf>().system_milliseconds.clone();
    let (draw_calls, triangles, gpu) = {
        let perf = world.res::<Perf>();
        (perf.draw_calls, perf.triangles, perf.gpu_milliseconds)
    };
    ui.add_space(4.0);
    egui::Grid::new("snow_systems")
        .num_columns(4)
        .spacing([12.0, 2.0])
        .show(ui, |ui| {
            for pair in systems.chunks(2) {
                for (name, milliseconds) in pair {
                    reading(ui, name, format!("{milliseconds:.2} ms"), false);
                }
                ui.end_row();
            }
            reading(ui, "draws", format!("{draw_calls}"), false);
            reading(ui, "tris", format!("{}k", triangles / 1000), false);
            ui.end_row();
            reading(ui, "gpu", format!("{gpu:.2} ms"), gpu > 16.7);
            ui.end_row();
        });

    ui.add_space(4.0);
    pose(ui, world);

    if ui.button("reset spikes").clicked() {
        perf::reset_spikes(world.res_mut::<Perf>());
    }
}

/// Where the camera and the character are, and a line of it to copy.
///
/// The numbers a shot is worth reproducing from: two angles, an arm length, a
/// position and a heading. Copying puts the same line on the clipboard, which is
/// the whole point of showing it.
fn pose(ui: &mut egui::Ui, world: &mut World) {
    let Some(snow) = world.ecs.resource::<SnowResources>() else {
        return;
    };
    let rig = &snow.rig;
    let character = &snow.character;

    let degrees = 180.0 / std::f32::consts::PI;
    let yaw = wrap_degrees(rig.yaw * degrees);
    let pitch = rig.pitch * degrees;
    let facing = wrap_degrees(character.facing * degrees);

    let camera_position = format!(
        "{:.2}  {:.2}  {:.2}",
        rig.position.x, rig.position.y, rig.position.z
    );
    let camera_angles = format!("{yaw:.1}\u{b0}  {pitch:+.1}\u{b0}");
    let camera_arm = format!("{:.2} m  {:.1}\u{b0} v", rig.distance, rig.fov * degrees);
    let character_position = format!(
        "{:.2}  {:.2}  {:.2}",
        character.position.x, character.position.y, character.position.z
    );
    let mut character_motion = format!("{:.2} m/s  {facing:.0}\u{b0}", character.speed);
    if character.surf > 0.01 {
        character_motion.push_str(&format!("  surf {:.2}", character.surf));
    }

    egui::Grid::new("snow_pose")
        .num_columns(2)
        .spacing([12.0, 2.0])
        .show(ui, |ui| {
            reading(ui, "cam pos", camera_position.clone(), false);
            ui.end_row();
            reading(ui, "cam ang", camera_angles, false);
            ui.end_row();
            reading(ui, "cam arm", camera_arm, false);
            ui.end_row();
            reading(ui, "chr pos", character_position.clone(), false);
            ui.end_row();
            reading(ui, "chr mot", character_motion, false);
            ui.end_row();
        });

    if ui.button("copy pose").clicked() {
        let line = format!(
            "position {character_position}  facing {facing:.1}  camera {camera_position}  \
             yaw {yaw:.1}  pitch {pitch:.1}  arm {:.2}",
            rig.distance
        );
        ui.ctx().copy_text(line);
    }
}

/// Wraps an angle in degrees to (-180, 180].
fn wrap_degrees(degrees: f32) -> f32 {
    let wrapped = (degrees + 180.0).rem_euclid(360.0) - 180.0;
    if wrapped <= -180.0 { 180.0 } else { wrapped }
}

fn reading(ui: &mut egui::Ui, label: &str, value: String, warn: bool) {
    ui.weak(label);
    if warn {
        ui.colored_label(egui::Color32::from_rgb(232, 176, 79), value);
    } else {
        ui.monospace(value);
    }
}

/// Frame times oldest to newest, on a fixed scale.
fn frame_graph(ui: &mut egui::Ui, history: &[f32]) {
    const CEILING: f32 = 33.4;
    let (response, painter) =
        ui.allocate_painter(egui::vec2(ui.available_width(), 66.0), egui::Sense::hover());
    let rect = response.rect;
    painter.rect_filled(rect, 3.0, egui::Color32::from_black_alpha(82));

    for (budget, shade) in [(16.7_f32, 90_u8), (8.3_f32, 45_u8)] {
        let y = rect.bottom() - rect.height() * (budget / CEILING);
        painter.line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            egui::Stroke::new(1.0, egui::Color32::from_white_alpha(shade)),
        );
    }

    if history.len() < 2 {
        return;
    }
    let step = rect.width() / (history.len() - 1) as f32;
    let points: Vec<egui::Pos2> = history
        .iter()
        .enumerate()
        .map(|(index, milliseconds)| {
            let height = (milliseconds / CEILING).clamp(0.0, 1.0);
            egui::pos2(
                rect.left() + index as f32 * step,
                rect.bottom() - rect.height() * height,
            )
        })
        .collect();
    painter.add(egui::Shape::line(
        points,
        egui::Stroke::new(1.0, egui::Color32::from_rgb(143, 196, 232)),
    ));
}

fn groups(ui: &mut egui::Ui, world: &mut World) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.weak("preset");
            for preset in settings::PRESETS {
                let active = world.res::<Settings>().preset == preset;
                if ui
                    .selectable_label(active, settings::preset_label(preset))
                    .clicked()
                {
                    settings::apply_preset(world.res_mut::<Settings>(), preset);
                }
            }
        });

        ui.add_space(4.0);
        ui.weak("wasd move   shift run   space jump");
        ui.weak("right mouse surf   1-5 spells, hold 2");

        let settings = world.res_mut::<Settings>();

        heading(ui, "Sun & Sky");
        slider(ui, "Azimuth", &mut settings.sun_azimuth, 0.0..=360.0);
        slider(ui, "Elevation", &mut settings.sun_elevation, 0.5..=45.0);
        slider(ui, "Intensity", &mut settings.sun_intensity, 0.0..=10.0);
        slider(ui, "Warmth", &mut settings.sun_temp_warm, 0.0..=1.0);
        slider(ui, "Ambient", &mut settings.ambient_intensity, 0.0..=3.0);
        slider(ui, "Ambient blue", &mut settings.ambient_blue, 0.0..=2.0);

        heading(ui, "Atmosphere");
        slider(ui, "Fog density", &mut settings.fog_density, 0.0..=0.03);
        slider(
            ui,
            "Height falloff",
            &mut settings.fog_height_falloff,
            0.0..=0.3,
        );
        slider(
            ui,
            "Aerial persp.",
            &mut settings.aerial_strength,
            0.0..=2.0,
        );
        slider(ui, "Wind dir", &mut settings.wind_direction, 0.0..=360.0);
        slider(ui, "Wind strength", &mut settings.wind_strength, 0.0..=2.0);
        toggle(ui, "Far range", &mut settings.show_mountains);
        slider(
            ui,
            "Range height",
            &mut settings.mountain_height,
            0.0..=2500.0,
        );
        toggle(ui, "Light shafts", &mut settings.show_light_shafts);
        slider(ui, "Shaft amt", &mut settings.shaft_strength, 0.0..=2.0);

        heading(ui, "Snow");
        slider(ui, "Glint", &mut settings.glint_intensity, 0.0..=2.0);
        slider(ui, "Glint gate", &mut settings.glint_grazing, 0.0..=1.0);
        slider(ui, "SSS strength", &mut settings.sss_strength, 0.0..=3.0);
        slider(ui, "SSS radius", &mut settings.sss_radius, 0.1..=3.0);
        slider(
            ui,
            "Detail normals",
            &mut settings.detail_normal_strength,
            0.0..=2.0,
        );
        slider(
            ui,
            "Dune height",
            &mut settings.macro_height_scale,
            0.0..=2.0,
        );
        slider(ui, "Sastrugi", &mut settings.sastrugi_strength, 0.0..=2.0);

        heading(ui, "Deformation");
        slider(ui, "Depth", &mut settings.deform_depth, 0.0..=3.0);
        slider(ui, "Berm mass", &mut settings.deform_berm, 0.0..=3.0);
        slider(ui, "Refill rate", &mut settings.refill_rate, 0.0..=4.0);

        heading(ui, "Weather");
        slider(ui, "Snowfall", &mut settings.snowfall, 0.0..=1.0);
        slider(ui, "Snowfall wind", &mut settings.snowfall_wind, 0.0..=3.0);

        heading(ui, "Snow-surf");
        slider(ui, "Wake height", &mut settings.wake_height, 0.0..=2.0);
        slider(ui, "Plume density", &mut settings.wake_spray, 0.0..=2.5);
        toggle(ui, "Speed streaks", &mut settings.wind_streaks);
        slider(ui, "Streak amt", &mut settings.streak_strength, 0.0..=2.0);
        toggle(ui, "Wake mesh", &mut settings.show_wake);

        heading(ui, "Spells");
        toggle(ui, "Spells", &mut settings.show_spells);
        slider(ui, "Spell light", &mut settings.spell_light, 0.0..=3.0);
        slider(ui, "Spell spray", &mut settings.spell_spray, 0.0..=2.5);
        slider(ui, "Water depth", &mut settings.water_depth_tint, 0.0..=3.0);

        heading(ui, "Post");
        toggle(ui, "TAA", &mut settings.taa);
        toggle(ui, "SSR (ice)", &mut settings.ssr);
        toggle(ui, "Depth of field", &mut settings.dof);
        toggle(ui, "Bloom", &mut settings.bloom);
        toggle(ui, "Film grain", &mut settings.grain);
        toggle(ui, "Sharpen", &mut settings.sharpen);
        choice(
            ui,
            "Tonemap",
            &mut settings.tonemap,
            &settings::TONEMAPS,
            settings::tonemap_label,
        );
        slider(ui, "Exposure", &mut settings.exposure, 0.01..=0.6);
        slider(ui, "Contrast", &mut settings.contrast, 0.5..=2.0);
        slider(ui, "Bloom amt", &mut settings.bloom_strength, 0.0..=1.0);
        slider(ui, "Grain amt", &mut settings.grain_strength, 0.0..=0.1);
        slider(ui, "Sharpen amt", &mut settings.sharpen_strength, 0.0..=1.0);

        heading(ui, "Systems");
        toggle(ui, "Terrain", &mut settings.show_terrain);
        toggle(ui, "Character", &mut settings.show_character);
        toggle(ui, "Wireframe", &mut settings.wireframe);
        toggle(ui, "Freeze time", &mut settings.freeze_time);
        slider(ui, "Resolution", &mut settings.resolution_scale, 0.5..=1.5);
        choice(
            ui,
            "Debug view",
            &mut settings.debug_view,
            &settings::DEBUG_VIEWS,
            settings::debug_view_label,
        );
    });
}

fn heading(ui: &mut egui::Ui, title: &str) {
    ui.add_space(10.0);
    ui.weak(title.to_uppercase());
    ui.separator();
}

fn slider(ui: &mut egui::Ui, label: &str, value: &mut f32, range: std::ops::RangeInclusive<f32>) {
    ui.horizontal(|ui| {
        ui.add_sized([108.0, 16.0], egui::Label::new(label));
        ui.add(egui::Slider::new(value, range).show_value(true));
    });
}

fn toggle(ui: &mut egui::Ui, label: &str, value: &mut bool) {
    ui.horizontal(|ui| {
        ui.add_sized([108.0, 16.0], egui::Label::new(label));
        ui.checkbox(value, "");
    });
}

fn choice<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut T,
    options: &[T],
    name: impl Fn(T) -> &'static str,
) {
    ui.horizontal(|ui| {
        ui.add_sized([108.0, 16.0], egui::Label::new(label));
        egui::ComboBox::from_id_salt(label)
            .selected_text(name(*value))
            .show_ui(ui, |ui| {
                for option in options {
                    ui.selectable_value(value, *option, name(*option));
                }
            });
    });
}
