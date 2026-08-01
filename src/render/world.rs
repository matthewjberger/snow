use crate::constants::CASCADE_COUNT;
use crate::render::character as character_pass;
use crate::render::character::CharacterRender;
use crate::render::crystal as crystal_pass;
use crate::render::crystal::CrystalRender;
use crate::render::deform as deform_pass;
use crate::render::deform::Deform;
use crate::render::gpu::SnowGpu;
use crate::render::sky as sky_pass;
use crate::render::sky::Sky;
use crate::render::spray as spray_pass;
use crate::render::spray::SprayRender;
use crate::render::terrain as terrain_pass;
use crate::render::terrain::Terrain;
use crate::render::wake as wake_pass;
use crate::render::wake::WakeRender;
use crate::render::water as water_pass;
use crate::render::water::WaterRender;
use crate::shaders;
use crate::shaders::ShaderLibrary;
use nightshade::prelude::*;
use nightshade::render::wgpu::rendergraph::{Result, SubGraphRunCommand};

/// Every draw the demo makes into the frame, in one graph node.
pub struct SnowWorldPass {
    pub terrain: Terrain,
    pub sky: Sky,
    pub deform: Deform,
    pub character: CharacterRender,
    pub wake: WakeRender,
    pub crystal: CrystalRender,
    pub water: WaterRender,
    pub spray: SprayRender,
    /// Draws and triangles the demo issued last frame, for the overlay.
    pub draw_calls: u32,
    pub triangles: u32,
    bound: bool,
    /// The scene targets, cloned at bind time. The demo owns them so it can
    /// render at its own resolution.
    targets: Option<[wgpu::TextureView; 4]>,
}

pub fn new(
    device: &wgpu::Device,
    library: &mut ShaderLibrary,
    color_format: wgpu::TextureFormat,
) -> SnowWorldPass {
    let sky_module = shaders::compile(library, device, "sky", crate::shaders::SKY);
    SnowWorldPass {
        terrain: terrain_pass::new(device, library, color_format),
        sky: sky_pass::new(device, &sky_module, color_format),
        deform: deform_pass::new(device, library),
        character: character_pass::new(device, library, color_format),
        wake: wake_pass::new(device, library, color_format),
        crystal: crystal_pass::new(device, library, color_format),
        water: water_pass::new(device, library, color_format),
        spray: spray_pass::new(device, library, color_format),
        draw_calls: 0,
        triangles: 0,
        bound: false,
        targets: None,
    }
}

/// Points every program at the persistent textures.
pub fn bind(pass_state: &mut SnowWorldPass, device: &wgpu::Device, gpu: &SnowGpu) {
    pass_state.targets = Some([
        gpu.prepass.view.clone(),
        gpu.prepass_depth.view.clone(),
        gpu.scene.view.clone(),
        gpu.scene_depth.view.clone(),
    ]);
    terrain_pass::bind(&mut pass_state.terrain, device, gpu);
    sky_pass::bind(&mut pass_state.sky, device, gpu);
    deform_pass::bind(&mut pass_state.deform, device, gpu);
    character_pass::bind(&mut pass_state.character, device, gpu);
    wake_pass::bind(&mut pass_state.wake, device, gpu);
    crystal_pass::bind(&mut pass_state.crystal, device, gpu);
    water_pass::bind(&mut pass_state.water, device, gpu);
    spray_pass::bind(&mut pass_state.spray, device, gpu);
    pass_state.bound = true;
}

pub fn is_bound(pass_state: &SnowWorldPass) -> bool {
    pass_state.bound
}

impl PassNode<RenderInputs> for SnowWorldPass {
    fn name(&self) -> &str {
        "snow_world"
    }

    fn reads(&self) -> Vec<&str> {
        Vec::new()
    }

    fn writes(&self) -> Vec<&str> {
        Vec::new()
    }

    fn execute<'r, 'e>(
        &mut self,
        context: PassExecutionContext<'r, 'e, RenderInputs>,
    ) -> Result<Vec<SubGraphRunCommand<'r>>> {
        if !self.bound {
            return Ok(Vec::new());
        }

        deform_pass::record(&self.deform, context.encoder);
        let deform_state = 1 - self.deform.source;

        let Some(targets) = self.targets.clone() else {
            return Ok(Vec::new());
        };
        let [prepass_view, prepass_depth, color_view, depth_view] = &targets;

        for cascade in 0..CASCADE_COUNT {
            let Some((color, depth)) = terrain_pass::cascade_targets(&self.terrain, cascade) else {
                break;
            };
            let mut pass = context
                .encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("snow_cascade"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: color,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: depth,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Discard,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
            terrain_pass::draw_cascade(&self.terrain, &mut pass, cascade, deform_state);
            character_pass::draw_cascade(&self.character, &mut pass, cascade);
            wake_pass::draw_cascade(&self.wake, &mut pass, cascade);
            crystal_pass::draw_cascade(&self.crystal, &mut pass, cascade);
        }

        {
            let mut pass = context
                .encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("snow_prepass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: prepass_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 9000.0,
                                g: 0.0,
                                b: 0.0,
                                a: 1.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: prepass_depth,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Discard,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
            terrain_pass::draw_prepass(&self.terrain, &mut pass, deform_state);
            character_pass::draw_prepass(&self.character, &mut pass);
            wake_pass::draw_prepass(&self.wake, &mut pass);
            crystal_pass::draw_prepass(&self.crystal, &mut pass);
        }

        {
            let mut pass = context
                .encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("snow_scene"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: color_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.02,
                                g: 0.03,
                                b: 0.05,
                                a: 1.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: depth_view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
            sky_pass::draw(&self.sky, &mut pass);
            terrain_pass::draw_beauty(&self.terrain, &mut pass, deform_state);
            character_pass::draw_beauty(&self.character, &mut pass);
            wake_pass::draw_beauty(&self.wake, &mut pass);
            crystal_pass::draw_beauty(&self.crystal, &mut pass);
            // Water before the mist: spray hanging in front of a body of
            // water is far commoner than the reverse, and both read depth alone.
            water_pass::draw(&self.water, &mut pass);
            spray_pass::draw(&self.spray, &mut pass);
        }

        self.draw_calls = terrain_pass::draw_calls(&self.terrain)
            + character_pass::draw_calls(&self.character)
            + wake_pass::draw_calls(&self.wake)
            + crystal_pass::draw_calls(&self.crystal)
            + water_pass::draw_calls(&self.water)
            + spray_pass::draw_calls(&self.spray)
            + 1;
        self.triangles = terrain_pass::triangles(&self.terrain)
            + character_pass::triangles(&self.character)
            + wake_pass::triangles(&self.wake)
            + crystal_pass::triangles(&self.crystal)
            + water_pass::triangles(&self.water)
            + spray_pass::triangles(&self.spray)
            + 12;

        Ok(Vec::new())
    }
}
