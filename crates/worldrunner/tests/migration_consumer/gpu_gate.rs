#[path = "canonical.rs"]
mod canonical;
#[path = "capture.rs"]
mod capture;
#[path = "landmarks.rs"]
mod landmarks;
#[path = "live.rs"]
mod live;
#[path = "../support/resident.rs"]
mod resident;
#[path = "stage.rs"]
mod stage;
#[path = "views.rs"]
mod views;
use gpu::{
    device::GpuContext,
    draw::SceneGpu,
    dynamic::DynamicQuad,
    frame::FrameUniform,
    offscreen::{OffscreenTarget, RgbaImage},
    renderer::ScenePipelines,
    wgpu,
};
use std::{
    future::Future,
    sync::Arc,
    task::{Context, Poll, Wake, Waker},
    time::Duration,
};
use worldrunner::scene::{camera::ViewHandedness, gpu};

struct ThreadWake(std::thread::Thread);
impl Wake for ThreadWake {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.unpark();
    }
}

fn wait<F: Future>(future: F) -> F::Output {
    let waker = Waker::from(Arc::new(ThreadWake(std::thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        if let Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "GPU acquisition deadline"
        );
        std::thread::park_timeout(Duration::from_millis(10));
    }
}

fn pixel(image: &RgbaImage, x: u32, y: u32) -> [u8; 4] {
    let index = usize::try_from((y * image.viewport.width + x) * 4).unwrap();
    image.pixels[index..index + 4].try_into().unwrap()
}

#[test]
fn external_device_target_and_live_contribution_reach_landmark_pixels() {
    let context = wait(GpuContext::request_with_features(wgpu::Features::empty()))
        .expect("required GPU consumer requires an admitted adapter");
    eprintln!("migration adapter: {:?}", context.adapter().get_info());
    let resolved = super::camera::planar([65, 47], ViewHandedness::RightHanded);
    let view = gpu::SceneView::from_resolved(resolved, [0, -10_000, 0]);
    let pipelines = ScenePipelines::new(context.device(), OffscreenTarget::COLOR_FORMAT);
    let mut scene = SceneGpu::empty(context.device(), context.submission(), &pipelines).unwrap();
    let contribution = DynamicQuad {
        tint: [1.0, 0.0, 0.0, 1.0],
        ..DynamicQuad::default()
    };
    apply_contribution(&mut scene, &context, &pipelines, contribution);
    scene.set_frame(context.queue(), FrameUniform::unlit(&view));
    let mut target = OffscreenTarget::new(context.device(), view.viewport()).unwrap();
    let image = target.render(&context, &scene, &pipelines).unwrap();
    assert_eq!(
        pixel(&image, 32, 23),
        [255, 0, 0, 255],
        "caller contribution must reach canonical scene output"
    );
}

fn apply_contribution(
    scene: &mut SceneGpu,
    context: &GpuContext,
    pipelines: &ScenePipelines,
    contribution: DynamicQuad,
) {
    scene
        .set_dynamic_quads(
            context.device(),
            context.submission(),
            pipelines,
            &[contribution],
        )
        .unwrap();
}
