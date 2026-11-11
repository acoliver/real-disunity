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
//! Covered: one sample, no shadow group, `DepthMode::Independent`, the default specular
//! convention. Not covered here: a shadowed feature (and with it the weighted filter, which
//! only changes shadowed text), `DepthMode::Shared`, more than one sample, a non-default
//! specular convention, and wind.

use super::*;
use worldrunner_render_plan::{SceneWaterSeedCoordinates, SceneWaterSeedSampling};

/// What every capture's scene colour is cleared to.
const CLEARED: wgpu::Color = wgpu::Color {
    r: 0.5,
    g: 0.125,
    b: 0.0625,
    a: 1.0,
};

const ATTACHMENTS: [&str; 5] = [
    "scene colour",
    "receiver 0",
    "receiver 1",
    "receiver 2",
    "depth",
];

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

/// A renderer with the case's body installed, one frame encoded in slot 1 so the body's
/// uniforms are uploaded, and a checker in that slot's snapshot.
fn prepared(context: &GpuContext, case: &Case) -> crate::compositor::FrameRenderer {
    let mut renderer = regressions::renderer(context, 1);
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

/// The scene colour, the three receivers and the depth after `recorder` drew slot 1's feature.
fn capture(
    context: &GpuContext,
    renderer: &crate::compositor::FrameRenderer,
    recorder: Recorder,
) -> [Vec<u8>; 5] {
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
    let receivers = &renderer.water.as_ref().unwrap().receivers[1];
    [
        read(context, renderer.targets().texture()),
        read(context, &receivers[0]),
        read(context, &receivers[1]),
        read(context, &receivers[2]),
        surface_depth(context, renderer),
    ]
}

#[test]
fn the_body_draw_matches_the_frozen_fixed_path_byte_for_byte() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = GpuContext::shared_for_test().unwrap();
    let adapter = context.adapter().get_info().name;
    for case in cases() {
        let name = case.name;
        let renderer = prepared(&context, &case);
        let settings = &renderer.water.as_ref().unwrap().settings;
        assert_eq!(
            terms::mask(settings) | crate::water::vertex_seeds::key(&settings[0]),
            case.options,
            "{name}: the body does not reach the pipeline key this case is for"
        );
        let nothing = capture(&context, &renderer, Recorder::Nothing);
        let live = capture(&context, &renderer, Recorder::Live);
        let frozen = capture(&context, &renderer, Recorder::Frozen);
        for (index, attachment) in ATTACHMENTS.iter().enumerate() {
            assert!(
                live[index] == frozen[index],
                "{name}: {attachment} differs from the fixed path's output (if \
                 the_live_water_wgsl_equals_the_frozen_snapshot fails too, copy the live WGSL \
                 into water/fixtures)"
            );
        }
        assert!(
            live[0] != nothing[0],
            "{name}: the water left the scene colour as it was cleared"
        );
        assert!(
            live[4] != nothing[4],
            "{name}: the water left the depth as it was cleared"
        );
        assert!(
            (1..4).any(|index| live[index] != nothing[index]),
            "{name}: the water wrote no receiver"
        );
        println!(
            "WATER-CAPTURE adapter={adapter:?} case={name} options={} attachments=5 equal=5",
            case.options
        );
    }
}
