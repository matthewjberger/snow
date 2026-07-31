use crate::camera;
use crate::camera::Z_FAR;
use crate::constants::{
    BASE_SPACING, CASCADE_COUNT, CHARACTER_TEX_HEIGHT, CHARACTER_TEX_WIDTH, DEFORM_COVERAGE,
    HEIGHT_RES, SKY_SH_HEIGHT, SKY_SH_WIDTH, WORLD_SIZE,
};
use crate::ecs::SnowResources;
use crate::render::bake;
use crate::render::bake::{Bakes, SkySample};
use crate::render::character as character_pass;
use crate::render::character::CharUniforms;
use crate::render::crystal as crystal_pass;
use crate::render::deform as deform_pass;
use crate::render::deform::DeformUniforms;
use crate::render::geometry::GRID_HALF_N;
use crate::render::gpu as gpu_state;
use crate::render::gpu::SnowGpu;
use crate::render::pipelines::write_uniform;
use crate::render::post as post_pass;
use crate::render::post::{PostUniforms, SnowPostPass};
use crate::render::readback::{Readback, begin_read, poll_read};
use crate::render::sky as sky_pass;
use crate::render::sky::SkyUniforms;
use crate::render::spray as spray_pass;
use crate::render::uniforms::{SnowUniforms, matrix_columns};
use crate::render::wake as wake_pass;
use crate::render::water as water_pass;
use crate::render::world as world_pass;
use crate::render::world::SnowWorldPass;
use crate::settings;
use crate::settings::Settings;
use crate::shaders;
use crate::systems::Perf;
use crate::systems::cloth;
use crate::systems::deform;
use crate::systems::shadows;
use crate::systems::sky;
use crate::systems::spell::water;
use crate::systems::spray;
use crate::systems::terrain;
use crate::systems::wake;
use nightshade::prelude::*;

/// Everything the demo owns on the GPU, plus the pipelines that write it.
pub struct SnowRender {
    pub gpu: SnowGpu,
    pub bakes: Bakes,
    pub post: PostState,
    pub boot: Boot,
    /// Latched once the heightfield has been read back. The sky goes back
    /// through `boot` whenever the sun moves; the terrain is baked once and this
    /// stays true from then on.
    pub terrain_ready: bool,
}

/// How far through the load-time bakes the demo is.
///
/// Each step needs the previous one read back to the CPU, and a readback lands
/// only once the runtime gets a turn, so they run a step per frame. The sky is
/// solved four times because the snow bounces light back into it and the bake
/// has to see its own answer.
pub enum Boot {
    /// The height and detail bakes are in flight.
    Terrain(Readback),
    /// Solving the sky, on the given iteration.
    Sky {
        iteration: u32,
        pixels: Readback,
    },
    Ready,
}

/// Whether the load-time bakes have finished.
///
/// The heightfield is empty until they have, and the simulation waits for it:
/// every height query the character makes comes out of it.
pub fn booted(world: &World) -> bool {
    world
        .ecs
        .resource::<SnowRender>()
        .is_some_and(|render| render.terrain_ready)
}

/// What the screen-space chain has to remember between frames.
pub struct PostState {
    /// Last frame's unjittered view-projection, which the resolve reprojects through.
    previous_view_projection: [f32; 16],
    /// Zero on the first frame, when the history is uninitialised memory and a single
    /// value out of it would propagate for the rest of the session.
    history_valid: f32,
    frame: u32,
    /// Eased focal distance, in metres, tracking the spring arm.
    focus_distance: f32,
    /// The internal resolution everything upstream of the final blit is
    /// currently allocated at, which is the surface times the resolution
    /// slider. Kept so the reconcile only runs on a real change.
    internal_size: (u32, u32),
}

impl Default for PostState {
    fn default() -> Self {
        let mut identity = [0.0_f32; 16];
        identity[0] = 1.0;
        identity[5] = 1.0;
        identity[10] = 1.0;
        identity[15] = 1.0;
        Self {
            previous_view_projection: identity,
            history_valid: 0.0,
            frame: 0,
            focus_distance: 6.2,
            internal_size: (0, 0),
        }
    }
}

