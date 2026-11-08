//! Opaque term captures: the six terms a replay writes, in the plain, shadowed, ambient and
//! reflection contexts, drawn into the capture's own colour and depth pass through the scene and
//! through the frozen copy. The term texture is read back and compared byte for byte.

use wgpu::BindGroup;

use super::shadow::Shadow;
use super::{Legacy, Rig, Source, assert_recorded_by_runtime, drawable, scenes};
use crate::compositor::terms::{CaptureTerm, TermCapture, pipelines_for};
use crate::device::GpuContext;
use crate::draw::{DrawCount, terrain_paths};
use crate::dynamic::capture_tests::{
    SIZE, ambient_group, context, copy_results, digest, read_results, reflection_group,
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

const REPLAYS: [(Replay, &str); 4] = [
    (Replay::Plain, "plain"),
    (Replay::Shadowed, "shadowed"),
    (Replay::Ambient, "ambient"),
    (Replay::Reflection, "reflection"),
];

/// Reads back the term texture of a capture recorded by `record`.
fn read(
    context: &GpuContext,
    rig: &Rig<'_>,
    record: impl FnOnce(&mut wgpu::CommandEncoder, &TermCapture),
    term: CaptureTerm,
) -> Vec<u8> {
    let device = context.device();
    let capture = TermCapture::new(device, [SIZE; 2], term);
    let mut encoder = context.submission().begin(device).unwrap();
    rig.prepare(&mut encoder);
    record(&mut encoder, &capture);
    let staging = copy_results(device, &mut encoder, std::slice::from_ref(&capture.texture));
    let index = encoder.finish().submit().unwrap();
    read_results(device, index, staging).remove(0)
}

/// The replay contexts a surface source is captured in. Plain terrain crosses all four. The
/// other sources add what changes their draw: the shadow group for pages, nothing for layered
/// sources, whose terms differ from plain terrain only in the surface they sample.
fn replays(source: Source) -> &'static [Replay] {
    match source {
        Source::Plain => &[
            Replay::Plain,
            Replay::Shadowed,
            Replay::Ambient,
            Replay::Reflection,
        ],
        Source::Pages => &[Replay::Plain, Replay::Shadowed],
        Source::Layered | Source::LayeredPages => &[Replay::Plain],
    }
}

/// Compares every term in the replay contexts of one surface source, returning how many
/// captures ran.
fn terms(source: Source) -> usize {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    if !drawable(&context, source) {
        return 0;
    }
    let device = context.device();
    let shadow = Shadow::new(&context);
    let shadow_layout = crate::shadows::water_layout(device);
    let screen_groups: [(Replay, Option<BindGroup>); 4] = [
        (Replay::Plain, None),
        (Replay::Shadowed, None),
        (Replay::Ambient, Some(ambient_group(device))),
        (
            Replay::Reflection,
            Some(reflection_group(
                device,
                &crate::ssr::forward::layout(device),
            )),
        ),
    ];
    let frame = scenes::frame(scenes::pose());
    let seed = pipelines_for(device, CaptureTerm::Albedo, None, None);
    let rig = Rig::new(&context, &seed, &source.scene(), (source, frame));
    let adapter = context.adapter().get_info().name;
    let mut run = 0;
    for ((replay, name), (_, screen)) in REPLAYS.into_iter().zip(&screen_groups) {
        if !replays(source).contains(&replay) {
            continue;
        }
        for (term, label) in TERMS {
            let layout = (replay == Replay::Shadowed).then_some(&shadow_layout);
            let screen_kind = match replay {
                Replay::Ambient => Some(false),
                Replay::Reflection => Some(true),
                _ => None,
            };
            let pipelines = pipelines_for(device, term, layout, screen_kind);
            let group = layout.map(|_| &shadow.groups[0]);
            let legacy = Legacy::new(&context, &rig.gpu, &rig.selected(&pipelines));
            let recorded = terrain_paths::recorded();
            let scene = read(
                &context,
                &rig,
                |encoder, capture| {
                    capture.replay(encoder, &rig.gpu, &pipelines, (group, screen.as_ref()));
                },
                term,
            );
            assert_recorded_by_runtime(
                &format!("{name} {label} {}", source.name()),
                &pipelines,
                recorded,
            );
            let mut drawn = DrawCount::default();
            let fixed = read(
                &context,
                &rig,
                |encoder, capture| {
                    capture.record(encoder, (group, screen.as_ref()), |pass| {
                        pass.set_bind_group(0, &rig.gpu.legacy_frame_group(), &[]);
                        drawn = legacy.draw(pass, &rig.gpu, group);
                    });
                },
                term,
            );
            let untouched = read(
                &context,
                &rig,
                |encoder, capture| {
                    capture.record(encoder, (group, screen.as_ref()), |_| {});
                },
                term,
            );
            assert!(
                scene == fixed,
                "{name} {label} {}: the term differs from the fixed pipeline's output",
                source.name()
            );
            assert!(drawn.calls > 0, "{name} {label}: no terrain was drawn");
            if term == CaptureTerm::Albedo {
                assert!(
                    scene != untouched,
                    "{name} {label} {}: terrain drew nothing into the term",
                    source.name()
                );
            }
            println!(
                "TERRAIN-CAPTURE adapter={adapter:?} config=term-{name}-{label} depth=independent source={} attachment=0 bytes={} hash={:016x}",
                source.name(),
                scene.len(),
                digest(&scene)
            );
            run += 1;
        }
    }
    println!(
        "TERRAIN-CAPTURE-SUMMARY set=terms source={} compared={run}",
        source.name()
    );
    run
}

macro_rules! term_tests {
    ($($test:ident: $source:expr;)*) => {$(
        #[test]
        fn $test() {
            let run = terms($source);
            if matches!($source, Source::Plain | Source::Pages) {
                assert_eq!(run, TERMS.len() * replays($source).len());
            }
        }
    )*};
}

term_tests! {
    plain_terrain_terms_match_the_fixed_pipeline_byte_for_byte_in_every_replay: Source::Plain;
    page_terrain_terms_match_the_fixed_pipeline_byte_for_byte_in_every_replay: Source::Pages;
    layered_terrain_terms_match_the_fixed_pipeline_byte_for_byte_in_every_replay: Source::Layered;
    layered_page_terrain_terms_match_the_fixed_pipeline_byte_for_byte_in_every_replay:
        Source::LayeredPages;
}
