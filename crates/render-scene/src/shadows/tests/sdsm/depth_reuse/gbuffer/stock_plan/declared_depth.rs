use super::super::*;
use std::{cell::Cell, rc::Rc};
use worldrunner_render_post::{
    BuildContext, DrawContext, FeatureDeclaration, FrameError, FrameFeature, PassDeclaration,
    PassEntry, PassSlot, PrepareContext, PreparedFeature,
};

#[derive(Clone, Default)]
struct DepthRead(Rc<Cell<([u32; 2], u32)>>);

struct DepthReader(DepthRead);

impl FrameFeature for DepthReader {
    fn declare(&self) -> FeatureDeclaration {
        let mut declaration = FeatureDeclaration::new(PassSlot::Opaque);
        declaration.passes[0]
            .reads
            .push(crate::frame::metadata::FrameProduct::linear_depth(1).resource());
        declaration
    }

    fn plan(
        &self,
        passes: &mut Vec<PassDeclaration>,
        _: &mut Vec<worldrunner_render_post::AttachmentView>,
    ) -> Result<FeatureDeclaration, FrameError> {
        let entry = PassEntry::new(PassSlot::Opaque);
        let opaque = passes
            .iter()
            .find(|pass| pass.entry() == entry)
            .ok_or(FrameError::AttachmentContract(entry))?;
        let mut declaration = self.declare();
        let pass = &mut declaration.passes[0];
        pass.color_format = opaque.color.as_ref().map(|attachment| attachment.format);
        pass.depth_format = opaque.depth.as_ref().map(|attachment| attachment.format);
        pass.additional_color_formats = opaque
            .additional_colors
            .iter()
            .map(|attachment| attachment.format)
            .collect();
        pass.samples = opaque
            .color
            .as_ref()
            .or(opaque.depth.as_ref())
            .map_or(1, |attachment| attachment.samples);
        Ok(declaration)
    }

    fn build(&self, _: &BuildContext<'_>) -> Result<Box<dyn PreparedFeature>, FrameError> {
        Ok(Box::new(self.0.clone()))
    }
}

impl PreparedFeature for DepthRead {
    fn prepare(&mut self, context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        let extent = context
            .resource(crate::frame::metadata::FrameProduct::linear_depth(1).resource())?
            .size();
        let (_, preparations) = self.0.get();
        self.0.set((extent, preparations + 1));
        Ok(())
    }

    fn draw(&self, _: &DrawContext, _: &mut wgpu::RenderPass<'_>) {}
}

fn assert_fused_prefix(
    context: &GpuContext,
    frame: &mut FrameRenderer,
    renderer: &mut ShadowedScene,
) {
    let prefix = frame
        .stock_depth_prefix(context.device(), (&mut renderer.gpu, &renderer.pipelines))
        .unwrap();
    let plan = prefix.plan();
    assert_eq!(
        plan.passes
            .iter()
            .map(worldrunner_render_post::PassDeclaration::entry)
            .collect::<Vec<_>>(),
        [
            PassEntry::new(PassSlot::DepthNormalPrepass),
            PassEntry::new(PassSlot::DepthPyramid(0)),
        ]
    );
    let prepass = &plan.passes[0];
    let attached: Vec<_> = prepass
        .color
        .iter()
        .chain(&prepass.additional_colors)
        .map(|attachment| attachment.resource)
        .collect();
    for target in GbufferTarget::ALL {
        let resource = ResourceRef::new(target.resource_name());
        assert!(attached.contains(&resource), "{target:?} is attached");
        let backing = frame
            .feature_target(FeatureTarget::Gbuffer(target))
            .unwrap();
        assert!(
            plan.views
                .iter()
                .any(|view| { view.resource() == resource && view.texture() == backing })
        );
    }
}

#[test]
fn gpu_fused_gbuffer_depth_prefix_supplies_declared_level_one_to_registered_reader() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let source = render_source();
    let (mut frame, mut renderer) = fitted_frame(&context, &source);
    assert_fused_prefix(&context, &mut frame, &mut renderer);

    let read = DepthRead::default();
    frame
        .register(context.device(), Box::new(DepthReader(read.clone())))
        .unwrap();
    let (color, depth) = attachments(&frame);
    let viewport = view().viewport();
    crate::scene_metadata::EXECUTED_PASSES.with_borrow_mut(Vec::clear);
    crate::compositor::plan_trace::take();
    let source_frame = 23;
    let fit = renderer
        .render_depth_fitted_with(
            &context,
            &view(),
            (&color, &depth),
            source_frame,
            &mut frame,
        )
        .unwrap()
        .expect("same-frame fitting adopts the captured frame");
    assert_eq!(
        (fit.source_frame, fit.adopted_frame),
        (source_frame, source_frame)
    );
    let level_one = [viewport.width, viewport.height].map(|extent| extent.div_ceil(2));
    assert_eq!(read.0.get(), (level_one, 1));
    assert_eq!(
        crate::compositor::plan_trace::take()
            .iter()
            .filter(|plan| plan.prefix)
            .count(),
        1
    );
    assert_geometry_once(&renderer);
}

fn assert_geometry_once(renderer: &ShadowedScene) {
    for slot in [
        PassSlot::DepthNormalPrepass,
        PassSlot::DepthPyramid(0),
        PassSlot::DepthPyramid(1),
    ] {
        crate::scene_metadata::EXECUTED_PASSES.with_borrow(|passes| {
            assert_eq!(
                passes.iter().filter(|&&pass| pass == slot).count(),
                1,
                "{slot:?}"
            );
        });
    }
    let objects = renderer.gpu.statistics().passes;
    assert!(objects.prepass_objects.meshes.calls > 0);
    assert_eq!(objects.opaque_objects, crate::draw::ObjectDraws::default());
}