/// Creates the GPU side on the first frame, runs the load-time bakes, and then keeps
/// the sky in step with the sun.
pub fn pre_render(renderer: &mut WgpuRenderer, world: &mut World) {
    if world.ecs.resource::<SnowRender>().is_none() {
        let mut library = shaders::new();
        let deform_resolution = world.res::<Settings>().deform_resolution.max(512);
        let scale = world.res::<Settings>().resolution_scale.clamp(0.25, 2.0);
        let width = ((renderer.surface_config.width.max(1) as f32 * scale).round() as u32).max(1);
        let height = ((renderer.surface_config.height.max(1) as f32 * scale).round() as u32).max(1);
        let gpu = gpu_state::new(&renderer.device, deform_resolution, width, height);
        let bakes = bake::new(&renderer.device, &mut library, &gpu);
        world.ecs.insert_resource(SnowRender {
            gpu,
            bakes,
            post: PostState::default(),
            boot: Boot::Ready,
            terrain_ready: false,
        });
        run_static_bakes(renderer, world);
    }

    sync_internal_resolution(renderer, world);
    let idle = advance_boot(renderer, world);
    if !booted(world) {
        return;
    }
    // Held off while a solve is in flight, so dragging the dial lets each one
    // finish.
    if idle {
        solve_sky(renderer, world);
    }
    upload_character(renderer, world);
    sync_world_pass(renderer, world);
}

/// Runs one step of the load-time bakes, reporting whether the queue is clear.
fn advance_boot(renderer: &mut WgpuRenderer, world: &mut World) -> bool {
    let Some(render) = world.ecs.resource::<SnowRender>() else {
        return false;
    };
    match &render.boot {
        Boot::Ready => return true,
        Boot::Terrain(readback) => {
            let Some(bytes) = poll_read(readback, &renderer.device) else {
                return false;
            };
            let pairs = bytemuck::cast_slice::<u8, f32>(&bytes).to_vec();
            let snow = world
                .ecs
                .resource_mut::<SnowResources>()
                .expect("snow state");
            terrain::absorb_readback(&mut snow.heightfield, &pairs);
            shadows::set_height_bounds(
                &mut snow.shadows,
                snow.heightfield.min_height - 4.0,
                snow.heightfield.max_height + 6.0,
            );
            world
                .ecs
                .resource_mut::<SnowRender>()
                .expect("snow render")
                .terrain_ready = true;
            begin_sky_iteration(renderer, world, 0);
        }
        Boot::Sky { iteration, pixels } => {
            let Some(bytes) = poll_read(pixels, &renderer.device) else {
                return false;
            };
            let iteration = *iteration;
            let pixels = bytemuck::cast_slice::<u8, f32>(&bytes).to_vec();
            let snow = world
                .ecs
                .resource_mut::<SnowResources>()
                .expect("snow state");
            sky::project_harmonics(&mut snow.sky, &pixels);
            if iteration >= 3 {
                world
                    .ecs
                    .resource_mut::<SnowRender>()
                    .expect("snow render")
                    .boot = Boot::Ready;
                return true;
            }
            sky::update_ground_bounce(&mut snow.sky);
            begin_sky_iteration(renderer, world, iteration + 1);
        }
    }
    false
}

/// Bakes the sky once more and asks for the result back.
fn begin_sky_iteration(renderer: &mut WgpuRenderer, world: &mut World, iteration: u32) {
    bake_sky_once(renderer, world);
    let render = world.ecs.resource::<SnowRender>().expect("snow render");
    let pixels = begin_read(
        &renderer.device,
        &renderer.queue,
        &render.gpu.sky_sh.texture,
        SKY_SH_WIDTH,
        SKY_SH_HEIGHT,
        16,
    );
    world
        .ecs
        .resource_mut::<SnowRender>()
        .expect("snow render")
        .boot = Boot::Sky { iteration, pixels };
}

