//! The terrain-draw switch: with the stock draws suppressed no pass that draws terrain records a
//! call or changes a texel, in the colour pass, the metadata prepass and the shadow casters, and
//! the same scene draws it again once the stock draws return.

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

fn both(rig: &Rig<'_>, mut pass: impl FnMut() -> Outcome) -> [Outcome; 2] {
    [TerrainDraws::Stock, TerrainDraws::Suppressed].map(|draws| {
        rig.gpu.set_terrain_draws(draws);
        pass()
    })
}

#[test]
fn suppressed_terrain_is_recorded_by_no_colour_prepass_or_caster_pass() {
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
    let outcomes = both(&rig, || {
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
    let outcomes = both(&rig, || Outcome {
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

    let built = build(context.device(), &CONFIGS[2]);
    let rig = Rig::new(
        &context,
        &built.pipelines,
        &scenes::plain(),
        (Source::Plain, frame),
    );
    let casting = Casting::new(&context, &built.pipelines, &frame, BIAS);
    let pipeline = casting.caster.pipeline().expect("the caster prepares");
    let untouched = vec![render_map(&context, |_| {})];
    let outcomes = both(&rig, || {
        let mut count = DrawCount::default();
        let map = render_map(&context, |pass| {
            count = rig.gpu.draw_terrain_depth(
                pass,
                (&pipeline, &casting.frames[3].1),
                (3, casting.matrices[3]),
            );
        });
        Outcome {
            drawn: vec![map],
            calls: Some(count.calls),
        }
    });
    assert_switch("shadow caster", outcomes, &untouched);
}
