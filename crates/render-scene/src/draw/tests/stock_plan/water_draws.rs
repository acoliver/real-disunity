//! A game that draws the water, through a configured renderer: the frame plans no water pass and
//! no water inputs for the reflections, so it is the frame of the same scene without water. A
//! game that declares nothing leaves the frame of a renderer without a game, and rebuilding the
//! renderer without the declaring game leaves nothing of the declaration behind.
use super::*;
use crate::compositor::{StockReplacement, declaring_stock};
use crate::test_support::upload_scene;

/// What one frame submitted and drew.
#[derive(Debug, PartialEq)]
pub(super) struct Frame {
    pub(super) passes: Vec<String>,
    colour: Vec<u8>,
}

/// A rig for `source` built with a game that declares `replaces`, or with no game, through the
/// public configuration.
fn rig_with(
    context: &GpuContext,
    source: RenderScene,
    replaces: Option<StockReplacement>,
) -> Rig<'_> {
    rig_configured(context, source, |settings| {
        if let Some(replaces) = replaces {
            settings.game_passes = declaring_stock(replaces);
        }
    })
}

/// A rig for `source` whose configuration `configure` adjusts.
pub(super) fn rig_configured(
    context: &GpuContext,
    source: RenderScene,
    configure: impl FnOnce(&mut RendererConfiguration),
) -> Rig<'_> {
    let mut settings = configuration(&source, SIZE);
    configure(&mut settings);
    let renderer =
        ConfiguredFrameRenderer::new(context, settings, AllocationBudget(64 << 20)).unwrap();
    let pipelines = ScenePipelines::new(context.device(), TextureFormat::Rgba16Float, 1).unwrap();
    let gpu = upload_scene(context.device(), context.submission(), &source, &pipelines).unwrap();
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
    pub(super) fn frame(&mut self, slot: usize, pose: usize) -> Frame {
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
}

fn names_water(frame: &Frame) -> bool {
    frame
        .passes
        .iter()
        .any(|pass| pass.starts_with("TransparentWater"))
}

const REQUESTS: [Option<fn() -> SceneSsr>; 2] = [None, Some(reflections)];

#[test]
fn gpu_declared_water_frames_plan_and_render_as_the_scene_without_water() {
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

        let mut declared = rig_with(
            &context,
            scene_with(request, true),
            Some(StockReplacement::WATER),
        );
        let frame = declared.frame(0, 0);
        let mut dry = rig_with(&context, scene_with(request, false), None);
        let control = dry.frame(0, 0);

        assert!(!names_water(&frame), "{label}: {:?}", frame.passes);
        assert_eq!(declared.water_calls(), 0, "{label}: water draws");
        assert!(drawn.colour != control.colour, "{label}: water is visible");
        assert_eq!(frame.passes, control.passes, "{label}: the plan");
        assert!(frame.colour == control.colour, "{label}: the frame");
    }
}

#[test]
fn gpu_a_game_that_declares_nothing_leaves_the_stock_frame() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    for request in REQUESTS {
        let request = request.map(|make| make());
        let mut reference = rig_with(&context, scene_with(request, true), None);
        let mut explicit = rig_with(
            &context,
            scene_with(request, true),
            Some(StockReplacement::NONE),
        );
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
fn gpu_rebuilding_without_the_declaring_game_leaves_the_stock_frame_unchanged() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    for request in REQUESTS {
        let request = request.map(|make| make());
        let label = format!("reflections {}", request.is_some());
        let mut fresh = rig_with(&context, scene_with(request, true), None);
        let expected = fresh.frame(1, 1);

        let mut rig = rig_with(&context, scene_with(request, true), None);
        rig.frame(0, 0);
        let stock = rig.renderer.configuration().clone();
        let mut declared = stock.clone();
        declared.game_passes = declaring_stock(StockReplacement::WATER);
        rig.renderer.reconfigure(&context, declared).unwrap();
        let hidden = rig.frame(1, 1);
        assert!(!names_water(&hidden), "{label}: declared frame");
        assert_eq!(rig.water_calls(), 0, "{label}: water draws");

        rig.renderer.reconfigure(&context, stock).unwrap();
        let again = rig.frame(1, 1);
        assert!(names_water(&again), "{label}: water returns");
        assert_eq!(again.passes, expected.passes, "{label}: the plan");
        assert!(again.colour == expected.colour, "{label}: the frame");
        assert!(rig.water_calls() > 0, "{label}: draws recorded");
    }
}
