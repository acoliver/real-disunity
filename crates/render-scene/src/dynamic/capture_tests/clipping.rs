//! Byte-level captures of dynamic quads that carry clip rectangles.
//!
//! The clip rectangle of a quad splits draws in both places that stage quads, and only the
//! retained-UI overlay scissors to it. So the scene's pass has to draw the clipped quads as it
//! draws the same quads without clip rectangles, and the overlay's own pipeline has to draw
//! only what is left inside the scissors. Each comparison is between two renders of one run on
//! one device, never against stored bytes.

use super::*;
use crate::draw::DrawCount;
use crate::dynamic::{DynamicBindings, DynamicGpu, QuadTarget};
use crate::offscreen::OffscreenTarget;
use crate::upload::{material_bind_group_layout, placeholder_with_view};

const FULL: [u32; 4] = [0, 0, SIZE, SIZE];

/// Clip rectangles in lower-left pixels, per quad index.
struct Case {
    name: &'static str,
    clip: fn(usize) -> Option<[u32; 4]>,
    /// Whether the overlay's scissor removes pixels the unclipped quads draw.
    clips: bool,
    /// Whether anything of the quads is left inside the overlay's scissors.
    visible: bool,
}

fn clipped(quads: Vec<DynamicQuad>, clip: impl Fn(usize) -> Option<[u32; 4]>) -> Vec<DynamicQuad> {
    quads
        .into_iter()
        .enumerate()
        .map(|(index, quad)| DynamicQuad {
            clip_rect: clip(index),
            ..quad
        })
        .collect()
}

fn cases() -> Vec<Case> {
    fn case(
        name: &'static str,
        clip: fn(usize) -> Option<[u32; 4]>,
        clips: bool,
        visible: bool,
    ) -> Case {
        Case {
            name,
            clip,
            clips,
            visible,
        }
    }
    vec![
        case(
            "partial",
            |index| (index != 2).then_some([6, 6, 16, 20]),
            true,
            true,
        ),
        case(
            "disjoint",
            |index| {
                Some(if index % 2 == 0 {
                    [0, 0, 14, SIZE]
                } else {
                    [18, 0, 14, SIZE]
                })
            },
            true,
            true,
        ),
        case("full", |_| Some(FULL), false, true),
        case("oversized", |_| Some([8, 8, 100, 100]), true, true),
        case("outside", |_| Some([40, 0, 8, 8]), true, false),
        case(
            "mixed",
            |index| {
                Some(if index % 2 == 0 {
                    FULL
                } else {
                    [SIZE, 4, 4, 4]
                })
            },
            true,
            true,
        ),
    ]
}

#[test]
fn clipped_dynamic_quads_draw_in_the_scene_pass_as_unclipped_quads_do_in_every_target_set() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let textures = textures();
    let context = context();
    for config in &CONFIGS {
        if !supported(&context, config) {
            println!(
                "UNLIT-CLIP-CAPTURE config={} skipped: format or samples unsupported",
                config.name
            );
            continue;
        }
        let unclipped = capture(&context, config, &textures, &quads());
        for case in cases() {
            let quads = clipped(quads(), case.clip);
            let drawn = capture(&context, config, &textures, &quads);
            for (index, (drawn, unclipped)) in drawn.iter().zip(&unclipped).enumerate() {
                assert!(
                    drawn == unclipped,
                    "{} {}: attachment {index} differs from the same quads without clip \
                     rectangles, but the scene's pass does not clip",
                    config.name,
                    case.name
                );
            }
            let adapter = context.adapter().get_info().name;
            for (index, image) in drawn.iter().enumerate() {
                println!(
                    "UNLIT-CLIP-CAPTURE adapter={adapter:?} config={} path=scene case={} attachment={index} bytes={} hash={:016x}",
                    config.name,
                    case.name,
                    image.len(),
                    digest(image)
                );
            }
        }
    }
}

/// A screen-space quad of the overlay: pixels from `(left, bottom)` to `(right, top)`.
fn pixels((left, bottom): (f32, f32), (right, top): (f32, f32)) -> DynamicQuad {
    DynamicQuad {
        overlay: true,
        corners: Some([
            [left, bottom, 0.0],
            [right, bottom, 0.0],
            [right, top, 0.0],
            [left, top, 0.0],
        ]),
        ..DynamicQuad::default()
    }
}

/// Four overlapping quads over the 32 pixel target, two of them textured, in submission order.
fn overlay_quads() -> Vec<DynamicQuad> {
    vec![
        DynamicQuad {
            texture: Some(0),
            tint: [1.0, 1.0, 1.0, 0.8],
            ..pixels((2.0, 2.0), (30.0, 14.0))
        },
        DynamicQuad {
            texture: Some(1),
            tint: [1.0, 0.5, 0.25, 0.6],
            hierarchy_order: 1,
            ..pixels((4.0, 10.0), (26.0, 28.0))
        },
        DynamicQuad {
            tint: [0.2, 0.8, 1.0, 0.5],
            hierarchy_order: 2,
            ..pixels((16.0, 2.0), (30.0, 30.0))
        },
        DynamicQuad {
            tint: [1.0, 0.0, 0.0, 0.5],
            hierarchy_order: 3,
            ..pixels((0.0, 18.0), (32.0, 32.0))
        },
    ]
}

