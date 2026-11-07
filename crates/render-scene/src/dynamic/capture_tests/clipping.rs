//! Byte-level captures of dynamic quads that carry clip rectangles.
//!
//! The clip rectangle of a quad splits draws in both places that stage quads, and only the
//! retained-UI overlay scissors to it. So the scene's pass is compared with [`Legacy::draw`],
//! which clips nothing, and the overlay's draw with [`Legacy::draw_scissored`], the fixed path's
//! scissor. The overlay's own pipeline is not what is compared here: its draw is recorded with
//! the fixed unlit pipeline, which isolates the grouping, the material groups, the scissor and
//! the counts from the shader it draws with.

use super::*;
use crate::dynamic::{DynamicBindings, DynamicGpu, QuadTarget};

const FULL: [u32; 4] = [0, 0, SIZE, SIZE];

/// Clip rectangles in lower-left pixels, per quad index of [`quads`].
struct Case {
    name: &'static str,
    quads: Vec<DynamicQuad>,
    /// Whether the overlay's scissor removes pixels the scene's pass draws.
    clips: bool,
    /// Whether anything of the quads is left inside the overlay's scissors.
    visible: bool,
}

fn clipped(clip: impl Fn(usize) -> Option<[u32; 4]>) -> Vec<DynamicQuad> {
    quads()
        .into_iter()
        .enumerate()
        .map(|(index, quad)| DynamicQuad {
            clip_rect: clip(index),
            ..quad
        })
        .collect()
}

fn cases() -> Vec<Case> {
    let case = |name, quads, clips, visible| Case {
        name,
        quads,
        clips,
        visible,
    };
    vec![
        case(
            "partial",
            clipped(|index| (index != 2).then_some([6, 6, 16, 20])),
            true,
            true,
        ),
        case(
            "disjoint",
            clipped(|index| {
                Some(if index % 2 == 0 {
                    [0, 0, 14, SIZE]
                } else {
                    [18, 0, 14, SIZE]
                })
            }),
            true,
            true,
        ),
        case("full", clipped(|_| Some(FULL)), false, true),
        case("oversized", clipped(|_| Some([8, 8, 100, 100])), true, true),
        case("outside", clipped(|_| Some([40, 0, 8, 8])), true, false),
        case(
            "mixed",
            clipped(|index| {
                Some(if index % 2 == 0 {
                    FULL
                } else {
                    [SIZE, 4, 4, 4]
                })
            }),
            true,
            true,
        ),
    ]
}

/// What both paths drew, the counts the two paths reported, and the clear colour left alone.
struct Drawn {
    ours: Vec<Vec<u8>>,
    reference: Vec<Vec<u8>>,
    unclipped: Vec<Vec<u8>>,
    untouched: Vec<Vec<u8>>,
    counts: (DrawCount, DrawCount),
}

/// Stages `quads` as the overlay does, over its own material layout.
fn overlay_staged(
    context: &GpuContext,
    textures: &[SceneTexture],
    quads: &[DynamicQuad],
) -> DynamicGpu {
    let (device, queue) = (context.device(), context.queue());
    let (placeholder, _) = placeholder_with_view(device, queue);
    let sampler = material_sampler(device);
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
    staged
}

