//! The water-draw switch through a configured renderer: with the stock draws suppressed a frame
//! plans no water pass, no reactive mask pass and no water inputs for the reflections, so it is
//! the frame of the same scene without water. The default and the explicit `Stock` choice are
//! the frame of a renderer built without the setting, and switching between frames leaves
//! nothing of the other choice behind.
use super::*;
use crate::compositor::WaterDraws;

/// What one frame submitted and drew.
#[derive(Debug, PartialEq)]
struct Frame {
    passes: Vec<String>,
    colour: Vec<u8>,
}

/// A rig for `source` built with `draws`, through the public configuration.
fn rig_with(context: &GpuContext, source: RenderScene, draws: Option<WaterDraws>) -> Rig<'_> {
    let mut settings = configuration(&source, SIZE);
    if let Some(draws) = draws {
        settings.water_draws = draws;
    }
    let renderer =
        ConfiguredFrameRenderer::new(context, settings, AllocationBudget(64 << 20)).unwrap();
    let pipelines =
        ScenePipelines::with_samples(context.device(), TextureFormat::Rgba16Float, 1).unwrap();
    let gpu =
        SceneGpu::upload(context.device(), context.submission(), &source, &pipelines).unwrap();
    Rig {
        context,
        source,
        renderer,
        gpu,
        pipelines,
        size: SIZE,
    }
}

impl Rig<'_> {
    /// One frame, the passes it submitted and the scene colour it left.
    fn frame(&mut self, slot: usize, pose: usize) -> Frame {
        plan_trace::take();
        self.draw(slot, viewpoint(pose));
        let passes = plan_trace::take()
            .into_iter()
            .flat_map(|plan| plan.passes)
            .collect();
        let caller = self.own_caller();
        Frame {
            passes,
            colour: self.read(&caller.color),
        }
    }

    fn water_calls(&self) -> u64 {
        self.gpu.statistics().passes.water.calls
    }

    fn force(&mut self, draws: WaterDraws) {
        self.renderer.renderer_mut().force_water_draws(draws);
    }
}

fn names_water(frame: &Frame) -> bool {
    frame
        .passes
        .iter()
        .any(|pass| pass.starts_with("TransparentWater"))
}

const REQUESTS: [Option<fn() -> SceneSsr>; 2] = [None, Some(reflections)];

#[test]
fn gpu_suppressed_water_frames_plan_and_render_as_the_scene_without_water() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    for request in REQUESTS {
        let request = request.map(|make| make());
        let label = format!("reflections {}", request.is_some());
        let mut stock = rig_with(&context, scene_with(request, true), None);
        let drawn = stock.frame(0, 0);
        assert!(names_water(&drawn), "{label}: the stock frame draws water");
        assert!(
            stock.water_calls() > 0,
            "{label}: stock water records draws"
        );

        let mut suppressed = rig_with(
            &context,
            scene_with(request, true),
            Some(WaterDraws::Suppressed),
        );
        let frame = suppressed.frame(0, 0);
        let mut dry = rig_with(&context, scene_with(request, false), None);
        let control = dry.frame(0, 0);

        assert!(!names_water(&frame), "{label}: {:?}", frame.passes);
        assert_eq!(suppressed.water_calls(), 0, "{label}: water draws");
        assert!(drawn.colour != control.colour, "{label}: water is visible");
        assert_eq!(frame.passes, control.passes, "{label}: the plan");
        assert!(frame.colour == control.colour, "{label}: the frame");
    }
}

#[test]
fn gpu_stock_water_draws_are_the_frame_of_a_renderer_without_the_setting() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    for request in REQUESTS {
        let request = request.map(|make| make());
        let mut reference = rig_with(&context, scene_with(request, true), None);
        let mut explicit = rig_with(&context, scene_with(request, true), Some(WaterDraws::Stock));
        for (slot, pose) in [(0, 0), (1, 1), (2, 2), (0, 3)] {
            let expected = reference.frame(slot, pose);
            let actual = explicit.frame(slot, pose);
            assert!(names_water(&actual));
            assert_eq!(actual.passes, expected.passes, "slot {slot}");
            assert!(actual.colour == expected.colour, "slot {slot}: the frame");
        }
    }
}

#[test]
fn gpu_switching_the_water_draws_between_frames_leaves_the_stock_frame_unchanged() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    for request in REQUESTS {
        let request = request.map(|make| make());
        let label = format!("reflections {}", request.is_some());
        let mut fresh = rig_with(&context, scene_with(request, true), None);
        let expected = fresh.frame(2, 2);

        let mut switched = rig_with(&context, scene_with(request, true), None);
        switched.frame(0, 0);
        switched.force(WaterDraws::Suppressed);
        let hidden = switched.frame(1, 1);
        assert!(!names_water(&hidden), "{label}: suppressed frame");
        switched.force(WaterDraws::Stock);
        let again = switched.frame(2, 2);
        assert!(names_water(&again), "{label}: water returns");
        assert_eq!(again.passes, expected.passes, "{label}: the plan");
        assert!(again.colour == expected.colour, "{label}: the frame");
        assert!(switched.water_calls() > 0, "{label}: draws recorded");
    }
}

#[test]
fn gpu_reconfiguring_the_water_draws_replaces_the_frame_in_both_directions() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let request = Some(reflections());
    let mut fresh = rig_with(&context, scene_with(request, true), None);
    let expected = fresh.frame(1, 1);

    let mut rig = rig_with(&context, scene_with(request, true), None);
    rig.frame(0, 0);
    let mut settings = rig.renderer.configuration().clone();
    settings.water_draws = WaterDraws::Suppressed;
    rig.renderer
        .reconfigure(&context, settings.clone())
        .unwrap();
    let hidden = rig.frame(1, 1);
    assert!(!names_water(&hidden));
    assert_eq!(rig.water_calls(), 0);

    settings.water_draws = WaterDraws::Stock;
    rig.renderer.reconfigure(&context, settings).unwrap();
    let again = rig.frame(1, 1);
    assert_eq!(again.passes, expected.passes);
    assert!(again.colour == expected.colour, "the frame");
}