/// Keeps the internal render resolution in step with the window and the slider.
///
/// The scene is drawn at the surface times the slider, and the final blit stays
/// full resolution: the control buys frame time on the expensive passes while
/// the tonemap and the sharpen keep their edge. The chain's targets are rebuilt
/// to match, and the history is dropped because the frames in it are the wrong
/// size to reproject.
fn sync_internal_resolution(renderer: &mut WgpuRenderer, world: &mut World) {
    let scale = world.res::<Settings>().resolution_scale.clamp(0.25, 2.0);
    let surface = (
        renderer.surface_config.width.max(1),
        renderer.surface_config.height.max(1),
    );
    let wanted = (
        ((surface.0 as f32 * scale).round() as u32).max(1),
        ((surface.1 as f32 * scale).round() as u32).max(1),
    );

    let Some(render) = world.ecs.resource_mut::<SnowRender>() else {
        return;
    };
    if render.post.internal_size == wanted {
        return;
    }
    render.post.internal_size = wanted;
    // The frames already in the history are the wrong size to reproject.
    render.post.history_valid = 0.0;
    gpu_state::resize_scene(&mut render.gpu, &renderer.device, wanted.0, wanted.1);

    let render = world.ecs.resource::<SnowRender>().expect("snow render");
    if let Some(pass) = render_graph_get_pass_mut(&mut renderer.graph, "snow_world")
        && let Some(pass) = (pass as &mut dyn std::any::Any).downcast_mut::<SnowWorldPass>()
    {
        world_pass::bind(pass, &renderer.device, &render.gpu);
    }
    if let Some(pass) = render_graph_get_pass_mut(&mut renderer.graph, "snow_post")
        && let Some(pass) = (pass as &mut dyn std::any::Any).downcast_mut::<SnowPostPass>()
    {
        post_pass::resize(pass, &renderer.device, wanted.0, wanted.1);
        post_pass::bind(pass, &render.gpu);
    }
}

/// Pushes the posed skeleton to the GPU.
fn upload_character(renderer: &mut WgpuRenderer, world: &mut World) {
    let Some(render) = world.ecs.resource::<SnowRender>() else {
        return;
    };
    let snow = world.ecs.resource::<SnowResources>().expect("snow state");

    let width = CHARACTER_TEX_WIDTH as usize;
    let mut data = vec![0.0_f32; width * CHARACTER_TEX_HEIGHT as usize * 4];
    for (bone, matrix) in snow.figure.skin.iter().enumerate() {
        for column in 0..4 {
            let offset = (column * width + bone) * 4;
            data[offset..offset + 4].copy_from_slice(&matrix[column * 4..column * 4 + 4]);
        }
    }
    cloth::write_nodes(&snow.cloth, &mut data, width);

    renderer.queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &render.gpu.character.texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        bytemuck::cast_slice(&data),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(CHARACTER_TEX_WIDTH * 16),
            rows_per_image: Some(CHARACTER_TEX_HEIGHT),
        },
        wgpu::Extent3d {
            width: CHARACTER_TEX_WIDTH,
            height: CHARACTER_TEX_HEIGHT,
            depth_or_array_layers: 1,
        },
    );
}

