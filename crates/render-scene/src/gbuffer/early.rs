//! Attachment routing for the depth-writing material producer.
use super::{GBUFFER_FORMATS, Gbuffer, GbufferTarget};
use crate::gbuffer_writes::EarlyOutputs;
use worldrunner_render_post::{
    AttachmentDeclaration, AttachmentLoad, AttachmentView, FeaturePass, FrameError,
    PassDeclaration, PassEntry, PassSlot, ResourceRef,
};

impl Gbuffer {
    pub(crate) fn fuse_prepass(
        &self,
        passes: &mut [PassDeclaration],
        views: &mut Vec<AttachmentView>,
        velocity: Option<&AttachmentView>,
    ) -> Result<(), FrameError> {
        let pass = passes
            .iter_mut()
            .find(|pass| pass.slot == PassSlot::DepthNormalPrepass)
            .ok_or(FrameError::MissingPass(PassEntry::new(
                PassSlot::DepthNormalPrepass,
            )))?;
        let normal = pass.color.take().expect("canonical normal target");
        let material = pass
            .additional_colors
            .iter()
            .find(|attachment| {
                attachment.resource == crate::frame::metadata::FrameProduct::material().resource()
            })
            .cloned();
        pass.additional_colors.clear();
        for target in GbufferTarget::ALL {
            let resource = ResourceRef::new(target.resource_name());
            let view = AttachmentView::new(resource, self.texture(target))?;
            let mut attachment = crate::compositor::attachment(resource, &view, false);
            attachment.load = AttachmentLoad::ClearColor(wgpu::Color::TRANSPARENT);
            append(pass, attachment);
            views.push(view);
        }
        if let Some(view) = velocity {
            let resource = ResourceRef::new("scene velocity");
            let mut attachment = crate::compositor::attachment(resource, view, false);
            attachment.load = AttachmentLoad::ClearColor(wgpu::Color::TRANSPARENT);
            append(pass, attachment);
            if !views.iter().any(|existing| existing.resource() == resource) {
                views.push(view.with_resource(resource)?);
            }
        }
        append(pass, normal);
        if let Some(material) = material {
            append(pass, material);
        }
        Ok(())
    }
}

fn append(pass: &mut PassDeclaration, mut attachment: AttachmentDeclaration) {
    attachment.load = AttachmentLoad::ClearColor(wgpu::Color::TRANSPARENT);
    if pass.color.is_none() {
        pass.color = Some(attachment);
    } else {
        pass.additional_colors.push(attachment);
    }
}

pub(crate) fn producer_pass(
    pipelines: &crate::renderer::ScenePipelines,
    depth: wgpu::TextureFormat,
) -> FeaturePass {
    let early = pipelines.early_outputs();
    let mut pass = FeaturePass::new(if early.is_some() {
        PassSlot::DepthNormalPrepass
    } else {
        PassSlot::OpaqueGbuffer
    });
    let formats = early.map_or_else(|| GBUFFER_FORMATS.to_vec(), EarlyOutputs::formats);
    pass.color_format = Some(formats[0]);
    pass.additional_color_formats = formats[1..].to_vec();
    pass.depth_format = Some(depth);
    pass
}

pub(crate) fn move_early_velocity(
    passes: &mut [PassDeclaration],
    views: &[AttachmentView],
) -> Result<Option<AttachmentView>, FrameError> {
    let opaque = passes
        .iter_mut()
        .find(|pass| pass.slot == PassSlot::Opaque)
        .ok_or(FrameError::MissingPass(PassEntry::new(PassSlot::Opaque)))?;
    opaque
        .depth
        .as_mut()
        .ok_or(FrameError::AttachmentContract(PassEntry::new(
            PassSlot::Opaque,
        )))?
        .load = AttachmentLoad::Load;
    let Some(attachment) = opaque
        .additional_colors
        .iter_mut()
        .find(|attachment| attachment.resource.name == "scene velocity")
    else {
        return Ok(None);
    };
    attachment.load = AttachmentLoad::Load;
    views
        .iter()
        .find(|view| view.resource() == attachment.resource)
        .cloned()
        .map(Some)
        .ok_or(FrameError::AttachmentContract(PassEntry::new(
            PassSlot::DepthNormalPrepass,
        )))
}
