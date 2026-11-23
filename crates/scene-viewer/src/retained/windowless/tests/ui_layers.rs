//! The overlay shows the texture a host offers for the runs of UI draws the player drew with a
//! game's shader, at the sort position of each run, with rows the right way up.
use super::game_screen::{blank, blank_descriptor, scene, shared};
use super::overlay::pixel;
use super::{Driver, capture, host};
use crate::RetainedUpdate;
use std::collections::BTreeMap;
use std::sync::Arc;
use worldrunner_render_scene::simulation::UiOverlay;
use worldrunner_world_simulation::graphics::MaterialId;
use worldrunner_world_simulation::ui::presentation::{
    UiDraw, UiDrawKind, UiMaterial, UiPresentation,
};

const SIZE: u32 = 64;
const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const YELLOW: [u8; 4] = [255, 255, 0, 255];
const MAGENTA: [u8; 4] = [255, 0, 255, 255];

/// A graphic draw of `rect` (x, y, width, height from the lower left) in `order`.
fn draw(material: Option<UiMaterial>, rect: [f32; 4], order: i32, color: [f32; 4]) -> UiDraw {
    let [x, y, width, height] = rect;
    UiDraw {
        kind: UiDrawKind::Graphic,
        depth_test: false,
        triangle_uv: None,
        corners: [
            [x, y, 0.],
            [x + width, y, 0.],
            [x + width, y + height, 0.],
            [x, y + height, 0.],
        ],
        uv_rect: [0., 0., 1., 1.],
        color,
        distance_field: None,
        corner_colors: None,
        texture: None,
        material,
        sorting_layer: 0,
        sorting_order: order,
        hierarchy_order: 0,
        clip_rect: [0, 0, SIZE, SIZE],
    }
}

/// A red quad under the run (x 16 to 48), the run (x 20 to 28, y 20 to 28, the lower left of
/// the atlas) and a yellow quad over its upper right corner (x 26 to 34).
fn overlay() -> Arc<UiOverlay> {
    let material = UiMaterial {
        id: MaterialId::from_bits(1),
        generation: 1,
    };
    let plan = UiPresentation {
        draws: vec![
            draw(None, [16., 16., 32., 32.], 0, [1., 0., 0., 1.]),
            draw(Some(material), [20., 20., 8., 8.], 1, [1.; 4]),
            draw(None, [26., 26., 8., 8.], 2, [1., 1., 0., 1.]),
        ],
        ..UiPresentation::default()
    };
    let layers = plan.layers([SIZE, SIZE], 4096);
    assert_eq!(layers.runs.len(), 1);
    assert_eq!(layers.runs[0].atlas, [0, 0, 8, 8]);
    Arc::new(UiOverlay::from_presentation(&plan, &BTreeMap::new(), &layers).unwrap())
}

/// An atlas whose rows differ: its first four rows, the bottom of the run, hold `low` and the
/// next four hold `high`. Every other texel is magenta, so a wrong rectangle shows.
fn atlas(device: &wgpu::Device, queue: &wgpu::Queue, [low, high]: [[u8; 4]; 2]) -> wgpu::Texture {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        ..blank_descriptor(SIZE)
    });
    let texels: Vec<u8> = (0..SIZE)
        .flat_map(|row| {
            (0..SIZE).flat_map(move |column| match (row, column) {
                (0..4, 0..8) => low,
                (4..8, 0..8) => high,
                _ => MAGENTA,
            })
        })
        .collect();
    queue.write_texture(
        texture.as_image_copy(),
        &texels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(SIZE * 4),
            rows_per_image: Some(SIZE),
        },
        texture.size(),
    );
    texture
}

