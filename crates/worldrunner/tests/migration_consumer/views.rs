use super::super::policy::Readiness;
use super::{
    capture::{assert_metadata, capture},
    landmarks, wait,
};
use gpu::{
    device::GpuContext, draw::SceneGpu, frame::FrameUniform, offscreen::OffscreenTarget,
    renderer::ScenePipelines, wgpu,
};
use worldrunner::glam::DVec3;
use worldrunner::scene::{
    camera::{
        CameraLens, CameraPose, CameraState, ViewConvention, ViewHandedness, Viewport, resolve,
    },
    gpu,
};

#[test]
fn asymmetric_landmarks_render_with_both_lenses_conventions_and_resize() {
    let context = wait(GpuContext::request_with_features(wgpu::Features::empty())).unwrap();
    let pipelines = ScenePipelines::new(context.device(), OffscreenTarget::COLOR_FORMAT);
    let mut scene = SceneGpu::empty(context.device(), context.submission(), &pipelines).unwrap();
    scene
        .set_dynamic_quads(
            context.device(),
            context.submission(),
            &pipelines,
            &landmarks::quads(),
        )
        .unwrap();
    for size in [[65, 47], [130, 94]] {
        for convention in [
            ViewHandedness::RightHanded,
            ViewHandedness::DirectXLeftHanded,
        ] {
            for lens in lenses() {
                let resolved = resolve_view(size, convention, lens);
                let view = gpu::SceneView::from_resolved(resolved, [0, -10_000, 0]);
                scene.set_frame(context.queue(), FrameUniform::unlit(&view));
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
                landmarks::assert_landmarks(&image, &resolved);
                assert_metadata(&image, view.viewport());
            }
        }
    }
}

fn lenses() -> [CameraLens; 2] {
    [
        CameraLens::Orthographic {
            half_height_meters: 2.0,
            near_meters: 0.0,
            far_meters: 20.0,
        },
        CameraLens::Perspective {
            vertical_fov_radians: 0.4,
            near_meters: 0.1,
            far_meters: 20.0,
        },
    ]
}

fn resolve_view(
    size: [u32; 2],
    convention: ViewHandedness,
    lens: CameraLens,
) -> worldrunner::scene::camera::ResolvedView {
    resolve(&CameraState {
        pose: CameraPose {
            eye_meters: DVec3::new(0.0, -10.0, 0.0),
            forward: DVec3::Y,
            up: DVec3::Z,
        },
        lens,
        viewport: Viewport {
            width: size[0],
            height: size[1],
        },
        convention: ViewConvention {
            handedness: convention,
        },
    })
    .unwrap()
}
