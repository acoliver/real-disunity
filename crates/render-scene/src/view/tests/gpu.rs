use crate::device::GpuContext;
use crate::draw::SceneGpu;
use crate::dynamic::DynamicQuad;
use crate::frame::FrameUniform;
use crate::offscreen::{OffscreenTarget, RgbaImage};
use crate::renderer::ScenePipelines;
use crate::{CameraPose, OrthographicLens, SceneView, Viewport};
use glam::Vec3;

const LANDMARKS: [([f32; 3], [f32; 4], [u8; 4]); 3] = [
    ([-1.5, 0.0, 0.75], [0.0, 1.0, 0.0, 1.0], [0, 255, 0, 255]),
    ([1.25, 0.0, -0.5], [0.0, 0.0, 1.0, 1.0], [0, 0, 255, 255]),
    ([0.0, 0.0, 1.5], [1.0, 1.0, 0.0, 1.0], [255, 255, 0, 255]),
];

fn view(viewport: Viewport) -> SceneView {
    SceneView::orthographic_with_up(
        CameraPose {
            eye_millimeters: [0, -10_000, 0],
            target_millimeters: [0, 0, 0],
        },
        OrthographicLens {
            vertical_half_extent_meters: 3.0,
            near_meters: 0.0,
            far_meters: 20.0,
        },
        [0.0, 0.0, 1.0],
        viewport,
    )
    .unwrap()
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "fixture points project inside these small viewports; sample containing pixel"
)]
fn assert_landmarks(image: &RgbaImage, view: &SceneView) {
    for (point, _, color) in LANDMARKS {
        let screen = view
            .resolved_view()
            .world_to_screen(Vec3::from_array(point))
            .unwrap();
        let x = screen.x.floor() as usize;
        let y = screen.y.floor() as usize;
        let offset = (y * image.viewport.width as usize + x) * 4;
        assert_eq!(
            &image.pixels[offset..offset + 4],
            &color,
            "landmark {point:?} at ({x}, {y})"
        );
    }
}

fn retain_evidence(image: &RgbaImage) {
    if let Ok(prefix) = std::env::var("RESOLVED_CAMERA_EVIDENCE") {
        let path = format!(
            "{prefix}-{}x{}.rgba",
            image.viewport.width, image.viewport.height
        );
        std::fs::write(path, &image.pixels).unwrap();
    }
}

#[test]
fn required_gpu_landmarks_match_camera_queries_and_resize() {
    let context = pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty()))
        .expect("required headless landmark test needs an admitted GPU adapter");
    eprintln!("resolved camera GPU: {:?}", context.adapter().get_info());
    let pipelines = ScenePipelines::new(context.device(), OffscreenTarget::COLOR_FORMAT);
    let mut gpu = SceneGpu::empty(context.device(), context.submission(), &pipelines).unwrap();
    let quads = LANDMARKS.map(|(position, tint, _)| DynamicQuad {
        position,
        tint,
        ..DynamicQuad::default()
    });
    gpu.set_dynamic_quads(context.device(), context.submission(), &pipelines, &quads)
        .unwrap();
    for viewport in [
        Viewport {
            width: 65,
            height: 47,
        },
        Viewport {
            width: 130,
            height: 94,
        },
    ] {
        let view = view(viewport);
        gpu.set_frame(context.queue(), FrameUniform::unlit(&view));
        let mut target = OffscreenTarget::new(context.device(), viewport).unwrap();
        let image = target.render(&context, &gpu, &pipelines).unwrap();
        assert_landmarks(&image, &view);
        retain_evidence(&image);
    }
}

fn shared_view(
    viewport: Viewport,
    lens: worldrunner_camera::CameraLens,
    handedness: worldrunner_camera::ViewHandedness,
) -> SceneView {
    let resolved = worldrunner_camera::resolve(&worldrunner_camera::CameraState {
        pose: worldrunner_camera::CameraPose {
            eye_meters: glam::DVec3::new(0.0, -10.0, 0.0),
            forward: glam::DVec3::Y,
            up: glam::DVec3::Z,
        },
        lens,
        viewport: worldrunner_camera::Viewport {
            width: viewport.width,
            height: viewport.height,
        },
        convention: worldrunner_camera::ViewConvention { handedness },
    })
    .unwrap();
    SceneView::from_resolved(resolved, [0, -10_000, 0])
}

#[test]
fn required_gpu_shared_resolver_matches_pixels_for_lenses_and_handedness() {
    use worldrunner_camera::{CameraLens, ViewHandedness};

    let context = pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty()))
        .expect("required shared-camera test needs an admitted GPU adapter");
    eprintln!("shared resolver GPU: {:?}", context.adapter().get_info());
    let pipelines = ScenePipelines::new(context.device(), OffscreenTarget::COLOR_FORMAT);
    let mut gpu = SceneGpu::empty(context.device(), context.submission(), &pipelines).unwrap();
    let quads = LANDMARKS.map(|(position, tint, _)| DynamicQuad {
        position,
        tint,
        ..DynamicQuad::default()
    });
    gpu.set_dynamic_quads(context.device(), context.submission(), &pipelines, &quads)
        .unwrap();
    for lens in [
        CameraLens::Perspective {
            vertical_fov_radians: 2.0 * 0.3_f64.atan(),
            near_meters: 1.0,
            far_meters: 20.0,
        },
        CameraLens::Orthographic {
            half_height_meters: 3.0,
            near_meters: 0.0,
            far_meters: 20.0,
        },
    ] {
        for handedness in [
            ViewHandedness::RightHanded,
            ViewHandedness::DirectXLeftHanded,
        ] {
            for (width, height) in [(65, 47), (130, 94)] {
                let viewport = Viewport { width, height };
                let view = shared_view(viewport, lens, handedness);
                gpu.set_frame(context.queue(), FrameUniform::unlit(&view));
                let mut target = OffscreenTarget::new(context.device(), viewport).unwrap();
                let image = target.render(&context, &gpu, &pipelines).unwrap();
                assert_landmarks(&image, &view);
            }
        }
    }
}
