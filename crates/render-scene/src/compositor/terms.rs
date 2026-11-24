//! Request-owned pixel-centre opaque replay products, before exposure and fog.
use crate::shader::rewrite::ShaderRewrite;
use crate::{draw::SceneGpu, renderer::ScenePipelines};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureTerm {
    /// Linear reflectance after textures, terrain blending and projected decals.
    Albedo,
    /// Strength-adjusted directional visibility in [0, 1], replicated in RGB.
    SunVisibility,
    DirectDiffuse,
    AmbientDiffuse,
    EnvironmentSpecular,
    SunSpecular,
}

/// The shadow layout a term replay's pipelines are built against and the group it binds at set
/// two.
#[derive(Clone, Copy)]
pub(crate) struct ShadowBinding<'a> {
    pub layout: &'a wgpu::BindGroupLayout,
    pub group: &'a wgpu::BindGroup,
}

/// What a replay of the current frame binds beyond the scene: the shadow binding and the frame
/// renderer whose screen-space group binds at set three.
#[derive(Clone, Copy)]
pub(crate) struct FrameBindings<'a> {
    pub shadow: Option<ShadowBinding<'a>>,
    pub frame: Option<&'a super::FrameRenderer>,
}

/// The groups a replay binds: the shadow group at set two and the screen group at set three.
#[derive(Clone, Copy)]
pub(crate) struct ReplayGroups<'a> {
    pub shadow: Option<&'a wgpu::BindGroup>,
    pub screen: Option<&'a wgpu::BindGroup>,
}

