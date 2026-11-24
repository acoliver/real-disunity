//! The base passes of a scene frame without shadows: the opaque pass, its velocity attachment and
//! the celestial pass, with the declaration of the scene feature that draws in them.
use crate::compositor::{FrameRenderer, SceneFeature, attachment, attachment_resource};
use worldrunner_render_post::{
    AttachmentLoad, AttachmentView, FeatureDeclaration, FrameError, PassDeclaration, PassSlot,
};

/// What the unshadowed base is authored from.
#[derive(Clone, Copy)]
pub(crate) struct SceneInputs<'a> {
    pub gpu: &'a crate::draw::SceneGpu,
    pub pipelines: &'a crate::renderer::ScenePipelines,
    pub color: &'a AttachmentView,
    pub depth: &'a AttachmentView,
    pub velocity: Option<&'a AttachmentView>,
    pub depth_mode: crate::renderer::depth::DepthMode,
}

/// The authored base of a scene frame.
pub(crate) struct Base {
    pub declaration: FeatureDeclaration,
    pub passes: Vec<PassDeclaration>,
    pub views: Vec<AttachmentView>,
}

/// Authors the passes of an unshadowed scene frame: Opaque with its colour, depth and velocity,
/// and the celestial pass, with the declaration of the scene feature and the screen-space
/// geometry.
///
/// # Errors
/// When an attachment cannot be viewed as declared.
pub(crate) fn unshadowed(
    renderer: &FrameRenderer,
    device: &wgpu::Device,
    scene: &SceneInputs<'_>,
) -> Result<Base, FrameError> {
    let SceneInputs {
        gpu,
        pipelines,
        color,
        depth,
        velocity,
        depth_mode,
    } = *scene;
    let color_id = attachment_resource("scene color", color);
    let depth_id = renderer.scene_depth_resource(depth);
    let mut opaque = PassDeclaration::new(PassSlot::Opaque);
    opaque.color = Some(renderer.color_attachment(color_id, color));
    opaque.depth = Some(attachment(depth_id, depth, true));
    if depth_mode == crate::renderer::depth::DepthMode::Reuse {
        opaque.depth.as_mut().expect("scene depth").load = AttachmentLoad::Load;
    }
    let mut declaration = SceneFeature::declaration(pipelines);
    if let Some(screen) = &renderer.ssao {
        crate::ssao::passes::geometry_declaration(&mut declaration, screen);
    }
    let mut views = vec![
        color.with_resource(color_id)?,
        depth.with_resource(depth_id)?,
    ];
    crate::compositor::declare_velocity(&mut opaque, &mut views, velocity)?;
    let mut passes = vec![opaque];
    gpu.plan_celestial(
        device,
        color.size(),
        &mut passes,
        &mut views,
        &mut declaration,
    );
    Ok(Base {
        declaration,
        passes,
        views,
    })
}
