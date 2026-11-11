//! Byte-equality captures of the water body draw: the bodies of one `WaterFeature` are recorded
//! through `WaterFeature::draw` and through the frozen copy in `fixed`, into the same targets,
//! and the scene colour, the three receivers and the depth are compared byte for byte.
//!
//! Every capture starts from a cleared scene colour, cleared receivers and a cleared depth, and
//! the live capture has to differ from a capture that draws nothing, so a path that draws
//! nothing cannot pass. The snapshot is a checker, so refraction reads something that varies.
//!
//! Both paths share the feature's bind groups, buffers and vertex counts, and the live text
//! specialisers and override constants. The captures establish that the two paths write the
//! same bytes from the same inputs. They do not independently establish that the uniforms or
//! the specialised text are correct; the equation tests beside this file do that.
//!
//! Covered: one sample and, where the adapter has it, four; no shadow group;
//! `DepthMode::Independent`; the default specular convention. At four samples the scene colour
//! and the receivers are compared sample by sample. The depth is compared as the snapshot's
//! depth copy reads it, which is the nearest sample of each pixel, so a depth difference that
//! leaves every pixel's nearest sample alone is not seen. Not covered here: a shadowed feature
//! (and with it the weighted filter, which only changes shadowed text), `DepthMode::Shared`, a
//! non-default specular convention, and wind.

use super::*;
use worldrunner_render_plan::{SceneWaterSeedCoordinates, SceneWaterSeedSampling};

/// What every capture's scene colour is cleared to.
const CLEARED: wgpu::Color = wgpu::Color {
    r: 0.5,
    g: 0.125,
    b: 0.0625,
    a: 1.0,
};

/// What is read of each sample, in the order [`Captured::samples`] holds it.
const PER_SAMPLE: [&str; 4] = ["scene colour", "receiver 0", "receiver 1", "receiver 2"];

struct Case {
    name: &'static str,
    body: SceneWaterBody,
    /// The term mask and seed key the body must reach, which select the pipeline.
    options: u32,
}

fn cases() -> Vec<Case> {
    let mut terms = settings();
    terms.color_micro = [250_000, 500_000, 750_000];
    terms.material.direct_surface = true;
    terms.material.lighting_normal_micro = Some(0);
    terms.material.metallic_micro = Some(100_000);
    let seeded = |mode, sampling| {
        let mut body = seed_tests::body(mode);
        body.material.flow_seed_sampling = sampling;
        body
    };
    vec![
        Case {
            name: "default",
            body: settings(),
            options: 0,
        },
        // Lighting normal 1, metallic 2 and direct surface 16: override constants only.
        Case {
            name: "terms",
            body: terms,
            options: 19,
        },
        // Flow coordinates 64 and vertex seeds 512: seed text and a vertex-stage constant.
        Case {
            name: "vertex seeds",
            body: seeded(
                SceneWaterSeedCoordinates::Repeat,
                SceneWaterSeedSampling::Vertex,
            ),
            options: 64 | 512,
        },
        // Flow coordinates 64 and endpoint addressing 256: endpoint text and a second vertex
        // buffer.
        Case {
            name: "endpoint",
            body: seeded(
                SceneWaterSeedCoordinates::EndpointClamped,
                SceneWaterSeedSampling::Fragment,
            ),
            options: 64 | 256,
        },
        Case {
            name: "endpoint vertex seeds",
            body: seeded(
                SceneWaterSeedCoordinates::EndpointClamped,
                SceneWaterSeedSampling::Vertex,
            ),
            options: 64 | 256 | 512,
        },
    ]
}

/// A lit frame, so the direct and ambient terms are not all zero.
fn frame() -> FrameUniform {
    let mut frame = FrameUniform::new(&fixture_view(), None).unwrap();
    frame.unlit = false;
    frame.sun_direction = glam::Vec3::new(0.0, 0.6, 0.8);
    frame.sun_irradiance = glam::Vec3::ONE;
    frame.ambient_sky = glam::Vec3::ONE;
    frame.ambient_ground = glam::Vec3::ONE;
    frame
}

/// The sample counts the captures run at: one, and four where the adapter has it.
/// `supported_samples` logs a count the adapter lacks.
fn sample_counts(context: &GpuContext) -> Vec<u32> {
    crate::msaa_tests::supported_samples(context)
        .into_iter()
        .filter(|samples| matches!(samples, 1 | 4))
        .collect()
}

/// A renderer of `samples` samples with the case's body installed, one frame encoded in slot 1
/// so the body's uniforms are uploaded, and a checker in that slot's snapshot.
fn prepared(context: &GpuContext, case: &Case, samples: u32) -> crate::compositor::FrameRenderer {
    let mut renderer = regressions::renderer(context, samples);
    renderer
        .install_water(context.device(), case.body.clone())
        .unwrap();
    if case.body.material.flow.is_some() {
        seed_tests::install_field(context, renderer.water.as_mut().unwrap());
    }
    let color = AttachmentView::new(
        ResourceRef::new("scene color"),
        renderer.targets().scene().texture(),
    )
    .unwrap();
    let depth = AttachmentView::new(
        ResourceRef::new("scene depth"),
        renderer.targets().depth().texture(),
    )
    .unwrap();
    renderer.select_frame_slot(1);
    encode_fixture(context, &mut renderer, (&color, &depth), &frame(), 0.25)
        .finish()
        .submit()
        .unwrap();
    wait(context.device());
    optical_render_tests::upload_checker_snapshot(context, &renderer);
    renderer
}

