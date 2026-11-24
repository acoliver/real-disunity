//! The game's screen in a retained session on a shared device: shown in place of the stock scene
//! once a submitted frame wrote it, under the UI, withdrawn by a new allocation, and never copied
//! through the CPU for the window's own frame.
//!
//! The screen is the 8 by 8 render texture the viewer's offscreen tests clear to yellow, stored as
//! sRGB. A yellow capture can only come from the screen: the stock scene is not yellow anywhere.

use super::overlay::{apply, pixel, plan, region};
use super::{Driver, host};
use crate::renderer::retained_tests::offscreen::{
    clearing_operations, is_cleared, screen_operations,
};
use crate::{GameScreen, SharedGpu};
use worldrunner_render_gpu::device::DeviceRequirements;
use worldrunner_render_gpu::submission::SubmissionAuthority;
use worldrunner_render_plan::RenderScene;
use worldrunner_render_scene::offscreen::RgbaImage;

const YELLOW: [u8; 4] = [255, 255, 0, 255];
const RED: [u8; 4] = [255, 0, 0, 255];

pub(super) fn shared() -> (SharedGpu, wgpu::Device, SubmissionAuthority) {
    let mut gpu = SharedGpu::headless_for_test();
    let (device, authority) = gpu
        .device(&crate::tests::options(), &DeviceRequirements::default())
        .unwrap();
    (gpu, device, authority)
}

pub(super) fn scene() -> RenderScene {
    crate::renderer::retained_tests::source()
}

fn all(image: &RgbaImage, expected: [u8; 4]) -> bool {
    let (width, height) = (image.viewport.width, image.viewport.height);
    (0..height).all(|y| (0..width).all(|x| pixel(image, x, y) == expected))
}

/// A texture of the game screen's format that nothing wrote.
pub(super) fn blank(device: &wgpu::Device, size: u32) -> wgpu::Texture {
    device.create_texture(&blank_descriptor(size))
}

pub(super) fn blank_descriptor(size: u32) -> wgpu::TextureDescriptor<'static> {
    wgpu::TextureDescriptor {
        label: Some("blank game screen"),
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    }
}

#[test]
fn a_screen_the_handed_operations_wrote_replaces_the_stock_scene_and_the_ui_draws_over_it() {
    let (gpu, device, authority) = shared();
    let (operations, texture, _runtime) = screen_operations(&device, &authority);
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = host(scene()).windowless(&mut driver, Some(gpu)).unwrap();
    let baseline = super::capture(&mut run, &events).1;
    assert!(!all(&baseline, YELLOW), "the stock scene is not yellow");

    run.host_mut()
        .show_game_screen(Some(GameScreen::new(texture, false).unwrap()));
    run.host_mut().admit_offscreen(operations);
    let before = authority.submitted_serial();
    let mut submitted = false;
    let shown = super::capture_with(&mut run, &events, |run| {
        if !submitted {
            submitted = run.frame().unwrap();
        }
    })
    .1;
    assert!(all(&shown, YELLOW), "the screen covers the stock scene");
    assert_eq!(
        authority.submitted_serial() - before,
        1,
        "the screen's writer, its composite and the capture are one submission"
    );

    let later = super::capture(&mut run, &events).1;
    assert!(
        all(&later, YELLOW),
        "a frame with no writer keeps the screen"
    );
    apply(&mut run, plan([1., 0., 0., 1.]));
    let with_ui = super::capture(&mut run, &events).1;
    region(&with_ui, RED);
    assert_eq!(
        pixel(&with_ui, 30, 30),
        YELLOW,
        "the UI covers only its quad"
    );
    run.shutdown().unwrap();
}

#[test]
fn a_screen_whose_writer_was_replaced_before_any_frame_took_it_is_not_shown() {
    let (gpu, device, authority) = shared();
    let (dropped, texture, _first) = screen_operations(&device, &authority);
    let (replacement, image, _second) = clearing_operations(&device, &authority);
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = host(scene()).windowless(&mut driver, Some(gpu)).unwrap();
    let baseline = super::capture(&mut run, &events).1;

    run.host_mut()
        .show_game_screen(Some(GameScreen::new(texture, false).unwrap()));
    run.host_mut().admit_offscreen(dropped);
    run.host_mut().admit_offscreen(replacement);
    let shown = super::capture(&mut run, &events).1;
    let later = super::capture(&mut run, &events).1;
    run.shutdown().unwrap();

    assert_eq!(
        shown, baseline,
        "no write reached the screen: the stock scene"
    );
    assert_eq!(later, baseline, "the dropped write is not counted later");
    assert!(
        is_cleared(&image.bytes(&device)),
        "the work that replaced it did run"
    );
}

#[test]
fn a_new_allocation_starts_without_contents_and_shows_the_stock_scene() {
    let (gpu, device, authority) = shared();
    let (operations, texture, _runtime) = screen_operations(&device, &authority);
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = host(scene()).windowless(&mut driver, Some(gpu)).unwrap();
    let baseline = super::capture(&mut run, &events).1;
    run.host_mut()
        .show_game_screen(Some(GameScreen::new(texture, false).unwrap()));
    run.host_mut().admit_offscreen(operations);
    assert!(all(&super::capture(&mut run, &events).1, YELLOW));

    let resized = GameScreen::new(blank(&device, 16), false).unwrap();
    run.host_mut().show_game_screen(Some(resized));
    let after = super::capture(&mut run, &events).1;
    run.shutdown().unwrap();

    assert_eq!(
        after, baseline,
        "the old contents do not carry to a new texture"
    );
}

