use super::*;
use worldrunner_render_post::{BufferReadback, FrameError as ScheduleError};

pub(super) fn uniforms(context: &GpuContext, renderer: &ShadowedScene) -> Vec<Vec<u8>> {
    let mut encoder = context.submission().begin(context.device()).unwrap();
    let mut buffers = vec![renderer.gpu.frame_buffer(), &renderer.map.uniform];
    buffers.extend(renderer.depth_frames.iter().map(|(buffer, _)| buffer));
    let mut captures: Vec<_> = buffers
        .iter()
        .map(|buffer| {
            let mut capture = BufferReadback::new(context.device(), buffer.size(), 4096).unwrap();
            capture.encode(&mut encoder, buffer).unwrap();
            capture
        })
        .collect();
    let submission = encoder.finish().submit().unwrap();
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
    let mut encoder = context.submission().begin(context.device()).unwrap();
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
    encoder.finish().submit().unwrap();
    assert_eq!(uniforms(&context, &renderer), before);
}

#[test]
fn stock_water_and_non_neutral_grading_are_explicit_errors() {
    let mut water = fixture::effects_parts(600_000);
    water.water = Some(SceneWaterBody {
        height_millimeters: 0,
        color_micro: [50_000, 200_000, 250_000],
        absorption_micro: [300_000, 100_000, 50_000],
        roughness_micro: 200_000,
        east_min_millimeters: 0,
        north_min_millimeters: 0,
        east_extent_millimeters: 1000,
        north_extent_millimeters: 1000,
    });
    assert!(FrameUniform::new(&view(), Some(&SceneEffects::new(water).unwrap())).is_ok());
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

#[test]
fn rejected_cascade_reconfiguration_keeps_the_old_array_and_uniforms() {
    let context = context();
    let source = scene(&[], 600_000, None);
    let mut renderer = ShadowedScene::new(&context, &source, settings()).unwrap();
    let target = OffscreenTarget::new(context.device(), view().viewport()).unwrap();
    let effects = with_cascades(
        source.effects().unwrap(),
        vec![1_000, 2_000, 3_000, 4_000],
        100_000,
    );
    renderer.set_effects(&effects).unwrap();
    let before = uniforms(&context, &renderer);
    let wrong = worldrunner_render_post::color_texture(
        context.device(),
        "wrong",
        [257, 241],
        wgpu::TextureFormat::Rgba8Unorm,
    );
    let color = AttachmentView::new(ResourceRef::new("scene color"), &wrong).unwrap();
    let mut encoder = context.submission().begin(context.device()).unwrap();
    assert!(
        renderer
            .encode(
                &context,
                &view(),
                &color,
                &target.depth_attachment(),
                &mut encoder
            )
            .is_err()
    );
    encoder.finish().submit().unwrap();
    assert_eq!(renderer.map.depth.layers(), 1);
    assert_eq!(uniforms(&context, &renderer), before);
    render_target(&mut renderer, &context, &view(), &target).unwrap();
    assert_eq!(renderer.map.depth.layers(), 4);
}
