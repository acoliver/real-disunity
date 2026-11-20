//! When the stock frame plans and writes the post-water depth. The stock water and the stock
//! reflections ask for it; suppressed water, which no stock pass draws, asks for nothing, so a
//! frame plans the pass only when something else reads it.
use super::water_draws::{Frame, rig_configured};
use super::*;
use crate::compositor::WaterDraws;

const LINEAR: &str = "AfterWaterDepth#0 post-water depth:C / ";

/// The first frame of a scene with or without water and reflections, drawn with `draws`.
fn planned_frame(request: Option<SceneSsr>, water: bool, draws: WaterDraws) -> Frame {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let mut rig = rig_configured(&context, scene_with(request, water), |settings| {
        settings.water_draws = draws;
    });
    rig.frame(0, 0)
}

fn passes_after(frame: &Frame, slot: &str) -> Vec<String> {
    frame
        .passes
        .iter()
        .filter(|pass| pass.starts_with(slot))
        .cloned()
        .collect()
}

#[test]
fn gpu_stock_water_and_stock_reflections_plan_the_post_water_depth_and_nothing_else_does() {
    let cases = [
        ("stock water", None, true, WaterDraws::Stock, true),
        (
            "suppressed water",
            None,
            true,
            WaterDraws::Suppressed,
            false,
        ),
        ("no water", None, false, WaterDraws::Stock, false),
        (
            "suppressed water with reflections",
            Some(reflections()),
            true,
            WaterDraws::Suppressed,
            true,
        ),
        (
            "reflections without water",
            Some(reflections()),
            false,
            WaterDraws::Stock,
            true,
        ),
    ];
    for (label, request, water, draws, planned) in cases {
        let frame = planned_frame(request, water, draws);
        assert_eq!(
            passes_after(&frame, "AfterWaterDepth"),
            if planned {
                vec![LINEAR.to_owned()]
            } else {
                vec![]
            },
            "{label}: {:?}",
            frame.passes
        );
    }
}
