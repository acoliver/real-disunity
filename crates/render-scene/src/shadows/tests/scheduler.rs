use super::*;
use worldrunner_render_post::{BufferReadback, FrameError as ScheduleError};

fn uniforms(context: &GpuContext, renderer: &ShadowedScene) -> Vec<Vec<u8>> {
    let mut encoder = context
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    let buffers = [
        renderer.gpu.frame_buffer(),
        &renderer.depth_frame,
        &renderer.map.uniform,
    ];
    let mut captures: Vec<_> = buffers
        .iter()
        .map(|buffer| {
            let mut capture = BufferReadback::new(context.device(), buffer.size(), 4096).unwrap();
            capture.encode(&mut encoder, buffer).unwrap();
            capture
        })
        .collect();
    let submission = context.queue().submit([encoder.finish()]);
    captures
        .iter_mut()
        .map(|capture| {
            capture
                .read_encoded(context.device(), Some(submission.clone()))
                .unwrap()
        })
        .collect()
}

#[test]
fn rejected_shadow_contract_preserves_uniforms_and_published_frame() {
    let context = context();
    let source = scene(&[[0, 0, 1_000]], 600_000, None);
    let mut renderer = ShadowedScene::new(&context, &source, settings()).unwrap();
    let target = OffscreenTarget::new(context.device(), view().viewport()).unwrap();
    let before = uniforms(&context, &renderer);
    assert_eq!(renderer.gpu.current_frame(), None);
    let wrong = worldrunner_render_post::color_texture(
        context.device(),
        "wrong",
        [257, 241],
        wgpu::TextureFormat::Rgba8Unorm,
    );
    let color = AttachmentView::new(ResourceRef::new("scene color"), &wrong).unwrap();
    let mut encoder = context
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    assert!(matches!(
        renderer.encode_with(
            &context,
            &view(),
            (&color, &target.depth_attachment()),
            &mut encoder,
            None,
        ),
        Err(ShadowError::Schedule(ScheduleError::AttachmentContract(
            PassSlot::Opaque
        )))
    ));
    assert_eq!(renderer.gpu.current_frame(), None);
    context.queue().submit([encoder.finish()]);
    assert_eq!(uniforms(&context, &renderer), before);
}

#[test]
fn stock_water_and_non_neutral_grading_are_explicit_errors() {
    let mut water = fixture::effects_parts(600_000);
    water.water = Some(SceneWaterPlane {
        east_min_millimeters: 0,
        north_min_millimeters: 0,
        east_extent_millimeters: 1000,
        north_extent_millimeters: 1000,
    });
    assert_eq!(
        FrameUniform::new(&view(), Some(&SceneEffects::new(water).unwrap())),
        Err(FrameError::UnsupportedEffect("water"))
    );
    for contrast in [true, false] {
        let mut grading = fixture::effects_parts(600_000);
        if contrast {
            grading.grading.contrast_micro = 900_000;
        } else {
            grading.grading.saturation_micro = 900_000;
        }
        assert!(matches!(
            SceneEffects::new(grading),
            Err(SceneValidationError::UnsupportedColorGrading {
                observed: 900_000,
                neutral: 1_000_000,
                ..
            })
        ));
    }
}
