#[test]
fn default_capture_pool_plain_execution_after_resize_preserves_driver_error() {
    let (mut host, mut renderer) = super::setup();
    let recipe = crate::RetainedCaptureRecipe::new(
        1,
        1,
        crate::CaptureJitter::None,
        false,
        vec![crate::CaptureOutput::Srgb8],
        u64::MAX,
    )
    .unwrap();
    host.request_capture_recipe(0, recipe).unwrap();
    host.input(crate::RetainedInput::Resized([5120, 5120]));
    let device = renderer.context.device();
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("resized capture output"),
        size: wgpu::Extent3d {
            width: 5120,
            height: 5120,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let result = renderer.streaming.captures.prepare(
        device,
        &mut host,
        &texture,
        &renderer.presentation.scheduler,
        true,
    );
    assert!(matches!(result, Err(crate::ViewerError::Driver(message))
        if message == "capture recipe requires 314572800 bytes, limit is 268435456"));
}

use super::*;
use crate::{CapturePoolPolicy, CaptureRequestError, RetainedInput};

fn setup_pool(size: [u32; 2]) -> (RetainedHost, Renderer) {
    let host = RetainedHost::new(
        crate::renderer::retained_tests::source(),
        crate::ViewerOptions {
            size,
            capture_pool: CapturePoolPolicy::Derived {
                recipes: vec![recipe(2, 1, CaptureJitter::None)],
                max_extent: [64, 64],
                headroom_bytes: 0,
                granularity_bytes: 1,
                cap_bytes: None,
            },
            ..crate::tests::options()
        },
    )
    .unwrap();
    let renderer = pollster::block_on(Renderer::windowless(&host)).unwrap();
    (host, renderer)
}

fn resize(host: &mut RetainedHost, renderer: &mut Renderer, size: [u32; 2]) {
    host.input(RetainedInput::Resized(size));
    renderer.resize(size).unwrap();
}

#[test]
fn capture_pool_grow_rejects_overcommit_before_execution() {
    let (mut host, mut renderer) = setup_pool([32, 32]);
    let temporal = recipe(1, 2, CaptureJitter::None);
    let id = host.request_capture_recipe(0, temporal.clone()).unwrap();
    resize(&mut host, &mut renderer, [64, 64]);
    assert_eq!(
        host.request_capture_recipe(0, temporal),
        Err(CaptureRequestError::RecipeBudget {
            required: 524_288,
            limit: 458_752,
        })
    );
    let products = await_capture(&mut host, &mut renderer, id);
    assert_eq!(products.srgb8.unwrap().viewport.width, 64);
}

#[test]
fn capture_pool_shrink_admits_fitting_queue_and_executes_both() {
    let (mut host, mut renderer) = setup_pool([64, 64]);
    let first = host
        .request_capture_recipe(0, recipe(1, 2, CaptureJitter::None))
        .unwrap();
    resize(&mut host, &mut renderer, [32, 32]);
    let second = host
        .request_capture_recipe(0, recipe(1, 1, CaptureJitter::None))
        .unwrap();
    assert_eq!(
        await_capture(&mut host, &mut renderer, first)
            .srgb8
            .unwrap()
            .viewport
            .width,
        32
    );
    assert_eq!(
        await_capture(&mut host, &mut renderer, second)
            .srgb8
            .unwrap()
            .viewport
            .width,
        32
    );
}

fn assert_inventory(renderer: &Renderer, pool: u64, size: [u32; 2]) {
    let settings = renderer.presentation.scheduler.configuration();
    let ordinary = 4 * (u64::from(size[0]) * 4).next_multiple_of(256) * u64::from(size[1])
        + 3 * u64::from(size[0]) * u64::from(size[1]) * 4;
    let limits = renderer.context.device().limits();
    assert_eq!(
        settings.additional.validate(&limits).unwrap() - ordinary,
        pool
    );
    assert_eq!(renderer.capture_pool_bytes(), pool);
}

#[test]
fn capture_pool_in_flight_shrink_keeps_inventory_through_delivery() {
    let (mut host, mut renderer) = setup_pool([64, 64]);
    let id = host
        .request_capture_recipe(0, recipe(2, 4, CaptureJitter::None))
        .unwrap();
    renderer.render_retained(&mut host).unwrap();
    renderer.drain().unwrap();
    renderer.collect_retained(&mut host).unwrap();
    resize(&mut host, &mut renderer, [32, 32]);
    assert_eq!(host.capture_pool_bytes(), 458_752);
    assert_inventory(&renderer, 458_752, [32, 32]);
    for _ in 0..3 {
        renderer.render_retained(&mut host).unwrap();
        renderer.drain().unwrap();
        renderer.collect_retained(&mut host).unwrap();
        renderer.presentation.scheduler.collect();
        assert_inventory(&renderer, 458_752, [128, 128]);
    }
    assert!(
        matches!(host.captures.take_event(), Some(RetainedEvent::Capture {
        id: actual, outcome: RetainedCaptureOutcome::RecipeCompleted { products, .. }, ..
    }) if actual == id && products.srgb8.as_ref().unwrap().viewport.width == 64)
    );
    assert_inventory(&renderer, 458_752, [128, 128]);
}

#[test]
fn default_capture_pool_execution_after_resize_preserves_driver_error() {
    let mut host = RetainedHost::new(
        crate::renderer::retained_tests::source(),
        crate::ViewerOptions {
            size: [32, 32],
            ..crate::tests::options()
        },
    )
    .unwrap();
    let mut renderer = pollster::block_on(Renderer::windowless(&host)).unwrap();
    let temporal = RetainedCaptureRecipe::new(
        1,
        2,
        CaptureJitter::None,
        false,
        vec![CaptureOutput::Srgb8],
        u64::MAX,
    )
    .unwrap();
    host.request_capture_recipe(0, temporal).unwrap();
    resize(&mut host, &mut renderer, [3000, 1600]);
    assert!(
        matches!(renderer.render_retained(&mut host), Err(ViewerError::Driver(message))
        if message == "capture recipe requires 307302400 bytes, limit is 268435456")
    );
}
