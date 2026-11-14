//! Object term replays: the six terms a replay writes, in the plain, shadowed, ambient and
//! reflection contexts, drawn through `TermCapture::replay` and through the frozen fixed path
//! into the capture's own colour and depth pass. The term texture is read back and compared byte
//! for byte, as the terrain term captures do.
//!
//! The scene is the prepass captures' frame: an opaque mesh, a relief impostor, a masked
//! double-sided card and a foliage card under wind, three pipeline keys in one frame.

use wgpu::BindGroup;

use super::{Legacy, Rig, digest, keys_used, prepass, scenes};
use crate::compositor::terms::{CaptureTerm, TermCapture, pipelines_for, sources, term_sources};
use crate::draw::SceneGpu;
use crate::dynamic::capture_tests::{
    SIZE, ambient_group, context, copy_results, read_results, reflection_group, shadow_fixture,
};
use crate::renderer::legacy_objects::{Recipe, compose};

const TERMS: [(CaptureTerm, &str); 6] = [
    (CaptureTerm::Albedo, "albedo"),
    (CaptureTerm::SunVisibility, "sun-visibility"),
    (CaptureTerm::DirectDiffuse, "direct-diffuse"),
    (CaptureTerm::AmbientDiffuse, "ambient-diffuse"),
    (CaptureTerm::EnvironmentSpecular, "environment-specular"),
    (CaptureTerm::SunSpecular, "sun-specular"),
];

/// What a replay binds besides the frame: a shadow map, a screen group or neither.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Replay {
    Plain,
    Shadowed,
    Ambient,
    Reflection,
}

impl Replay {
    fn shadowed(self) -> bool {
        self == Self::Shadowed
    }

    /// The screen argument of `pipelines_for`: none, the ambient group, or reflections.
    fn screen(self) -> Option<bool> {
        match self {
            Self::Ambient => Some(false),
            Self::Reflection => Some(true),
            Self::Plain | Self::Shadowed => None,
        }
    }
}

const REPLAYS: [(Replay, &str); 4] = [
    (Replay::Plain, "plain"),
    (Replay::Shadowed, "shadowed"),
    (Replay::Ambient, "ambient"),
    (Replay::Reflection, "reflection"),
];

/// The object text of `term` in `replay`, composed as `pipelines_for` composes it but over the
/// frozen forward source. The term rewrite and the ambient and reflection rewriters are the live
/// ones, as the terrain term captures use them. While the live forward source equals the
/// snapshot, the composition must equal the live one.
fn frozen_source(term: CaptureTerm, replay: Replay) -> String {
    let base = compose(Recipe {
        shadowed: replay.shadowed(),
        ..Recipe::default()
    });
    let live = crate::shadows::capture_sources(replay.shadowed());
    let screen = replay.screen();
    let compose = |stock: (&str, &str)| {
        sources(
            term_sources(stock, term),
            screen.is_some(),
            screen == Some(true),
        )
        .0
    };
    let source = compose((&base, live.1));
    if live.0 == base {
        assert!(
            source == compose(live),
            "{replay:?} {term:?}: the frozen replay composition differs from the live one"
        );
    }
    source
}

/// Reads back the term texture of a capture that `record` draws.
fn read(
    rig: &mut Rig<'_>,
    term: CaptureTerm,
    record: impl FnOnce(&mut wgpu::CommandEncoder, &TermCapture, &SceneGpu),
) -> Vec<u8> {
    let context = rig.context;
    let device = context.device();
    let capture = TermCapture::new(device, [SIZE; 2], term);
    rig.prime();
    let mut encoder = context.submission().begin(device).unwrap();
    rig.gpu
        .flush_instances_encoded(device, &mut encoder)
        .unwrap();
    record(&mut encoder, &capture, &rig.gpu);
    let staging = copy_results(device, &mut encoder, std::slice::from_ref(&capture.texture));
    let index = encoder.finish().submit().unwrap();
    read_results(device, index, staging).remove(0)
}

/// Compares every term in every replay context and returns how many captures ran.
fn replays() -> usize {
    let context = context();
    let device = context.device();
    let shadow = shadow_fixture::groups(&context);
    let shadow_layout = crate::shadows::water_layout(device);
    let screens: [Option<BindGroup>; 4] = [
        None,
        None,
        Some(ambient_group(device)),
        Some(reflection_group(
            device,
            &crate::ssr::forward::layout(device),
        )),
    ];
    let scene = scenes::scene(&prepass::specs());
    let adapter = context.adapter().get_info().name;
    let mut run = 0;
    for ((replay, name), screen) in REPLAYS.into_iter().zip(&screens) {
        let layout = replay.shadowed().then_some(&shadow_layout);
        let group = layout.map(|_| &shadow[0]);
        for (term, label) in TERMS {
            let config = format!("term-{name}-{label}");
            let pipelines = pipelines_for(device, term, layout, replay.screen());
            let mut rig = Rig::moving(&context, &pipelines, &scene, false);
            let legacy = Legacy::new(
                &context,
                &rig.selected(&pipelines),
                &frozen_source(term, replay),
            );
            let inputs = (group, screen.as_ref());
            let before = pipelines.object_runtime_stats().pipelines_built;
            let drawn = read(&mut rig, term, |encoder, capture, gpu| {
                capture.replay(encoder, gpu, &pipelines, inputs);
            });
            assert!(
                pipelines.object_runtime_stats().pipelines_built > before,
                "{config}: the replay built no object pipeline on the program runtime"
            );
            let mut calls = 0;
            let fixed = read(&mut rig, term, |encoder, capture, gpu| {
                capture.record(encoder, inputs, |pass| {
                    calls = legacy.draw(pass, gpu, 0).calls;
                });
            });
            let untouched = read(&mut rig, term, |encoder, capture, _| {
                capture.record(encoder, inputs, |_| {});
            });
            assert!(
                drawn == fixed,
                "{config}: the term differs from the fixed pipeline's output (if \
                 the_live_object_wgsl_equals_the_frozen_snapshot fails too, copy the live WGSL \
                 into object/fixtures)"
            );
            assert!(calls > 0, "{config}: the fixed path drew nothing");
            // Every term of this scene writes something, so equal bytes are never two blanks.
            assert!(
                drawn != untouched,
                "{config}: the objects drew nothing into the term"
            );
            let keys = keys_used(&rig.gpu, 0);
            assert_eq!(
                keys.len(),
                3,
                "{config}: the opaque mesh, the relief impostor and the masked double-sided cards \
                 each draw with their own key, got {keys:?}"
            );
            println!(
                "OBJECT-CAPTURE adapter={adapter:?} config={config} scene=terms mask=0 attachment=0 bytes={} hash={:016x}",
                drawn.len(),
                digest(&drawn)
            );
            run += 1;
        }
    }
    run
}

#[test]
fn object_terms_match_the_fixed_pipeline_byte_for_byte_in_every_replay() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let run = replays();
    assert_eq!(run, TERMS.len() * REPLAYS.len());
    println!("OBJECT-CAPTURE-SUMMARY set=terms compared={run} skipped=0");
}