/// Builds this frame's uniform block and hands it to every terrain program.
fn sync_world_pass(renderer: &mut WgpuRenderer, world: &mut World) {
    let (width, height) = (
        renderer.surface_config.width.max(1) as f32,
        renderer.surface_config.height.max(1) as f32,
    );

    let (jitter, previous_view_projection, history_valid, focus_distance) = {
        let Some(render) = world.ecs.resource::<SnowRender>() else {
            return;
        };
        let settings = world.res::<Settings>();
        let jitter = if settings.taa {
            let step = crate::math::halton_sequence::<8>()[render.post.frame as usize % 8];
            [2.0 * step.0 / width, 2.0 * step.1 / height]
        } else {
            [0.0, 0.0]
        };
        (
            jitter,
            render.post.previous_view_projection,
            render.post.history_valid,
            render.post.focus_distance,
        )
    };

    let sky_uniforms;
    let mut post_uniforms = PostUniforms::default();
    let mut uniforms = SnowUniforms::default();
    let unjittered;
    {
        let settings = world.res::<Settings>();
        let snow = world.ecs.resource::<SnowResources>().expect("snow state");

        let view = camera::view_matrix(&snow.rig);
        let projection = camera::projection_matrix(&snow.rig, width / height);
        unjittered = matrix_columns(&(projection * view));

        let mut jittered = projection;
        jittered[(0, 2)] += jitter[0];
        jittered[(1, 2)] += jitter[1];
        let view_projection = jittered * view;

        let half_angle = (snow.rig.fov * 0.5).tan();
        post_uniforms.previous_view_projection = previous_view_projection;
        post_uniforms.inverse_view = matrix_columns(
            &view
                .try_inverse()
                .unwrap_or_else(nalgebra_glm::Mat4::identity),
        );
        post_uniforms.projection = [
            half_angle * (width / height),
            half_angle,
            1.0 / width,
            1.0 / height,
        ];
        post_uniforms.temporal = [jitter[0], jitter[1], history_valid, 0.90];

        uniforms.view_projection = matrix_columns(&view_projection);
        uniforms.wake = [
            snow.wake.count as f32,
            crate::systems::wake::WAKE_COLS as f32,
            crate::systems::wake::WAKE_ROWS as f32,
            snow.wake.clock,
        ];
        uniforms.water = [
            crate::systems::spell::water::LATTICE_COLS as f32,
            crate::systems::spell::water::RING as f32,
            snow.water.clock,
            settings.water_depth_tint,
        ];
        uniforms.strands = water::params(&snow.water);
        uniforms.lights.positions = snow.lights.positions;
        uniforms.lights.colors = snow.lights.colors;
        uniforms.lights.count = [snow.lights.count as f32, 0.0, 0.0, 0.0];
        uniforms.billboard = [
            [view[(0, 0)], view[(0, 1)], view[(0, 2)], 0.0],
            [view[(1, 0)], view[(1, 1)], view[(1, 2)], 0.0],
        ];
        uniforms.camera = [
            snow.rig.position.x,
            snow.rig.position.y,
            snow.rig.position.z,
            0.0,
        ];
        uniforms.clipmap = [snow.focus.x, snow.focus.z, BASE_SPACING, GRID_HALF_N];
        uniforms.field = [
            -WORLD_SIZE * 0.5,
            -WORLD_SIZE * 0.5,
            WORLD_SIZE,
            HEIGHT_RES as f32,
        ];
        uniforms.surface = [
            settings::wind_angle(settings),
            settings.macro_height_scale,
            settings.sastrugi_strength,
            settings.detail_normal_strength,
        ];
        uniforms.snow = [
            settings.glint_intensity,
            settings.glint_grazing,
            settings.sss_strength,
            settings.sss_radius,
        ];
        uniforms.fog = [
            settings.fog_density,
            settings.fog_height_falloff,
            settings.fog_start,
            settings.aerial_strength,
        ];
        let deform_texel = DEFORM_COVERAGE / settings.deform_resolution.max(512) as f32;
        uniforms.deform = [snow.focus.x, snow.focus.z, DEFORM_COVERAGE, deform_texel];
        uniforms.misc = [
            settings.deform_depth,
            settings.ambient_intensity,
            settings::debug_view_index(settings.debug_view),
            if settings.wireframe { 1.0 } else { 0.0 },
        ];
        uniforms.screen = [width, height, 0.0, 0.0];
        uniforms.sun_direction = [
            snow.sky.sun_direction.x,
            snow.sky.sun_direction.y,
            snow.sky.sun_direction.z,
            0.0,
        ];
        uniforms.sun_radiance = [
            snow.sky.sun_radiance.x,
            snow.sky.sun_radiance.y,
            snow.sky.sun_radiance.z,
            0.0,
        ];
        uniforms.harmonics = sky::ambient_harmonics(&snow.sky, settings.ambient_blue);

        for cascade in 0..CASCADE_COUNT {
            uniforms.shadow.matrices[cascade] = matrix_columns(&snow.shadows.matrices[cascade]);
            uniforms.shadow.cascade[cascade] = snow.shadows.params[cascade];
        }
        uniforms.shadow.splits = snow.shadows.splits;
        uniforms.shadow.filter = [snow.shadows.texel_size, 1.8, 0.022, 0.0];
        uniforms.shadow.sun_direction = uniforms.sun_direction;

        let wind = settings::wind_angle(settings);
        sky_uniforms = SkyUniforms {
            view_projection: uniforms.view_projection,
            camera: [
                snow.rig.position.x,
                snow.rig.position.y,
                snow.rig.position.z,
                Z_FAR * 0.5,
            ],
            sun: [
                snow.sky.sun_direction.x,
                snow.sky.sun_direction.y,
                snow.sky.sun_direction.z,
                snow.sky.sun_scale,
            ],
            sun_color: [
                snow.sky.sun_color.x,
                snow.sky.sun_color.y,
                snow.sky.sun_color.z,
                settings.ambient_intensity,
            ],
            sun_radiance: [
                snow.sky.sun_radiance.x,
                snow.sky.sun_radiance.y,
                snow.sky.sun_radiance.z,
                if settings.show_mountains {
                    settings.mountain_height
                } else {
                    0.0
                },
            ],
            weather: [snow.time, 0.55, wind.sin(), wind.cos()],
            fog: uniforms.fog,
            harmonics: sky::ambient_harmonics(&snow.sky, settings.ambient_blue),
        };
    }

    let mut character_uniforms = CharUniforms::default();
    let cascade_matrices;
    {
        let settings = world.res::<Settings>();
        let snow = world.ecs.resource::<SnowResources>().expect("snow state");
        character_uniforms.view_projection = uniforms.view_projection;
        character_uniforms.camera = uniforms.camera;
        character_uniforms.sun_direction = uniforms.sun_direction;
        character_uniforms.sun_radiance = uniforms.sun_radiance;
        character_uniforms.fog = uniforms.fog;
        character_uniforms.misc = [
            settings.ambient_intensity,
            settings.sss_strength,
            210.0,
            0.0,
        ];
        let wind = settings::wind_angle(settings);
        let strength = 0.6 * settings.wind_strength;
        character_uniforms.fur = [
            wind.sin() * strength * 0.006
                - snow.character.velocity.x * 0.0016
                - snow.character.acceleration.x * 0.00018,
            -0.018,
            wind.cos() * strength * 0.006
                - snow.character.velocity.z * 0.0016
                - snow.character.acceleration.z * 0.00018,
            250.0,
        ];
        character_uniforms.panels = cloth::panel_params(&snow.cloth);
        character_uniforms.screen = uniforms.screen;
        character_uniforms.harmonics = uniforms.harmonics;
        character_uniforms.shadow = uniforms.shadow;
        character_uniforms.shadow.filter[1] = 1.4;
        character_uniforms.shadow.filter[2] = 0.012;
        cascade_matrices = uniforms.shadow.matrices;
    }

    {
        let settings = world.res::<Settings>();
        let snow = world.ecs.resource::<SnowResources>().expect("snow state");

        let far_sun = snow.rig.position + snow.sky.sun_direction * 2000.0;
        let clip = project_point(&unjittered, far_sun);
        let in_front = snow.sky.sun_direction.dot(&snow.rig.forward) > 0.05;
        post_uniforms.sun = [
            clip[0] * 0.5 + 0.5,
            0.5 - clip[1] * 0.5,
            if in_front { 1.0 } else { 0.0 },
            width / height,
        ];
        post_uniforms.sun_color = [
            snow.sky.sun_radiance.x,
            snow.sky.sun_radiance.y,
            snow.sky.sun_radiance.z,
            settings.shaft_strength,
        ];
        post_uniforms.tone = [
            settings.exposure,
            settings.contrast,
            settings::tonemap_index(settings.tonemap),
            if settings.grain {
                settings.grain_strength
            } else {
                0.0
            },
        ];
        post_uniforms.look = [
            snow.time,
            0.22,
            if settings.wind_streaks {
                snow.character.streak01 * settings.streak_strength
            } else {
                0.0
            },
            if settings.bloom {
                settings.bloom_strength
            } else {
                0.0
            },
        ];
        post_uniforms.focus = [
            focus_distance,
            height * 0.0024,
            if settings.dof { 1.0 } else { 0.0 },
            if settings.show_light_shafts { 1.0 } else { 0.0 },
        ];
        post_uniforms.toggles = [
            if settings.ssr { 1.0 } else { 0.0 },
            if settings.taa { 1.0 } else { 0.0 },
            if settings.sharpen {
                settings.sharpen_strength
            } else {
                0.0
            },
            1.0,
        ];
    }

    let deform_source = world
        .ecs
        .resource::<SnowRender>()
        .map(|render| render.gpu.deform_read)
        .unwrap_or(0);
    let deform_uniforms = {
        let settings = world.res::<Settings>();
        let snow = world.ecs.resource::<SnowResources>().expect("snow state");
        DeformUniforms {
            centres: [
                snow.deform.centre.x,
                snow.deform.centre.y,
                snow.deform.previous_centre.x,
                snow.deform.previous_centre.y,
            ],
            window: [
                DEFORM_COVERAGE,
                snow.deform.resolution as f32,
                snow.deform.relax_step,
                deform::brush_count(&snow.deform) as f32,
            ],
            relax: [
                settings.refill_rate,
                0.55 * settings.deform_depth,
                0.34 * settings.deform_berm,
                settings::wind_angle(settings),
            ],
        }
    };

    let needs_bind = {
        let Some(pass) = render_graph_get_pass_mut(&mut renderer.graph, "snow_world") else {
            return;
        };
        let Some(pass) = (pass as &mut dyn std::any::Any).downcast_mut::<SnowWorldPass>() else {
            return;
        };
        !world_pass::is_bound(pass)
    };

    if needs_bind {
        let device = renderer.device.clone();
        let gpu_ready = world.ecs.resource::<SnowRender>().is_some();
        if gpu_ready {
            let render = world.ecs.resource::<SnowRender>().expect("snow render");
            if let Some(pass) = render_graph_get_pass_mut(&mut renderer.graph, "snow_world")
                && let Some(pass) = (pass as &mut dyn std::any::Any).downcast_mut::<SnowWorldPass>()
            {
                world_pass::bind(pass, &device, &render.gpu);
            }
            if let Some(pass) = render_graph_get_pass_mut(&mut renderer.graph, "snow_post")
                && let Some(pass) = (pass as &mut dyn std::any::Any).downcast_mut::<SnowPostPass>()
            {
                post_pass::bind(pass, &render.gpu);
            }
        }
    }

    if let Some(pass) = render_graph_get_pass_mut(&mut renderer.graph, "snow_world")
        && let Some(pass) = (pass as &mut dyn std::any::Any).downcast_mut::<SnowWorldPass>()
    {
        let mut cascade_uniforms = uniforms;
        for cascade in 0..CASCADE_COUNT {
            cascade_uniforms.view_projection = uniforms.shadow.matrices[cascade];
            write_uniform(
                &pass.terrain.cascade_uniforms[cascade],
                &renderer.queue,
                &cascade_uniforms,
            );
        }
        write_uniform(&pass.terrain.prepass_uniforms, &renderer.queue, &uniforms);
        write_uniform(&pass.terrain.beauty_uniforms, &renderer.queue, &uniforms);
        sky_pass::write(&pass.sky, &renderer.queue, &sky_uniforms);
        character_pass::write(
            &pass.character,
            &renderer.queue,
            &character_uniforms,
            &cascade_matrices,
        );
        if let Some(render) = world.ecs.resource::<SnowRender>() {
            let snow = world.ecs.resource::<SnowResources>().expect("snow state");
            spray_pass::write(
                &mut pass.spray,
                &renderer.queue,
                &render.gpu,
                &uniforms,
                spray::texels(&snow.spray),
                snow.spray.live as u32,
            );
            let spells_on = world.res::<Settings>().show_spells;
            pass.crystal.visible = spells_on
                && crystal_pass::write(
                    &mut pass.crystal,
                    &renderer.queue,
                    &render.gpu,
                    &uniforms,
                    &cascade_matrices,
                    world,
                );
            pass.water.visible = water::visible(&snow.water) && spells_on;
            if pass.water.visible {
                water_pass::write(
                    &mut pass.water,
                    &renderer.queue,
                    &render.gpu,
                    &uniforms,
                    water::texels(&snow.water),
                    water::live_strands(&snow.water) as u32,
                );
            }
            pass.terrain.visible = world.res::<Settings>().show_terrain;
            pass.character.visible = world.res::<Settings>().show_character;
            pass.wake.visible = wake::visible(&snow.wake) && world.res::<Settings>().show_wake;
            if pass.wake.visible {
                wake_pass::write(
                    &pass.wake,
                    &renderer.queue,
                    &render.gpu,
                    &uniforms,
                    &cascade_matrices,
                    wake::texels(&snow.wake),
                );
            }
        }
        pass.deform.source = deform_source;
        if let Some(render) = world.ecs.resource::<SnowRender>() {
            let snow = world.ecs.resource::<SnowResources>().expect("snow state");
            deform_pass::write(
                &mut pass.deform,
                &renderer.queue,
                &render.gpu,
                &deform_uniforms,
                deform::staging(&snow.deform),
            );
        }
    }

    if let Some(render) = world.ecs.resource_mut::<SnowRender>() {
        render.gpu.deform_read = 1 - deform_source;
    }
    let snow = world
        .ecs
        .resource_mut::<SnowResources>()
        .expect("snow state");
    deform::end_frame(&mut snow.deform);

    // The device-side number comes from the renderer, which brackets every
    // submission the frame makes.
    let gpu_milliseconds = renderer.timing.milliseconds;
    if let Some(pass) = render_graph_get_pass_mut(&mut renderer.graph, "snow_world")
        && let Some(pass) = (pass as &mut dyn std::any::Any).downcast_mut::<SnowWorldPass>()
    {
        let (draws, triangles) = (pass.draw_calls, pass.triangles);
        let perf = world.res_mut::<Perf>();
        perf.draw_calls = draws;
        perf.triangles = triangles;
        perf.gpu_milliseconds = gpu_milliseconds;
    }

    if let Some(pass) = render_graph_get_pass_mut(&mut renderer.graph, "snow_post")
        && let Some(pass) = (pass as &mut dyn std::any::Any).downcast_mut::<SnowPostPass>()
    {
        post_pass::advance(pass);
        post_pass::write(pass, &renderer.queue, &post_uniforms);
    }

    if let Some(render) = world.ecs.resource_mut::<SnowRender>() {
        render.post.previous_view_projection = unjittered;
        render.post.frame = render.post.frame.wrapping_add(1);
        if render.post.history_valid < 1.0 {
            render.post.history_valid += 0.5;
        }
        let arm = world
            .ecs
            .resource::<SnowResources>()
            .map(|snow| snow.rig.distance)
            .unwrap_or(6.2);
        let render = world.ecs.resource_mut::<SnowRender>().expect("snow render");
        render.post.focus_distance += (arm - render.post.focus_distance) * 0.06;
    }
}

