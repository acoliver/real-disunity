#[path = "stock_scene_retry/fixture.rs"]
mod fixture;
#[path = "stock_scene_retry/products.rs"]
mod products;
#[path = "stock_scene_retry/scene.rs"]
mod scene;

use products::Products;
use scene::Scene;
use worldrunner_render_scene::{device::GpuContext, test_support::SceneTestSession, wgpu};

#[test]
fn public_stock_retained_motion_completes_and_settles_with_taa_and_ssr() {
    let _session = SceneTestSession::acquire();
    let context = GpuContext::shared_for_test_with_features(wgpu::Features::empty()).unwrap();
    let validation = context
        .device()
        .push_error_scope(wgpu::ErrorFilter::Validation);
    let source = fixture::source();
    let mut scene = Scene::new(&context, &source);
    scene.complete(&context);
    let initial = Products::read(&context, &scene.frame);
    initial.assert_confidence(0.0);
    scene.seed_confidence(&context);
    Products::read(&context, &scene.frame).assert_confidence(0.5);
    scene.move_retained_instance();
    scene.complete(&context);
    let moved = Products::read(&context, &scene.frame);
    moved.assert_instance_motion(&initial);
    moved.assert_confidence(0.0);
    scene.seed_confidence(&context);
    Products::read(&context, &scene.frame).assert_confidence(0.5);
    scene.complete(&context);
    let settled = Products::read(&context, &scene.frame);
    settled.assert_motion_settled();
    settled.assert_confidence(0.0);
    assert!(pollster::block_on(validation.pop()).is_none());
}
