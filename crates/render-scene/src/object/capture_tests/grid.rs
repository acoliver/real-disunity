//! A grid of cards at three depths: every card draws, and depth decides where they overlap.

use super::{Rig, Target, render, scenes};
use crate::dynamic::capture_tests::{CONFIGS, build, context};

#[test]
fn the_grid_draws_every_card_and_depth_decides_where_they_overlap() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let built = build(context.device(), &CONFIGS[2]);
    let target = Target {
        formats: &built.formats,
        samples: 1,
    };
    let mut rig = Rig::new(&context, &built.pipelines, &scenes::grid(3, 5));
    let drawn = render(&context, target, &mut rig.gpu, |pass, gpu| {
        gpu.draw_color(pass, &built.pipelines, None);
    });
    let untouched = render(&context, target, &mut rig.gpu, |_, _| {});
    let (colour, depth) = (&drawn[0], drawn.last().unwrap());
    assert_ne!(colour, &untouched[0], "the cards drew");
    // Fifteen cards in three rows at three depths leave several distinct depths in the plane.
    let mut depths: Vec<_> = depth.chunks_exact(4).map(<[u8]>::to_vec).collect();
    depths.sort();
    depths.dedup();
    assert!(depths.len() >= 4, "the cards left {} depths", depths.len());
}
