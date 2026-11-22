//! A UI draw over a render texture the player offers: the viewer runs the frame's own offscreen
//! operations first and the overlay samples what they wrote, in one submission, and a texture
//! the player no longer offers is not shown again.
//!
//! The render texture is the 8 by 8 texture the viewer's offscreen tests clear to yellow; the
//! stock scene is not yellow anywhere.

use super::overlay::pixel;
use super::{Driver, host};
use crate::renderer::retained_tests::offscreen::target_operations;
use crate::{RetainedUpdate, SharedGpu};
use std::collections::BTreeMap;
use std::sync::Arc;
use worldrunner_render_gpu::device::DeviceRequirements;
use worldrunner_render_plan::{TextureFilter, TextureSampler, TextureWrap};
use worldrunner_render_scene::simulation::{UiOverlay, UiTarget};
use worldrunner_world_simulation::graphics::RenderTextureId;
use worldrunner_world_simulation::ui::UiTexture;
use worldrunner_world_simulation::ui::presentation::{UiDraw, UiDrawKind, UiPresentation};

const YELLOW: [u8; 4] = [255, 255, 0, 255];

fn id() -> RenderTextureId {
    RenderTextureId::from_bits(1)
}

fn sampler() -> TextureSampler {
    TextureSampler {
        filter: TextureFilter::Point,
        wrap_u: TextureWrap::Clamp,
        wrap_v: TextureWrap::Clamp,
    }
}

/// A 16 by 16 pixel quad at 8 to 24 over the render texture.
fn overlay() -> UiOverlay {
    let draw = UiDraw {
        kind: UiDrawKind::Graphic,
        depth_test: false,
        triangle_uv: None,
        corners: [[8., 8., 0.], [24., 8., 0.], [24., 24., 0.], [8., 24., 0.]],
        uv_rect: [0., 0., 1., 1.],
        color: [1.; 4],
        corner_colors: None,
        distance_field: None,
        texture: Some(UiTexture::Target(id())),
        material: None,
        sorting_layer: 0,
        sorting_order: 0,
        hierarchy_order: 0,
        clip_rect: [0, 0, 64, 64],
    };
    let presentation = UiPresentation {
        draws: vec![draw],
        ..UiPresentation::default()
    };
    UiOverlay::from_presentation(
        &presentation,
        &BTreeMap::new(),
        &worldrunner_world_simulation::ui::presentation::UiLayers::default(),
    )
    .unwrap()
}

#[test]
fn a_frame_shows_a_render_texture_its_own_admitted_operations_cleared() {
    let mut gpu = SharedGpu::headless_for_test();
    let (device, authority) = gpu
        .device(&crate::tests::options(), &DeviceRequirements::default())
        .unwrap();
    let (operations, texture, _runtime) = target_operations(&device, &authority);
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = host(crate::renderer::retained_tests::source())
        .windowless(&mut driver, Some(gpu))
        .unwrap();
    let baseline = super::capture(&mut run, &events).1;
    assert_ne!(
        pixel(&baseline, 16, 16),
        YELLOW,
        "the stock scene is not yellow"
    );

    run.host_mut()
        .apply(RetainedUpdate::Ui(Arc::new(overlay())))
        .unwrap();
    run.host_mut()
        .show_ui_textures(vec![UiTarget::new(id(), Some(texture), sampler()).unwrap()]);
    run.host_mut().admit_offscreen(operations);
    let shown = super::capture(&mut run, &events).1;

    assert_eq!(
        pixel(&shown, 16, 16),
        YELLOW,
        "the overlay sampled what the frame's operations wrote before it"
    );
    assert_eq!(pixel(&shown, 40, 40), pixel(&baseline, 40, 40));
    run.shutdown().unwrap();
}

#[test]
fn a_texture_withdrawn_from_the_offer_is_not_shown_again() {
    let mut gpu = SharedGpu::headless_for_test();
    let (device, authority) = gpu
        .device(&crate::tests::options(), &DeviceRequirements::default())
        .unwrap();
    let (operations, texture, _runtime) = target_operations(&device, &authority);
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = host(crate::renderer::retained_tests::source())
        .windowless(&mut driver, Some(gpu))
        .unwrap();
    let baseline = super::capture(&mut run, &events).1;
    run.host_mut()
        .apply(RetainedUpdate::Ui(Arc::new(overlay())))
        .unwrap();
    run.host_mut()
        .show_ui_textures(vec![UiTarget::new(id(), Some(texture), sampler()).unwrap()]);
    run.host_mut().admit_offscreen(operations);
    assert_eq!(pixel(&super::capture(&mut run, &events).1, 16, 16), YELLOW);

    run.host_mut().show_ui_textures(Vec::new());
    let withdrawn = super::capture(&mut run, &events).1;

    assert_eq!(
        pixel(&withdrawn, 16, 16),
        pixel(&baseline, 16, 16),
        "no offered texture draws as transparent over the scene"
    );
    run.shutdown().unwrap();
}

#[test]
fn an_overlay_staged_before_its_texture_arrives_shows_it_once_offered() {
    let mut gpu = SharedGpu::headless_for_test();
    let (device, authority) = gpu
        .device(&crate::tests::options(), &DeviceRequirements::default())
        .unwrap();
    let (operations, texture, _runtime) = target_operations(&device, &authority);
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = host(crate::renderer::retained_tests::source())
        .windowless(&mut driver, Some(gpu))
        .unwrap();
    let baseline = super::capture(&mut run, &events).1;
    run.host_mut()
        .apply(RetainedUpdate::Ui(Arc::new(overlay())))
        .unwrap();
    let unoffered = super::capture(&mut run, &events).1;
    assert_eq!(pixel(&unoffered, 16, 16), pixel(&baseline, 16, 16));

    run.host_mut().admit_offscreen(operations);
    run.host_mut()
        .show_ui_textures(vec![UiTarget::new(id(), Some(texture), sampler()).unwrap()]);
    let offered = super::capture(&mut run, &events).1;

    assert_eq!(pixel(&offered, 16, 16), YELLOW);
    run.shutdown().unwrap();
}
