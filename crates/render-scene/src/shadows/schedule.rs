use worldrunner_render_post::{
    AttachmentView, FeatureDeclaration, PassDeclaration, PassSlot, ResourceRef,
};
use worldrunner_render_shadow::DepthMap;

use super::ShadowError;

pub(super) struct ShadowSchedule {
    pub passes: Vec<PassDeclaration>,
    pub views: Vec<AttachmentView>,
    pub declaration: FeatureDeclaration,
}

impl ShadowSchedule {
    pub fn celestial(
        &mut self,
        gpu: &crate::draw::SceneGpu,
        device: &wgpu::Device,
        size: [u32; 2],
    ) {
        gpu.plan_celestial(
            device,
            size,
            &mut self.passes,
            &mut self.views,
            &mut self.declaration,
        );
    }

    pub fn screen_space(&mut self, scheduler: &crate::compositor::FrameRenderer) {
        if scheduler.ssao.is_some() {
            crate::ssao::passes::geometry_declaration(&mut self.declaration);
            if scheduler.ssr.is_some() {
                crate::ssr::geometry_declaration(&mut self.declaration);
            }
        }
    }
    pub fn new(
        map: &DepthMap,
        color: &AttachmentView,
        depth: &AttachmentView,
        pipelines: &crate::renderer::ScenePipelines,
    ) -> Result<Self, ShadowError> {
        let color_id = crate::compositor::attachment_resource("scene color", color);
        let depth_id = crate::compositor::attachment_resource("scene depth", depth);
        let mut passes = Vec::new();
        let mut views = Vec::new();
        let mut declaration = FeatureDeclaration::new(PassSlot::ShadowCasters(0));
        declaration.passes.clear();
        let mut opaque_feature = crate::compositor::SceneFeature::declaration(pipelines)
            .passes
            .remove(0);
        for index in 0..map.layers() {
            let shadow = ResourceRef {
                layer: index,
                ..ResourceRef::new("shadow depth")
            };
            let attachment = AttachmentView::new(shadow, map.texture())?;
            let slot = PassSlot::ShadowCasters(index);
            let mut pass = PassDeclaration::new(slot);
            pass.depth = Some(crate::compositor::attachment(shadow, &attachment, true));
            passes.push(pass);
            views.push(attachment);
            let mut feature = FeatureDeclaration::new(slot).passes.remove(0);
            feature.depth_format = Some(wgpu::TextureFormat::Depth32Float);
            declaration.passes.push(feature);
            opaque_feature.reads.push(shadow);
        }
        let mut opaque = PassDeclaration::new(PassSlot::Opaque);
        opaque.color = Some(crate::compositor::attachment(color_id, color, false));
        opaque.depth = Some(crate::compositor::attachment(depth_id, depth, true));
        passes.push(opaque);
        views.push(color.with_resource(color_id)?);
        views.push(depth.with_resource(depth_id)?);
        declaration.passes.push(opaque_feature);
        Ok(Self {
            passes,
            views,
            declaration,
        })
    }
}
