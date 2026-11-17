//! How often the stock plan of a scene frame is rebuilt, and that its attachments follow the
//! allocations they are bound to rather than the schedule they were first resolved for.
use super::*;
use crate::compositor::StockPlanStatistics;

impl Rig<'_> {
    pub(super) fn statistics(&mut self) -> StockPlanStatistics {
        self.renderer.renderer_mut().stock_plan_statistics()
    }
}

#[test]
fn gpu_moving_frames_reuse_the_plan_and_a_schedule_change_rebuilds_it_once() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let mut rig = Rig::new(&context, Some(reflections()));
    // The first frame builds the plan and allocates the snapshots its attachments bind to, so the
    // second resolves the binding table again against them.
    rig.draw(0, viewpoint(0));
    rig.draw(0, viewpoint(1));
    let warm = rig.statistics();
    assert_eq!(warm.rebuilds, 1, "one schedule built for the first frame");
    for index in [2, 3, 0, 1, 2] {
        rig.draw(0, viewpoint(index));
    }
    let moved = rig.statistics();
    assert_eq!(
        moved.rebuilds, warm.rebuilds,
        "a camera that moves does not rebuild the plan"
    );
    assert_eq!(moved.binding_resolutions, warm.binding_resolutions);
    assert_eq!(moved.reuses, warm.reuses + 5);

    rig.renderer
        .renderer_mut()
        .set_clear_color(wgpu::Color {
            r: 0.25,
            g: 0.0,
            b: 0.5,
            a: 1.0,
        })
        .unwrap();
    for index in 0..3 {
        rig.draw(0, viewpoint(index));
    }
    let cleared = rig.statistics();
    assert_eq!(
        cleared.rebuilds,
        moved.rebuilds + 1,
        "a clear colour is part of the passes, so changing it rebuilds once"
    );

    rig.reconfigure([33, 25]);
    for index in 0..3 {
        rig.draw(0, viewpoint(index));
    }
    assert_eq!(
        rig.statistics().rebuilds,
        cleared.rebuilds + 1,
        "another frame extent rebuilds once"
    );
}

#[test]
fn gpu_frame_slots_and_an_identical_reconfigure_rebind_without_rebuilding() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let mut rig = Rig::new(&context, Some(reflections()));
    rig.draw(0, viewpoint(0));
    let built = rig.statistics();
    assert_eq!(built.rebuilds, 1);
    let first = rig.read_snapshot(0);
    let untouched = rig.read_snapshot(1);
    assert!(first.iter().any(|byte| *byte != 0));
    assert!(untouched.iter().all(|byte| *byte == 0));

    rig.draw(1, viewpoint(1));
    let second = rig.read_snapshot(1);
    assert_ne!(first, second, "the viewpoints must draw differently");
    assert_eq!(
        rig.read_snapshot(0),
        first,
        "the second frame writes only the snapshots of its own slot"
    );
    let slot_one = rig.statistics();

    rig.draw(0, viewpoint(2));
    let third = rig.read_snapshot(0);
    assert!(third != first && third != second);
    assert_eq!(
        rig.read_snapshot(1),
        second,
        "the third frame writes only the snapshots of its own slot"
    );
    let slot_zero = rig.statistics();
    assert_ne!(
        slot_zero.binding, slot_one.binding,
        "another frame slot binds to other allocations"
    );
    assert_eq!(
        slot_zero.rebuilds, built.rebuilds,
        "slots share one schedule"
    );

    // The test keeps the allocations of the renderer it replaces, so nothing can be written to
    // them without the test seeing it.
    let held = rig.snapshot(0);
    let held_second = rig.snapshot(1);
    rig.reconfigure(SIZE);
    rig.draw(0, viewpoint(3));
    let replaced = rig.statistics();
    assert_eq!(
        replaced.rebuilds, built.rebuilds,
        "an identical configuration has the schedule of the one it replaces"
    );
    assert_ne!(replaced.binding, slot_zero.binding);
    let fourth = rig.read_snapshot(0);
    assert!(fourth != third && fourth != first && fourth != second);
    assert_ne!(
        rig.snapshot(0),
        held,
        "the replacement allocates its own snapshots"
    );
    assert_eq!(
        rig.read(&held),
        third,
        "the replaced allocations are not written"
    );
    assert_eq!(rig.read(&held_second), second);
}