#[test]
fn a_screen_the_handed_operations_did_not_export_is_not_marked_written() {
    let (gpu, device, authority) = shared();
    let (operations, written, _runtime) = screen_operations(&device, &authority);
    let unwritten = blank(&device, 8);
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = host(scene()).windowless(&mut driver, Some(gpu)).unwrap();
    let baseline = super::capture(&mut run, &events).1;

    run.host_mut()
        .show_game_screen(Some(GameScreen::new(unwritten, false).unwrap()));
    run.host_mut().admit_offscreen(operations);
    let shown = super::capture(&mut run, &events).1;
    let later = super::capture(&mut run, &events).1;
    run.shutdown().unwrap();

    drop(written);
    assert_eq!(
        shown, baseline,
        "the writer exported another texture: the offered one has no contents"
    );
    assert_eq!(later, baseline, "the offered texture stays unwritten");
}

#[test]
fn work_exporting_a_withdrawn_screen_runs_in_a_stock_frame() {
    let (gpu, device, authority) = shared();
    let (operations, texture, _runtime) = screen_operations(&device, &authority);
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = host(scene()).windowless(&mut driver, Some(gpu)).unwrap();
    let baseline = super::capture(&mut run, &events).1;

    run.host_mut()
        .show_game_screen(Some(GameScreen::new(texture, false).unwrap()));
    run.host_mut().show_game_screen(None);
    run.host_mut().admit_offscreen(operations);
    let shown = super::capture(&mut run, &events).1;
    run.shutdown().unwrap();

    assert_eq!(shown, baseline, "no screen is offered: the stock scene");
}

/// Clears `texture` to yellow in a submission of its own, as a flush the player runs does.
fn fill_yellow(device: &wgpu::Device, authority: &SubmissionAuthority, texture: &wgpu::Texture) {
    let mut frame = authority.begin(device).unwrap();
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    drop(frame.begin_render_pass(&wgpu::RenderPassDescriptor {
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: &view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color {
                    r: 1.0,
                    g: 1.0,
                    b: 0.0,
                    a: 1.0,
                }),
                store: wgpu::StoreOp::Store,
            },
        })],
        ..Default::default()
    }));
    frame.finish().submit().unwrap();
}

#[test]
fn contents_the_host_submitted_itself_are_shown_by_a_frame_with_no_writer() {
    let (gpu, device, authority) = shared();
    let texture = blank(&device, 8);
    fill_yellow(&device, &authority, &texture);
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = host(scene()).windowless(&mut driver, Some(gpu)).unwrap();

    run.host_mut()
        .show_game_screen(Some(GameScreen::new(texture, true).unwrap()));
    let image = super::capture(&mut run, &events).1;
    run.shutdown().unwrap();

    assert!(all(&image, YELLOW));
}

#[test]
fn a_submitted_screen_replaces_the_depth_fitted_stock_scene() {
    let options = crate::ViewerOptions {
        size: [64, 64],
        shadows: Some(crate::renderer::retained_tests::shadow_settings()),
        ..crate::tests::options()
    };
    let mut gpu = SharedGpu::headless_for_test();
    let (device, authority) = gpu
        .device(&options, &DeviceRequirements::default())
        .unwrap();
    let texture = blank(&device, 8);
    fill_yellow(&device, &authority, &texture);
    let mut host =
        crate::RetainedHost::new(crate::renderer::sdsm_tests::fitted_scene(), options).unwrap();
    host.show_game_screen(Some(GameScreen::new(texture, true).unwrap()));
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = host.windowless(&mut driver, Some(gpu)).unwrap();

    let image = super::capture(&mut run, &events).1;
    run.shutdown().unwrap();

    assert!(all(&image, YELLOW));
}

#[test]
fn an_ordinary_frame_that_shows_the_screen_is_one_submission_and_no_readback() {
    let (gpu, device, authority) = shared();
    let (operations, texture, _runtime) = screen_operations(&device, &authority);
    let mut driver = Driver::default();
    let mut run = host(scene()).windowless(&mut driver, Some(gpu)).unwrap();
    run.host_mut()
        .show_game_screen(Some(GameScreen::new(texture, false).unwrap()));
    run.host_mut().admit_offscreen(operations);

    for frame in 0..3 {
        let before = authority.submitted_serial();
        assert!(run.frame().unwrap());
        assert_eq!(
            authority.submitted_serial() - before,
            1,
            "frame {frame}: a copy of the screen for the CPU would be a submission of its own"
        );
        // Release the flight slot before asking for another measured frame.
        run.renderer.drain().unwrap();
        assert!(
            authority.settled_through(before + 1),
            "the measured frame still occupies a flight slot"
        );
    }
    run.shutdown().unwrap();
}

#[test]
fn a_screen_that_is_not_a_sampled_single_sample_srgb_texture_is_refused_by_name() {
    let (_gpu, device, _authority) = shared();
    let linear = device.create_texture(&wgpu::TextureDescriptor {
        format: wgpu::TextureFormat::Rgba8Unorm,
        ..blank_descriptor(8)
    });

    let refused = GameScreen::new(linear, false).unwrap_err().to_string();

    assert_eq!(
        refused,
        "the viewer does not support a game screen that is not a single-sample sampled Rgba8UnormSrgb texture"
    );
}
