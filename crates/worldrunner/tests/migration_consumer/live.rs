use super::super::{camera::planar, generation, policy::Readiness};
use super::{
    capture::{assert_metadata, capture, pixel},
    landmarks, stage, wait,
};
use gpu::{
    device::GpuContext, draw::SceneGpu, dynamic::DynamicQuad, frame::FrameUniform,
    offscreen::OffscreenTarget, renderer::ScenePipelines, wgpu,
};
use std::cell::Cell;
use worldrunner::glam::Vec3;
use worldrunner::scene::{camera::ViewHandedness, gpu};

#[test]
fn two_resident_updates_and_caller_stage_obey_capture_policy() {
    let context = wait(GpuContext::request_with_features(wgpu::Features::empty())).unwrap();
    let resolved = planar([65, 47], ViewHandedness::RightHanded);
    let view = gpu::SceneView::from_resolved(resolved, [0, -10_000, 0]);
    let pipelines = ScenePipelines::new(context.device(), OffscreenTarget::COLOR_FORMAT);
    let mut scene = SceneGpu::empty(context.device(), context.submission(), &pipelines).unwrap();
    scene.set_frame(context.queue(), FrameUniform::unlit(&view));
    let mut target = OffscreenTarget::new(context.device(), view.viewport()).unwrap();
    let stage_calls = Cell::new(0);
    let marker = stage::marker_pipeline(context.device(), OffscreenTarget::COLOR_FORMAT);
    assert_blocked(&context, &scene, &pipelines, &mut target);
    update_texture(&context, &mut scene, &pipelines, 0);
    let first = capture(
        &context,
        &scene,
        &pipelines,
        &mut target,
        Readiness::Ready,
        |_, _| {},
    )
    .unwrap();
    assert_metadata(&first, view.viewport());
    assert_texture(&first, &resolved, false);
    update_texture(&context, &mut scene, &pipelines, 1);
    update_texture(&context, &mut scene, &pipelines, 2);
    let final_image = capture(
        &context,
        &scene,
        &pipelines,
        &mut target,
        Readiness::Ready,
        |encoder, output| {
            stage_calls.set(stage_calls.get() + 1);
            stage::marker(encoder, output, &marker);
        },
    )
    .unwrap();
    assert_eq!(stage_calls.get(), 1);
    assert_texture(&final_image, &resolved, true);
    assert_eq!(pixel(&first, 1, 1), [0, 0, 0, 255]);
    assert_eq!(pixel(&final_image, 1, 1), [255, 255, 0, 255]);
    assert_ne!(first.pixels, final_image.pixels);
    assert_metadata(&final_image, view.viewport());
}

fn update_texture(
    context: &GpuContext,
    scene: &mut SceneGpu,
    pipelines: &ScenePipelines,
    revision: u8,
) {
    scene
        .set_dynamic_textures(
            context.device(),
            context.submission(),
            &[landmarks::texture(revision)],
        )
        .unwrap();
    scene
        .set_dynamic_quads(
            context.device(),
            context.submission(),
            pipelines,
            &[DynamicQuad {
                rect: [-1.5, -1.5, 1.5, 1.5],
                texture: Some(0),
                ..DynamicQuad::default()
            }],
        )
        .unwrap();
}

fn assert_texture(
    image: &gpu::offscreen::RgbaImage,
    view: &gpu_camera::ResolvedView,
    swapped: bool,
) {
    let points = [
        [-1.1, 0.0, 1.1],
        [1.1, 0.0, 1.1],
        [-1.1, 0.0, -1.1],
        [1.1, 0.0, -1.1],
    ];
    let mut expected = [
        landmarks::COLORS[0],
        landmarks::COLORS[1],
        landmarks::COLORS[2],
        [255; 4],
    ];
    if swapped {
        expected.swap(0, 3);
    }
    for (point, expected) in points.into_iter().zip(expected) {
        let screen = view.world_to_screen(Vec3::from_array(point)).unwrap();
        let [x, y] = landmarks::texel(screen);
        assert_eq!(pixel(image, x, y), expected, "texture corner {point:?}");
    }
}

use worldrunner::scene::camera as gpu_camera;

#[test]
fn canonical_generation_output_drives_visible_consumer_state() {
    let generated = generation::generated();
    let height = generated.document.terrain.heights_millimeters[17];
    let channel = u8::try_from(height.unsigned_abs() % 192 + 32).unwrap();
    let context = wait(GpuContext::request_with_features(wgpu::Features::empty())).unwrap();
    let view = gpu::SceneView::from_resolved(
        planar([65, 47], ViewHandedness::RightHanded),
        [0, -10_000, 0],
    );
    let pipelines = ScenePipelines::new(context.device(), OffscreenTarget::COLOR_FORMAT);
    let mut scene = SceneGpu::empty(context.device(), context.submission(), &pipelines).unwrap();
    scene.set_frame(context.queue(), FrameUniform::unlit(&view));
    scene
        .set_dynamic_textures(
            context.device(),
            context.submission(),
            &[worldrunner::scene::plan::SceneTexture::Texels {
                color_space: worldrunner::scene::plan::TextureColorSpace::Srgb,
                format: worldrunner::scene::plan::TexelFormat::Rgba8Unorm,
                width: 1,
                height: 1,
                mips: 1,
                bytes: vec![channel, 0, 255, 255].into_boxed_slice(),
            }],
        )
        .unwrap();
    scene
        .set_dynamic_quads(
            context.device(),
            context.submission(),
            &pipelines,
            &[DynamicQuad {
                texture: Some(0),
                ..DynamicQuad::default()
            }],
        )
        .unwrap();
    let mut target = OffscreenTarget::new(context.device(), view.viewport()).unwrap();
    let image = capture(
        &context,
        &scene,
        &pipelines,
        &mut target,
        Readiness::Ready,
        |_, _| {},
    )
    .unwrap();
    assert_eq!(pixel(&image, 32, 23), [channel, 0, 255, 255]);
    eprintln!(
        "generation canonical_hash={:?} sample_height_mm={height} pixel_channel={channel}",
        generated.canonical_hash
    );
}

fn assert_blocked(
    context: &GpuContext,
    scene: &SceneGpu,
    pipelines: &ScenePipelines,
    target: &mut OffscreenTarget,
) {
    let stage_calls = Cell::new(0);
    for readiness in [Readiness::Streaming, Readiness::Settling] {
        assert!(
            capture(context, scene, pipelines, target, readiness, |_, _| {
                stage_calls.set(stage_calls.get() + 1);
            })
            .is_err()
        );
    }
    assert_eq!(
        stage_calls.get(),
        0,
        "readiness rejects before the caller stage"
    );
}