/// Storage is owned by one capture request and must be included in its budget.
pub struct TermCapture {
    pub texture: wgpu::Texture,
    depth: wgpu::Texture,
    term: CaptureTerm,
}
impl TermCapture {
    pub fn new(device: &wgpu::Device, size: [u32; 2], term: CaptureTerm) -> Self {
        Self {
            texture: crate::ssao::passes::texture(device, size, wgpu::TextureFormat::Rgba16Float),
            depth: crate::ssao::passes::texture(device, size, wgpu::TextureFormat::Depth32Float),
            term,
        }
    }
    pub fn allocations(size: [u32; 2]) -> worldrunner_render_gpu::configuration::AllocationSet {
        let mut set = worldrunner_render_gpu::configuration::AllocationSet::default();
        for format in [
            wgpu::TextureFormat::Rgba16Float,
            wgpu::TextureFormat::Depth32Float,
        ] {
            set.texture(
                "opaque term replay",
                [size[0], size[1], 1],
                format,
                [1, 1],
                1,
            );
        }
        set
    }
    pub(crate) fn textures(&self) -> [&wgpu::Texture; 2] {
        [&self.texture, &self.depth]
    }
    /// Replay the current drawing frame. No clock, upload, residency or history advancement.
    pub fn encode(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        gpu: &SceneGpu,
    ) {
        let bindings = FrameBindings {
            shadow: None,
            frame: None,
        };
        self.encode_with_frame(device, encoder, gpu, bindings);
    }
    pub(crate) fn encode_with_frame(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        gpu: &SceneGpu,
        bindings: FrameBindings<'_>,
    ) {
        let FrameBindings { shadow, frame } = bindings;
        let screen = frame.and_then(|frame| frame.ssao.as_ref());
        let pipelines = pipelines(device, self.term, shadow.map(|s| s.layout), screen);
        let group = screen.map(|screen| {
            screen
                .geometry
                .forward
                .as_ref()
                .map_or(&screen.geometry.ambient, |(_, group)| group)
        });
        let groups = ReplayGroups {
            shadow: shadow.map(|s| s.group),
            screen: group,
        };
        self.replay(encoder, gpu, &pipelines, groups);
    }
    /// Draws the term into this capture's texture with `pipelines`, binding the shadow group at
    /// set two and the screen group at set three. The terrain's and the objects' pipelines are
    /// prepared before the pass opens.
    ///
    /// # Panics
    /// Panics when the terrain or object programs do not prepare against the pipelines' layouts,
    /// which the program tests rule out for every context the crate builds. A replay returns no
    /// error.
    pub(crate) fn replay(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        gpu: &SceneGpu,
        pipelines: &ScenePipelines,
        groups: ReplayGroups<'_>,
    ) {
        gpu.prepare_terrain_replay(pipelines)
            .expect("the terrain program of a term replay prepares against its layouts");
        gpu.prepare_objects_replay(pipelines)
            .expect("the object programs of a term replay prepare against their layouts");
        self.record(encoder, groups, |pass| {
            gpu.draw_opaque_capture(pass, pipelines, groups.shadow);
        });
    }
    /// Opens the replay pass over this capture's texture and depth, binds the shadow group at set
    /// two and the screen group at set three, and records `draw` into it.
    pub(crate) fn record(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        groups: ReplayGroups<'_>,
        draw: impl FnOnce(&mut wgpu::RenderPass<'_>),
    ) {
        let color = self
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let depth = self
            .depth
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("opaque term replay"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &color,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        if let Some(group) = groups.shadow {
            pass.set_bind_group(2, group, &[]);
        }
        if let Some(group) = groups.screen {
            pass.set_bind_group(3, group, &[]);
        }
        draw(&mut pass);
    }
}
fn pipelines(
    device: &wgpu::Device,
    term: CaptureTerm,
    shadow: Option<&wgpu::BindGroupLayout>,
    screen: Option<&crate::scene_metadata::SceneMetadata>,
) -> ScenePipelines {
    let reflection = screen.map(|screen| screen.geometry.forward.is_some());
    pipelines_for(device, term, shadow, reflection)
}

/// The pipelines a term replay draws with: `screen` is `None` without screen-space passes and
/// otherwise whether reflections are installed, which decides the group at set three.
pub(crate) fn pipelines_for(
    device: &wgpu::Device,
    term: CaptureTerm,
    shadow: Option<&wgpu::BindGroupLayout>,
    screen: Option<bool>,
) -> ScenePipelines {
    let (object, terrain) = term_sources(crate::shadows::capture_sources(shadow.is_some()), term);
    let reflection = screen == Some(true);
    let layout = screen.map(|_| {
        if reflection {
            crate::ssr::forward::layout(device)
        } else {
            crate::ssao::ambient_layout(device)
        }
    });
    let (object, terrain) = sources((object, terrain), screen.is_some(), reflection);
    ScenePipelines::with_screen_space(
        device,
        crate::renderer::SceneTarget {
            format: wgpu::TextureFormat::Rgba16Float,
            samples: 1,
        },
        &object,
        &terrain,
        shadow,
        layout.as_ref(),
    )
}

pub(crate) fn term_sources(stock: (&str, &str), term: CaptureTerm) -> (String, String) {
    let card = stock
        .0
        .find("fn scene_card_finish_shading(")
        .expect("card shading body");
    split_term_sources(stock, term, Some(card))
}

/// The term sources of a frozen forward source, which has only the ordinary shading body.
#[cfg(test)]
pub(crate) fn frozen_term_sources(stock: (&str, &str), term: CaptureTerm) -> (String, String) {
    split_term_sources(stock, term, None)
}

/// Term sources over an object source whose card shading body starts at `card`.
fn split_term_sources(
    stock: (&str, &str),
    term: CaptureTerm,
    card: Option<usize>,
) -> (String, String) {
    let (mesh_value, terrain_value) = values(term);
    // Card shadow coordinates and gradients were prepared before coverage rejection.
    let card_value = if term == CaptureTerm::SunVisibility {
        "vec3<f32>(direct_sunlight_visibility(raw))"
    } else {
        mesh_value
    };
    let (ordinary, card) = stock.0.split_at(card.unwrap_or(stock.0.len()));
    let target = "return vec4<f32>(apply_exposure(apply_height_fog(colour, input.world_position)), albedo_sample.a);";
    let ordinary = ordinary.anchored(
        "compositor::terms::term_sources",
        1,
        target,
        &format!("return vec4<f32>({mesh_value}, albedo_sample.a);"),
    );
    let object = if card.is_empty() {
        ordinary
    } else {
        ordinary
            + &card.anchored(
                "compositor::terms::term_sources",
                1,
                target,
                &format!("return vec4<f32>({card_value}, albedo_sample.a);"),
            )
    };
    let terrain = stock.1.anchored(
        "compositor::terms::term_sources",
        1,
        "return vec4<f32>(apply_exposure(apply_height_fog(colour, input.world_position)), 1.0);",
        &format!("return vec4<f32>({terrain_value}, 1.0);"),
    );
    let independent = crate::renderer::depth::DepthMode::Independent;
    (
        independent.vertex_source(&object).into_owned(),
        independent.vertex_source(&terrain).into_owned(),
    )
}

fn values(term: CaptureTerm) -> (&'static str, &'static str) {
    match term {
        CaptureTerm::Albedo => ("albedo_sample.rgb", "projected.albedo"),
        CaptureTerm::DirectDiffuse => ("lighting.direct_diffuse", "lighting.direct_diffuse"),
        CaptureTerm::AmbientDiffuse => ("lighting.ambient_diffuse", "lighting.ambient_diffuse"),
        CaptureTerm::EnvironmentSpecular => (
            "lighting.environment_specular",
            "lighting.environment_specular",
        ),
        CaptureTerm::SunSpecular => ("lighting.sun_specular", "lighting.sun_specular"),
        CaptureTerm::SunVisibility => (
            "vec3<f32>(sunlight_visibility(input.world_position, normal))",
            "vec3<f32>(sunlight_visibility(input.world_position, projected.normal))",
        ),
    }
}

pub(crate) fn sources(stock: (String, String), screen: bool, reflection: bool) -> (String, String) {
    let apply = |source: String, terrain| {
        let source = if screen {
            crate::ssao::ambient_source(&source)
        } else {
            source
        };
        if reflection {
            crate::ssr::forward::replay_source(&source, terrain)
        } else {
            source
        }
    };
    (apply(stock.0, false), apply(stock.1, true))
}