/// Projects a world point through a column-major matrix and divides by w.
fn project_point(matrix: &[f32; 16], point: nalgebra_glm::Vec3) -> [f32; 2] {
    let x = matrix[0] * point.x + matrix[4] * point.y + matrix[8] * point.z + matrix[12];
    let y = matrix[1] * point.x + matrix[5] * point.y + matrix[9] * point.z + matrix[13];
    let w = matrix[3] * point.x + matrix[7] * point.y + matrix[11] * point.z + matrix[15];
    let inverse = 1.0 / w.abs().max(1e-4);
    [x * inverse, y * inverse]
}

fn run_static_bakes(renderer: &mut WgpuRenderer, world: &mut World) {
    let (wind_angle, height_amplitude) = {
        let settings = world.res::<Settings>();
        (settings::wind_angle(settings), settings.macro_height_scale)
    };

    let mut encoder = renderer
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("snow_static_bakes"),
        });
    {
        let render = world.ecs.resource::<SnowRender>().expect("snow render");
        bake::run_static(
            &render.bakes,
            &renderer.device,
            &renderer.queue,
            &mut encoder,
            &render.gpu,
            wind_angle,
            height_amplitude,
        );
    }
    renderer.queue.submit(std::iter::once(encoder.finish()));

    let render = world.ecs.resource::<SnowRender>().expect("snow render");
    let height = begin_read(
        &renderer.device,
        &renderer.queue,
        &render.gpu.height.texture,
        render.gpu.height.width,
        render.gpu.height.height,
        8,
    );
    world
        .ecs
        .resource_mut::<SnowRender>()
        .expect("snow render")
        .boot = Boot::Terrain(height);
}