#[derive(Clone, Copy, PartialEq)]
enum Recorder {
    Nothing,
    Live,
    Frozen,
}

/// What a capture leaves in the targets.
struct Captured {
    /// The scene colour after the resolve.
    resolved: Vec<u8>,
    /// The depth as the snapshot's depth copy reads it: the nearest sample of each pixel.
    depth: Vec<u8>,
    /// For each sample, what [`PER_SAMPLE`] names.
    samples: Vec<[Vec<u8>; 4]>,
}

impl Captured {
    /// Every image with its name, in one order for any two captures of one renderer.
    fn images(&self) -> Vec<(String, &[u8])> {
        let mut images = vec![
            ("resolved scene colour".to_owned(), self.resolved.as_slice()),
            ("depth".to_owned(), self.depth.as_slice()),
        ];
        for (sample, read) in self.samples.iter().enumerate() {
            for (name, bytes) in PER_SAMPLE.iter().zip(read) {
                images.push((format!("{name}, sample {sample}"), bytes.as_slice()));
            }
        }
        images
    }
}

/// The targets after `recorder` drew slot 1's feature.
fn capture(
    context: &GpuContext,
    renderer: &crate::compositor::FrameRenderer,
    recorder: Recorder,
) -> Captured {
    let mut drawn = None;
    draw_water_with(
        context,
        renderer,
        (1, wgpu::LoadOp::Clear(CLEARED)),
        |feature, pass| match recorder {
            Recorder::Nothing => {}
            Recorder::Live => feature.draw(
                &worldrunner_render_post::DrawContext {
                    slot: PassSlot::TransparentWater,
                },
                pass,
            ),
            Recorder::Frozen => {
                let pipelines = fixed::pipelines(context.device(), feature);
                drawn = Some(fixed::record(feature, &pipelines, pass));
            }
        },
    );
    if recorder == Recorder::Frozen {
        assert_eq!(drawn, Some(1), "the fixed path draws the one body");
    }
    let water = renderer.water.as_ref().unwrap();
    let receivers = &water.receivers[1];
    let sample_read = lighting_tests::environment_tests::sample_read;
    let samples = (0..water.samples)
        .map(|sample| {
            [
                sample_read(context, renderer.targets().scene().texture(), sample),
                sample_read(context, &receivers[0], sample),
                sample_read(context, &receivers[1], sample),
                sample_read(context, &receivers[2], sample),
            ]
        })
        .collect();
    Captured {
        resolved: read(context, renderer.targets().texture()),
        depth: surface_depth(context, renderer),
        samples,
    }
}

/// Compares the live draw of `case` at `samples` samples with the frozen one and with a draw of
/// nothing, and returns how many images were equal.
fn compare(context: &GpuContext, case: &Case, samples: u32) -> usize {
    let name = format!("{} at {samples} samples", case.name);
    let renderer = prepared(context, case, samples);
    let settings = &renderer.water.as_ref().unwrap().settings;
    assert_eq!(
        terms::mask(settings) | crate::water::vertex_seeds::key(&settings[0]),
        case.options,
        "{name}: the body does not reach the pipeline key this case is for"
    );
    let nothing = capture(context, &renderer, Recorder::Nothing);
    let live = capture(context, &renderer, Recorder::Live);
    let frozen = capture(context, &renderer, Recorder::Frozen);
    assert_eq!(
        live.samples.len(),
        usize::try_from(samples).unwrap(),
        "{name}: every sample is read"
    );
    let (live_images, frozen_images) = (live.images(), frozen.images());
    for ((image, live), (_, frozen)) in live_images.iter().zip(&frozen_images) {
        assert!(
            live == frozen,
            "{name}: {image} differs from the fixed path's output (if \
             the_live_water_wgsl_equals_the_frozen_snapshot fails too, copy the live WGSL \
             into water/fixtures)"
        );
    }
    assert!(
        live.resolved != nothing.resolved,
        "{name}: the water left the scene colour as it was cleared"
    );
    assert!(
        live.depth != nothing.depth,
        "{name}: the water left the depth as it was cleared"
    );
    for (sample, (drawn, cleared)) in live.samples.iter().zip(&nothing.samples).enumerate() {
        assert!(
            drawn[0] != cleared[0],
            "{name}: the water left sample {sample} of the scene colour as it was cleared"
        );
        assert!(
            (1..4).any(|index| drawn[index] != cleared[index]),
            "{name}: the water wrote no receiver in sample {sample}"
        );
    }
    live_images.len()
}

#[test]
fn the_body_draw_matches_the_frozen_fixed_path_byte_for_byte() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = GpuContext::shared_for_test().unwrap();
    let adapter = context.adapter().get_info().name;
    let counts = sample_counts(&context);
    assert!(
        counts.contains(&1),
        "one sample is always captured: {counts:?}"
    );
    if !counts.contains(&4) {
        println!("WATER-CAPTURE adapter={adapter:?} samples=4 skipped: the adapter lacks it");
    }
    for case in cases() {
        for &samples in &counts {
            let equal = compare(&context, &case, samples);
            println!(
                "WATER-CAPTURE adapter={adapter:?} case={} options={} samples={samples} images={equal} equal={equal}",
                case.name, case.options
            );
        }
    }
}
