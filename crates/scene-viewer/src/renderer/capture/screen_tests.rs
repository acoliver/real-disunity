//! A frame that shows the game's screen does not encode the stock scene, so the attachments that
//! scene writes, such as depth and normals, would hold an earlier frame's contents. Captures of
//! them are refused before anything is submitted, and what the host queued stays queued.
use super::feature_tests::setup;
use super::*;
use crate::renderer::retained_tests::offscreen::{
    clearing_operations, is_cleared, screen_operations,
};
use crate::{
    CaptureJitter, CaptureOutput, GameScreen, RetainedCaptureOutcome, RetainedCaptureRecipe,
    RetainedEvent,
};

const REFUSAL: &str = "capture of frame attachments while the game screen replaces the stock scene";

fn attachments() -> RetainedCaptureRecipe {
    RetainedCaptureRecipe::new(
        1,
        1,
        CaptureJitter::None,
        false,
        vec![
            CaptureOutput::Srgb8,
            CaptureOutput::Depth,
            CaptureOutput::Normals,
        ],
        16 * 1024 * 1024,
    )
    .unwrap()
}

/// A texture of the game screen's format that nothing wrote.
fn blank(device: &wgpu::Device) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("blank game screen"),
        size: wgpu::Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    })
}

fn assert_refused(host: &mut RetainedHost, renderer: &mut Renderer) {
    let authority = renderer.context.submission().clone();
    let before = authority.submitted_serial();
    assert!(matches!(
        renderer.render_retained(host),
        Err(ViewerError::UnsupportedScene(message)) if message == REFUSAL
    ));
    assert_eq!(
        authority.submitted_serial(),
        before,
        "nothing was submitted"
    );
    assert_eq!(
        host.captures.waiting.len(),
        1,
        "the request was not consumed"
    );
}

#[test]
fn attachments_are_refused_while_a_screen_with_contents_is_offered() {
    let (mut host, mut renderer) = setup(1, false);
    let screen = GameScreen::new(blank(renderer.context.device()), true).unwrap();
    host.show_game_screen(Some(screen));
    host.request_capture_recipe(0, attachments()).unwrap();

    assert_refused(&mut host, &mut renderer);
}

#[test]
fn attachments_are_refused_before_the_queued_screen_writer_is_taken() {
    let (mut host, mut renderer) = setup(1, false);
    let device = renderer.context.device().clone();
    let authority = renderer.context.submission().clone();
    let (operations, texture, _runtime) = screen_operations(&device, &authority);
    host.show_game_screen(Some(GameScreen::new(texture, false).unwrap()));
    host.admit_offscreen(operations);
    host.request_capture_recipe(0, attachments()).unwrap();

    assert_refused(&mut host, &mut renderer);

    assert!(
        host.offscreen.is_some(),
        "the refused frame left the queued write with the host"
    );
}

#[test]
fn attachments_are_captured_from_the_stock_scene_while_the_screen_has_no_contents() {
    let (mut host, mut renderer) = setup(1, false);
    let screen = GameScreen::new(blank(renderer.context.device()), false).unwrap();
    host.show_game_screen(Some(screen));
    let device = renderer.context.device().clone();
    let (other, image, _runtime) = clearing_operations(&device, renderer.context.submission());
    host.admit_offscreen(other);
    let id = host.request_capture_recipe(0, attachments()).unwrap();

    assert!(renderer.render_retained(&mut host).unwrap());
    renderer.drain().unwrap();
    renderer.collect_retained(&mut host).unwrap();

    let Some(RetainedEvent::Capture {
        id: actual,
        outcome: RetainedCaptureOutcome::RecipeCompleted { products, .. },
        ..
    }) = host.captures.take_event()
    else {
        panic!("recipe result");
    };
    assert_eq!(actual, id);
    assert!(products.depth.is_some());
    assert!(products.normals.is_some());
    assert!(is_cleared(&image.bytes(&device)));
}

#[test]
fn lighting_only_capture_is_refused_before_the_screen_writer_is_taken() {
    let (mut host, mut renderer) = setup(1, false);
    let device = renderer.context.device().clone();
    let (ops, texture, _runtime) = screen_operations(&device, renderer.context.submission());
    host.show_game_screen(Some(GameScreen::new(texture, false).unwrap()));
    host.admit_offscreen(ops);
    let lighting = RetainedCaptureRecipe::new(
        1,
        1,
        CaptureJitter::None,
        false,
        vec![CaptureOutput::DirectDiffuse],
        16 * 1024 * 1024,
    )
    .unwrap();
    host.request_capture_recipe(0, lighting).unwrap();
    assert_refused(&mut host, &mut renderer);
    assert!(host.offscreen.is_some());
}

#[test]
fn each_stock_lighting_product_requires_the_stock_scene() {
    for term in [
        CaptureOutput::Albedo,
        CaptureOutput::SunVisibility,
        CaptureOutput::DirectDiffuse,
        CaptureOutput::AmbientDiffuse,
        CaptureOutput::EnvironmentSpecular,
        CaptureOutput::SunSpecular,
    ] {
        assert!(Products::requires_stock_scene(&[term]), "{term:?}");
        assert!(
            Products::requires_stock_scene(&[CaptureOutput::Srgb8, term]),
            "{term:?}"
        );
    }
    assert!(!Products::requires_stock_scene(&[
        CaptureOutput::Srgb8,
        CaptureOutput::LinearHdr
    ]));
}

#[test]
fn mixed_display_and_lighting_capture_completes_after_the_screen_is_withdrawn() {
    let (mut host, mut renderer) = setup(1, false);
    let screen = GameScreen::new(blank(renderer.context.device()), true).unwrap();
    host.show_game_screen(Some(screen));
    let recipe = RetainedCaptureRecipe::new(
        1,
        1,
        CaptureJitter::None,
        false,
        vec![CaptureOutput::Srgb8, CaptureOutput::DirectDiffuse],
        16 * 1024 * 1024,
    )
    .unwrap();
    let id = host.request_capture_recipe(0, recipe).unwrap();
    assert_refused(&mut host, &mut renderer);

    host.show_game_screen(None);
    assert!(renderer.render_retained(&mut host).unwrap());
    renderer.drain().unwrap();
    renderer.collect_retained(&mut host).unwrap();
    let Some(RetainedEvent::Capture {
        id: completed,
        outcome: RetainedCaptureOutcome::RecipeCompleted { products, .. },
        ..
    }) = host.captures.take_event()
    else {
        panic!("expected the refused request to complete on the stock frame");
    };
    assert_eq!(completed, id);
    assert!(products.srgb8.is_some());
    assert!(products.direct_diffuse().is_some());
}

#[test]
fn attachments_are_captured_when_the_queued_writer_exports_another_texture() {
    let (mut host, mut renderer) = setup(1, false);
    let device = renderer.context.device().clone();
    let authority = renderer.context.submission().clone();
    let (operations, _written, _runtime) = screen_operations(&device, &authority);
    host.show_game_screen(Some(GameScreen::new(blank(&device), false).unwrap()));
    host.admit_offscreen(operations);
    let id = host.request_capture_recipe(0, attachments()).unwrap();

    assert!(renderer.render_retained(&mut host).unwrap());
    renderer.drain().unwrap();
    renderer.collect_retained(&mut host).unwrap();

    let Some(RetainedEvent::Capture {
        id: actual,
        outcome: RetainedCaptureOutcome::RecipeCompleted { products, .. },
        ..
    }) = host.captures.take_event()
    else {
        panic!("recipe result");
    };
    assert_eq!(actual, id);
    assert!(products.depth.is_some());
}