/// What the overlay drew over the target the compositor draws it into, and its draw count.
struct Overlaid {
    image: Vec<u8>,
    drawn: DrawCount,
}

/// Draws `quads` with the overlay's own pipeline and draw into a cleared target of the output
/// format, as the compositor does, but without depth.
fn overlay(context: &GpuContext, textures: &[SceneTexture], quads: &[DynamicQuad]) -> Overlaid {
    let device = context.device();
    let (queue, format) = (context.queue(), OffscreenTarget::COLOR_FORMAT);
    let (placeholder, _) = placeholder_with_view(device, queue);
    let sampler = device.create_sampler(&crate::upload::material_sampler_descriptor(
        worldrunner_render_plan::TextureAnisotropy::default(),
    ));
    let layout = material_bind_group_layout(device);
    let mut staged = DynamicGpu::default();
    staged.set_textures(device, queue, textures).unwrap();
    staged
        .set_quads(
            device,
            queue,
            DynamicBindings {
                placeholder: &placeholder,
                sampler: &sampler,
                target: QuadTarget::Overlay(&layout),
            },
            quads,
        )
        .unwrap();
    let pipeline = crate::renderer::overlay::build(device, format, [SIZE, SIZE], &layout, false);
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("overlay clip capture"),
        size: wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = context.submission().begin(device).unwrap();
    let drawn = {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("overlay clip capture"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(fill(0, format)),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..wgpu::RenderPassDescriptor::default()
        });
        staged
            .draw_overlay(&mut pass, &pipeline, [SIZE, SIZE], None)
            .0
    };
    let staging = copy_results(device, &mut encoder, &[target]);
    let index = encoder.finish().submit().unwrap();
    let image = read_results(device, index, staging).remove(0);
    Overlaid { image, drawn }
}

#[test]
fn clipped_overlay_quads_draw_only_inside_their_scissors() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let textures = textures();
    let context = context();
    let unclipped = overlay(&context, &textures, &overlay_quads());
    let untouched = overlay(&context, &textures, &[]);
    assert!(
        unclipped.image != untouched.image,
        "the unclipped overlay quads drew nothing"
    );
    for case in cases() {
        let quads = clipped(overlay_quads(), case.clip);
        let drawn = overlay(&context, &textures, &quads);
        let name = case.name;
        assert_eq!(
            drawn.image != untouched.image,
            case.visible,
            "{name}: the overlay draws exactly when something is left inside the scissors"
        );
        assert_eq!(
            drawn.image != unclipped.image,
            case.clips,
            "{name}: the scissor changes the output exactly when it removes pixels"
        );
        if !case.clips {
            assert_eq!(drawn.drawn, unclipped.drawn, "{name}: draw counts differ");
        }
        if !case.visible {
            assert_eq!(
                drawn.drawn,
                DrawCount::default(),
                "{name}: a draw was recorded"
            );
        }
    }
}

#[test]
fn an_overlay_scissor_keeps_the_pixels_inside_its_lower_left_rectangle() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let edge = f32::from(u8::try_from(SIZE).unwrap());
    let covering = DynamicQuad {
        tint: [1.0, 1.0, 1.0, 1.0],
        ..pixels((0.0, 0.0), (edge, edge))
    };
    let clip = [6, 4, 16, 10];
    let clipped_quad = DynamicQuad {
        clip_rect: Some(clip),
        ..covering
    };
    let whole = overlay(&context, &[], &[covering]).image;
    let scissored = overlay(&context, &[], &[clipped_quad]).image;
    let untouched = overlay(&context, &[], &[]).image;
    assert!(whole != untouched, "the covering quad drew nothing");
    let bytes_per_pixel = whole.len() / (SIZE * SIZE) as usize;
    for row in 0..SIZE {
        for column in 0..SIZE {
            let start = (row * SIZE + column) as usize * bytes_per_pixel;
            let texel = start..start + bytes_per_pixel;
            // Rows read back from the top, and the clip counts from the bottom.
            let bottom = SIZE - 1 - row;
            let inside = (clip[0]..clip[0] + clip[2]).contains(&column)
                && (clip[1]..clip[1] + clip[3]).contains(&bottom);
            let expected = if inside { &whole } else { &untouched };
            assert!(
                scissored[texel.clone()] == expected[texel],
                "pixel {column}, {bottom} (from the bottom) should be {}",
                if inside { "drawn" } else { "untouched" }
            );
        }
    }
}
