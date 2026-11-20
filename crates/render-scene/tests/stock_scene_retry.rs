#[path = "stock_scene_retry/capture_retry.rs"]
mod capture_retry;
#[path = "stock_scene_retry/fixture.rs"]
mod fixture;
#[path = "stock_scene_retry/prefix_identity.rs"]
mod prefix_identity;
#[path = "stock_scene_retry/products.rs"]
mod products;
#[path = "stock_scene_retry/refusal.rs"]
mod refusal;
#[path = "stock_scene_retry/scene.rs"]
mod scene;

use products::Products;
use refusal::Refusal;
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
    let initial = Products::read(&context, &scene);
    initial.assert_confidence(0.0);
    scene.seed_confidence(&context);
    Products::read(&context, &scene).assert_confidence(0.5);
    scene.move_retained_instance();
    scene.complete(&context);
    let moved = Products::read(&context, &scene);
    moved.assert_instance_motion(&initial);
    moved.assert_confidence(0.0);
    scene.seed_confidence(&context);
    Products::read(&context, &scene).assert_confidence(0.5);
    scene.complete(&context);
    let settled = Products::read(&context, &scene);
    settled.assert_motion_settled();
    settled.assert_confidence(0.0);
    assert!(pollster::block_on(validation.pop()).is_none());
}

#[test]
fn public_caller_refusal_preserves_completed_products_and_retry_matches_control() {
    let _session = SceneTestSession::acquire();
    let context = GpuContext::shared_for_test_with_features(wgpu::Features::empty()).unwrap();
    let validation = context
        .device()
        .push_error_scope(wgpu::ErrorFilter::Validation);
    let source = fixture::source();
    let mut scene = Scene::new(&context, &source);
    let refusal = Refusal::default();
    scene
        .frame
        .register(context.device(), Box::new(refusal.clone()))
        .unwrap();
    scene.complete(&context);
    scene.seed_confidence(&context);
    let completed = Products::read(&context, &scene);
    completed.assert_confidence(0.5);
    let statistics = scene.gpu.statistics();
    let selection = scene.gpu.view_stats();
    scene.move_retained_instance();
    refusal.set_refusing(true);
    let submitted = context.submission().submitted_serial();
    assert_eq!(scene.try_complete(&context), Err(Refusal::error()));
    assert_eq!(context.submission().submitted_serial(), submitted);
    assert_eq!(refusal.preparations(), 2);
    assert_eq!(scene.frame.projection_jitter_index(), 1);
    assert!(scene.frame.has_temporal_history());
    assert_eq!(scene.gpu.statistics(), statistics);
    assert_eq!(scene.gpu.view_stats(), selection);
    assert_eq!(Products::read(&context, &scene), completed);
    refusal.set_refusing(false);
    scene.complete(&context);
    assert_eq!(refusal.preparations(), 3);
    let retried = Products::read(&context, &scene);
    retried.assert_instance_motion(&completed);
    retried.assert_confidence(0.0);
    let mut control = Scene::new(&context, &source);
    control.complete(&context);
    control.seed_confidence(&context);
    control.move_retained_instance();
    control.complete(&context);
    assert_eq!(retried, Products::read(&context, &control));
    assert_eq!(scene.gpu.statistics(), control.gpu.statistics());
    scene.seed_confidence(&context);
    control.seed_confidence(&context);
    scene.complete(&context);
    control.complete(&context);
    assert_eq!(refusal.preparations(), 4);
    assert_eq!(scene.gpu.statistics(), control.gpu.statistics());
    let settled = Products::read(&context, &scene);
    settled.assert_motion_settled();
    settled.assert_confidence(0.0);
    assert_eq!(settled, Products::read(&context, &control));
    assert!(pollster::block_on(validation.pop()).is_none());
}

#[test]
fn public_stock_depth_prefix_refusal_retries_retained_motion_with_taa_and_ssr() {
    let _session = SceneTestSession::acquire();
    let context = GpuContext::shared_for_test_with_features(wgpu::Features::empty()).unwrap();
    let validation = context
        .device()
        .push_error_scope(wgpu::ErrorFilter::Validation);
    let source = fixture::source();
    capture_retry::assert_retry(&context, &source);
    assert!(pollster::block_on(validation.pop()).is_none());
}
