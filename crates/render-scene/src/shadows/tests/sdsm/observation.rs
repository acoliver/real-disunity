//! A depth-fitted frame with the G-buffer observation on: the fitted frame's capture
//! contract has to declare the four G-buffer targets the `OpaqueGbuffer` slot writes.
use super::*;
use crate::gbuffer::{GBUFFER_FORMATS, Gbuffer, GbufferUse};
use crate::shadows::sdsm::{AdoptionPolicy, SdsmSettings};
use worldrunner_render_post::{
    BuildContext, DrawContext, FeatureDeclaration, FrameError, FrameFeature, PrepareContext,
    PreparedFeature,
};

/// A producer in the G-buffer slot that writes nothing and records that its pass ran.
struct GbufferRecorder;

struct GbufferRecorderDraw;

impl FrameFeature for GbufferRecorder {
    fn declare(&self) -> FeatureDeclaration {
        let mut declaration = FeatureDeclaration::new(PassSlot::OpaqueGbuffer);
        declaration.passes[0].color_format = Some(GBUFFER_FORMATS[0]);
        declaration.passes[0].additional_color_formats = GBUFFER_FORMATS[1..].to_vec();
        declaration
    }

    fn build(&self, _: &BuildContext<'_>) -> Result<Box<dyn PreparedFeature>, FrameError> {
        Ok(Box::new(GbufferRecorderDraw))
    }
}

impl PreparedFeature for GbufferRecorderDraw {
    fn prepare(&mut self, _: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        Ok(())
    }

    fn draw(&self, context: &DrawContext, _: &mut wgpu::RenderPass<'_>) {
        crate::scene_metadata::record_pass(context.slot);
    }
}

#[test]
fn gpu_depth_fitted_frame_renders_with_the_gbuffer_observation() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    for adoption in [
        AdoptionPolicy::SameFrameCapture,
        AdoptionPolicy::Interactive {
            latency_frames: 2,
            contraction_frames: 0,
        },
    ] {
        let (mut frame, mut renderer) = screen_renderers(&context);
        let size = [view().viewport().width, view().viewport().height];
        let format = frame.targets().scene().texture().format();
        frame.gbuffer = Some(Gbuffer::new(
            context.device(),
            size,
            (format, false),
            GbufferUse::Observe,
        ));
        frame
            .register(context.device(), Box::new(GbufferRecorder))
            .unwrap();
        renderer
            .set_depth_fitting(
                &context,
                Some(SdsmSettings {
                    policy: adoption,
                    range: policy(),
                }),
            )
            .unwrap();
        let color = AttachmentView::new(
            ResourceRef::new("scene color"),
            frame.targets().scene().texture(),
        )
        .unwrap();
        let depth = AttachmentView::new(
            ResourceRef::new("scene depth"),
            frame.targets().depth().texture(),
        )
        .unwrap();
        crate::scene_metadata::EXECUTED_PASSES.with_borrow_mut(Vec::clear);
        renderer
            .render_depth_fitted_with(&context, &view(), (&color, &depth), 3, &mut frame)
            .unwrap_or_else(|error| panic!("{adoption:?}: {error:?}"));
        crate::scene_metadata::EXECUTED_PASSES.with_borrow(|passes| {
            assert_eq!(
                passes
                    .iter()
                    .filter(|&&pass| pass == PassSlot::OpaqueGbuffer)
                    .count(),
                1,
                "{adoption:?}: the G-buffer pass runs once in the fitted frame"
            );
        });
    }
}