/// Re-solves the sky when the sun has moved.
///
/// The solve is the boot sequence's, a step per frame, so moving the sun puts
/// the demo back into it and the dial stays live while it runs.
fn solve_sky(renderer: &mut WgpuRenderer, world: &mut World) {
    {
        let sun = sky::sun_settings(world.res::<Settings>());
        let snow = world
            .ecs
            .resource_mut::<SnowResources>()
            .expect("snow state");
        sky::sync_from_settings(&mut snow.sky, &sun);
        if !snow.sky.dirty {
            return;
        }
        snow.sky.dirty = false;
        snow.sky.ground_bounce = nalgebra_glm::Vec3::zeros();
    }

    begin_sky_iteration(renderer, world, 0);
}

fn bake_sky_once(renderer: &mut WgpuRenderer, world: &mut World) {
    let sample = {
        let snow = world.ecs.resource::<SnowResources>().expect("snow state");
        SkySample {
            direction: snow.sky.sun_direction,
            intensity: snow.sky.sun_scale,
            ground_bounce: snow.sky.ground_bounce,
        }
    };

    let mut encoder = renderer
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("snow_sky_bake"),
        });
    {
        let render = world.ecs.resource::<SnowRender>().expect("snow render");
        bake::run_sky(
            &render.bakes,
            &renderer.device,
            &renderer.queue,
            &mut encoder,
            &render.gpu,
            &sample,
        );
    }
    renderer.queue.submit(std::iter::once(encoder.finish()));
}