fn draw_both(
    context: &GpuContext,
    (config, built): (&Config, &Built),
    (textures, quads): (&[SceneTexture], &[DynamicQuad]),
    overlay: bool,
) -> Drawn {
    let view = view();
    let legacy = Legacy::new(context, &built.pipelines, textures, quads, &view);
    let target = (&built.formats[..], config.samples);
    let bind_screen = |pass: &mut wgpu::RenderPass<'_>| {
        if let Some(screen) = &built.screen {
            pass.set_bind_group(3, screen, &[]);
        }
    };
    let mut counts = (DrawCount::default(), DrawCount::default());
    let (ours, reference) = if overlay {
        let staged = overlay_staged(context, textures, quads);
        let ours = render(context, target, |pass| {
            bind_screen(pass);
            pass.set_bind_group(0, legacy.frame_group(), &[]);
            counts.0 = staged
                .draw_overlay(pass, legacy.pipeline(), [SIZE, SIZE], None)
                .0;
        });
        let reference = render(context, target, |pass| {
            bind_screen(pass);
            counts.1 = legacy.draw_scissored(pass, [SIZE, SIZE]);
        });
        (ours, reference)
    } else {
        let mut gpu =
            SceneGpu::empty(context.device(), context.submission(), &built.pipelines).unwrap();
        gpu.set_dynamic_contribution(
            context.device(),
            context.submission(),
            &built.pipelines,
            textures,
            quads,
        )
        .unwrap();
        gpu.set_frame(context.queue(), FrameUniform::unlit(&view));
        let ours = render(context, target, |pass| {
            bind_screen(pass);
            gpu.draw(pass, &built.pipelines);
        });
        let reference = render(context, target, |pass| {
            bind_screen(pass);
            legacy.draw(pass);
        });
        (ours, reference)
    };
    let unclipped = render(context, target, |pass| {
        bind_screen(pass);
        legacy.draw(pass);
    });
    let untouched = render(context, target, |_| {});
    Drawn {
        ours,
        reference,
        unclipped,
        untouched,
        counts,
    }
}

fn print(context: &GpuContext, (config, path, case): (&Config, &str, &str), images: &[Vec<u8>]) {
    let adapter = context.adapter().get_info().name;
    for (index, image) in images.iter().enumerate() {
        println!(
            "UNLIT-CLIP-CAPTURE adapter={adapter:?} config={} path={path} case={case} attachment={index} bytes={} hash={:016x}",
            config.name,
            image.len(),
            digest(image)
        );
    }
}

fn check_unwritten(drawn: &Drawn, (config, case): (&Config, &Case), colour_drawn: bool) {
    for (index, (ours, untouched)) in drawn.ours.iter().zip(&drawn.untouched).enumerate() {
        let name = format!("{} {}", config.name, case.name);
        if index == 0 && colour_drawn {
            assert!(ours != untouched, "{name}: the quads drew nothing");
        } else {
            assert!(ours == untouched, "{name}: attachment {index} was written");
        }
    }
}

fn each_config(mut run: impl FnMut(&GpuContext, &Config, &Built)) {
    let context = context();
    for config in &CONFIGS {
        if !supported(&context, config) {
            println!(
                "UNLIT-CLIP-CAPTURE config={} skipped: format or samples unsupported",
                config.name
            );
            continue;
        }
        let built = build(context.device(), config);
        run(&context, config, &built);
    }
}

#[test]
fn clipped_dynamic_quads_match_the_fixed_pipeline_in_the_scene_pass_in_every_target_set() {
    let textures = textures();
    each_config(|context, config, built| {
        for case in cases() {
            let drawn = draw_both(context, (config, built), (&textures, &case.quads), false);
            for (index, (ours, reference)) in drawn.ours.iter().zip(&drawn.reference).enumerate() {
                assert!(
                    ours == reference,
                    "{} {}: attachment {index} differs from the fixed pipeline's unclipped scene pass",
                    config.name,
                    case.name
                );
            }
            check_unwritten(&drawn, (config, &case), true);
            print(context, (config, "scene", case.name), &drawn.ours);
        }
    });
}

#[test]
fn clipped_overlay_quads_match_the_fixed_pipeline_scissored_in_every_target_set() {
    let textures = textures();
    each_config(|context, config, built| {
        for case in cases() {
            let drawn = draw_both(context, (config, built), (&textures, &case.quads), true);
            for (index, (ours, reference)) in drawn.ours.iter().zip(&drawn.reference).enumerate() {
                assert!(
                    ours == reference,
                    "{} {}: attachment {index} differs from the fixed pipeline's scissored draw",
                    config.name,
                    case.name
                );
            }
            assert_eq!(
                drawn.counts.0, drawn.counts.1,
                "{} {}: draw counts differ",
                config.name, case.name
            );
            check_unwritten(&drawn, (config, &case), case.visible);
            assert_eq!(
                drawn.ours[0] != drawn.unclipped[0],
                case.clips,
                "{} {}: the scissor {} the output",
                config.name,
                case.name,
                if case.clips {
                    "did not clip"
                } else {
                    "clipped"
                },
            );
            print(context, (config, "overlay", case.name), &drawn.ours);
        }
    });
}
