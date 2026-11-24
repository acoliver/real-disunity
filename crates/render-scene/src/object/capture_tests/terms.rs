//! Object term replays: the six terms a replay writes, in the plain, shadowed, ambient and
//! reflection contexts, drawn through `TermCapture::replay`. The term texture is read back and
//! compared with an untouched capture, as the terrain term captures do.
//!
//! The scene is the prepass captures' frame: an opaque mesh, a relief impostor, a masked
//! double-sided card and a foliage card under wind, three pipeline keys in one frame.

use wgpu::BindGroup;

use super::{Rig, digest, prepass, scenes};
use crate::compositor::terms::{CaptureTerm, ReplayGroups, TermCapture, pipelines_for};
use crate::draw::SceneGpu;
use crate::dynamic::capture_tests::{
    SIZE, ambient_group, context, copy_results, read_results, reflection_group, shadow_fixture,
};

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

/// Captures every term in every replay context and returns how many captures ran.
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
            let inputs = ReplayGroups {
                shadow: group,
                screen: screen.as_ref(),
            };
            let before = pipelines.object_runtime_stats().pipelines_built;
            let drawn = read(&mut rig, term, |encoder, capture, gpu| {
                capture.replay(encoder, gpu, &pipelines, inputs);
            });
            assert!(
                pipelines.object_runtime_stats().pipelines_built > before,
                "{config}: the replay built no object pipeline on the program runtime"
            );
            let untouched = read(&mut rig, term, |encoder, capture, _| {
                capture.record(encoder, inputs, |_| {});
            });
            // Every term of this scene writes something, so equal bytes are never two blanks.
            assert!(
                drawn != untouched,
                "{config}: the objects drew nothing into the term"
            );
            let keys = rig.gpu.pipeline_keys_drawn(0);
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
fn object_terms_draw_in_every_replay() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let run = replays();
    assert_eq!(run, TERMS.len() * REPLAYS.len());
    println!("OBJECT-CAPTURE-SUMMARY set=terms captures={run} skipped=0");
}
