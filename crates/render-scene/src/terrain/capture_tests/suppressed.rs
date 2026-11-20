//! The terrain-draw switch: with the stock draws suppressed no pass that draws terrain records a
//! call or changes a texel, in the colour pass, the metadata prepass and the shadow casters, and
//! the same scene draws it again once the stock draws return. With the casters only the colour
//! pass and the metadata prepass stay untouched while the four shadow cascades receive the
//! texels and the calls of the stock draws.

use super::{
    Rig, Source, Target, caster::BIAS, caster::Casting, caster::render_map, prepass::pipelines,
    render, scenes,
};
use crate::draw::{DrawCount, TerrainDraws};
use crate::dynamic::capture_tests::{CONFIGS, build, context};
use crate::renderer::depth::DepthMode;
use crate::scene_metadata::MetadataOutput;

/// What one pass recorded for the terrain and the texels it left, with the stock draws on and
/// off.
struct Outcome {
    drawn: Vec<Vec<u8>>,
    calls: Option<u64>,
}

fn assert_switch(pass: &str, [stock, suppressed]: [Outcome; 2], untouched: &[Vec<u8>]) {
    assert!(
        stock.drawn != untouched,
        "{pass}: the stock draws left every texel untouched, so the switch cannot be seen"
    );
    assert_eq!(
        suppressed.drawn, untouched,
        "{pass}: suppressed terrain changed a texel"
    );
    assert!(
        stock.calls != Some(0),
        "{pass}: the stock draws recorded nothing"
    );
    assert!(
        suppressed.calls.is_none_or(|calls| calls == 0),
        "{pass}: suppressed terrain recorded {:?} calls",
        suppressed.calls
    );
}

fn both(rig: &Rig<'_>, skipped: TerrainDraws, mut pass: impl FnMut() -> Outcome) -> [Outcome; 2] {
    [TerrainDraws::Stock, skipped].map(|draws| {
        rig.gpu.set_terrain_draws(draws);
        pass()
    })
}

/// The colour pass and the metadata prepass with the stock draws and with `skipped`, which must
/// leave both untouched.
fn assert_camera_passes_skip(skipped: TerrainDraws) {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let frame = scenes::frame(scenes::pose());

    let built = build(context.device(), &CONFIGS[0]);
    let target = Target {
        formats: &built.formats,
        samples: 1,
    };
    let rig = Rig::new(
        &context,
        &built.pipelines,
        &scenes::plain(),
        (Source::Plain, frame),
    );
    let untouched = render(&context, target, |_| {}, |_| {});
    let outcomes = both(&rig, skipped, || {
        let mut count = DrawCount::default();
        let drawn = render(
            &context,
            target,
            |encoder| rig.prepare(encoder),
            |pass| {
                count = rig.gpu.draw_color(pass, &built.pipelines, None).terrain;
            },
        );
        Outcome {
            drawn,
            calls: Some(count.calls),
        }
    });
    assert_switch("colour pass", outcomes, &untouched);

    let (prepass, group, formats) = pipelines(&context, &frame, MetadataOutput::Normal);
    let target = Target {
        formats: &formats,
        samples: 1,
    };
    let rig = Rig::new(&context, &prepass, &scenes::plain(), (Source::Plain, frame));
    let untouched = render(&context, target, |_| {}, |_| {});
    let outcomes = both(&rig, skipped, || Outcome {
        drawn: render(
            &context,
            target,
            |encoder| rig.prepare(encoder),
            |pass| {
                rig.gpu
                    .draw_prepass(pass, &prepass.for_depth(DepthMode::Independent), &group);
            },
        ),
        calls: None,
    });
    assert_switch("metadata prepass", outcomes, &untouched);
}

/// What the terrain caster leaves in each of the four cascades, and how many calls it records
/// over them, with `draws` selected.
fn cascades_with(rig: &Rig<'_>, casting: &Casting, draws: TerrainDraws) -> Outcome {
    rig.gpu.set_terrain_draws(draws);
    let pipeline = casting.caster.pipeline().expect("the caster prepares");
    let mut calls = 0;
    let maps = (0..4_u32)
        .map(|index| {
            let slot = index as usize;
            render_map(rig.context, |pass| {
                calls += rig
                    .gpu
                    .draw_terrain_depth(
                        pass,
                        (&pipeline, &casting.frames[slot].1),
                        (index, casting.matrices[slot]),
                    )
                    .calls;
            })
        })
        .collect();
    Outcome {
        drawn: maps,
        calls: Some(calls),
    }
}

/// The terrain caster over the four cascades of the plain scene, under each selection of
/// `draws`, with the maps of a pass that draws nothing.
fn cascade_outcomes<const N: usize>(draws: [TerrainDraws; N]) -> ([Outcome; N], Vec<Vec<u8>>) {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let frame = scenes::frame(scenes::pose());
    let built = build(context.device(), &CONFIGS[2]);
    let rig = Rig::new(
        &context,
        &built.pipelines,
        &scenes::plain(),
        (Source::Plain, frame),
    );
    let casting = Casting::new(&context, &built.pipelines, &frame, BIAS);
    let untouched = (0..4).map(|_| render_map(&context, |_| {})).collect();
    (
        draws.map(|draws| cascades_with(&rig, &casting, draws)),
        untouched,
    )
}

#[test]
fn suppressed_terrain_is_recorded_by_no_colour_prepass_or_caster_pass() {
    assert_camera_passes_skip(TerrainDraws::Suppressed);
    let ([stock, suppressed], untouched) =
        cascade_outcomes([TerrainDraws::Stock, TerrainDraws::Suppressed]);
    assert_switch("shadow caster", [stock, suppressed], &untouched);
}

#[test]
fn casters_only_terrain_is_recorded_by_the_four_caster_cascades_and_no_camera_pass() {
    assert_camera_passes_skip(TerrainDraws::CastersOnly);
    let ([stock, casters], untouched) =
        cascade_outcomes([TerrainDraws::Stock, TerrainDraws::CastersOnly]);
    assert!(
        stock.drawn != untouched,
        "the stock casters left every cascade untouched, so the casters cannot be seen"
    );
    assert!(stock.calls != Some(0), "the stock casters recorded nothing");
    assert_eq!(
        casters.drawn, stock.drawn,
        "the casters only terrain left other texels than the stock casters"
    );
    assert_eq!(casters.calls, stock.calls, "the casters' calls");
}