#[test]
fn the_overlay_shows_the_layer_texture_between_two_default_quads() {
    let (gpu, device, authority) = shared();
    let layers = atlas(&device, authority.queue(), [BLUE, GREEN]);
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = host(scene()).windowless(&mut driver, Some(gpu)).unwrap();
    run.host_mut().apply(RetainedUpdate::Ui(overlay())).unwrap();
    run.host_mut().show_ui_layers(Some(layers)).unwrap();
    let image = capture(&mut run, &events).1;
    run.shutdown().unwrap();

    assert_eq!(pixel(&image, 17, 17), RED, "the quad under the run");
    assert_eq!(
        pixel(&image, 21, 21),
        BLUE,
        "the bottom rows of the atlas run"
    );
    assert_eq!(pixel(&image, 27, 21), BLUE, "the right edge of the run");
    assert_eq!(
        pixel(&image, 21, 27),
        GREEN,
        "the top rows of the atlas run"
    );
    assert_eq!(pixel(&image, 20, 27), GREEN, "the left edge of the run");
    assert_eq!(
        pixel(&image, 28, 21),
        RED,
        "past the run, no magenta from the atlas"
    );
    assert_eq!(pixel(&image, 19, 25), RED, "left of the run");
    assert_eq!(pixel(&image, 27, 27), YELLOW, "the quad over the run");
    assert_eq!(pixel(&image, 31, 31), YELLOW, "the quad over the red one");
}

#[test]
fn a_translucent_layer_composites_as_source_over_the_quad_under_it() {
    let (gpu, device, authority) = shared();
    // The atlas holds colour multiplied by alpha, as a program draws it: half-covering blue is
    // 0.5 blue (188 as sRGB) with an alpha of 0.5 (128).
    let half_blue = [0, 0, 188, 128];
    let layers = atlas(&device, authority.queue(), [half_blue, half_blue]);
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = host(scene()).windowless(&mut driver, Some(gpu)).unwrap();
    run.host_mut().apply(RetainedUpdate::Ui(overlay())).unwrap();
    run.host_mut().show_ui_layers(Some(layers)).unwrap();
    let image = capture(&mut run, &events).1;
    run.shutdown().unwrap();

    // Half the blue over red: 0.5 of each in linear light, which sRGB stores as 188.
    let over = pixel(&image, 21, 21);
    for (channel, expected) in [187, 0, 188, 255].into_iter().enumerate() {
        assert!(
            over[channel].abs_diff(expected) <= 2,
            "channel {channel}: {over:?}"
        );
    }
}

#[test]
fn an_unwritten_layer_is_not_shown() {
    let (gpu, device, _authority) = shared();
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = host(scene()).windowless(&mut driver, Some(gpu)).unwrap();
    run.host_mut().apply(RetainedUpdate::Ui(overlay())).unwrap();
    let before_any_offer = capture(&mut run, &events).1;
    run.host_mut()
        .show_ui_layers(Some(blank(&device, SIZE)))
        .unwrap();
    let blank_layers = capture(&mut run, &events).1;
    run.shutdown().unwrap();

    for image in [&before_any_offer, &blank_layers] {
        assert_eq!(pixel(image, 21, 21), RED, "the run shows nothing yet");
        assert_eq!(pixel(image, 27, 27), YELLOW);
    }
}

#[test]
fn a_new_layer_texture_replaces_the_old_one_and_withdrawing_it_hides_the_run() {
    let (gpu, device, authority) = shared();
    let first = atlas(&device, authority.queue(), [BLUE, GREEN]);
    let second = atlas(&device, authority.queue(), [YELLOW, BLUE]);
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = host(scene()).windowless(&mut driver, Some(gpu)).unwrap();
    run.host_mut().apply(RetainedUpdate::Ui(overlay())).unwrap();
    run.host_mut().show_ui_layers(Some(first)).unwrap();
    assert_eq!(pixel(&capture(&mut run, &events).1, 21, 21), BLUE);
    run.host_mut().show_ui_layers(Some(second)).unwrap();
    assert_eq!(pixel(&capture(&mut run, &events).1, 21, 21), YELLOW);
    run.host_mut().show_ui_layers(None).unwrap();
    assert_eq!(pixel(&capture(&mut run, &events).1, 21, 21), RED);
    run.shutdown().unwrap();
}

#[test]
fn a_texture_of_another_format_is_refused() {
    let (gpu, device, _authority) = shared();
    let mut driver = Driver::default();
    let mut run = host(scene()).windowless(&mut driver, Some(gpu)).unwrap();
    let wrong = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("not a layer atlas"),
        size: wgpu::Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    assert!(run.host_mut().show_ui_layers(Some(wrong)).is_err());
    run.shutdown().unwrap();
}
