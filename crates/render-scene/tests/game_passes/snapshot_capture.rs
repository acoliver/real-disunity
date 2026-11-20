//! A game that samples the opaque snapshots in a frame whose shadows are fitted by a same-frame
//! capture. The capture submits the whole frame as its suffix, so the snapshots the frame
//! writes, including the device depth only a game reads, are part of its write contract.
use super::fixture::{Rig, SIZE, quad_scene};
use super::shadowed::{capture, shadowed};
use super::snapshot_game::{FRAME_PASS, Fill, Plan, SnapGame, TRANSPARENT_PASS};
use worldrunner_render_scene::compositor::SnapshotReads;

const DEPTHS: SnapshotReads = SnapshotReads {
    linear_depth: true,
    device_depth: true,
    ..SnapshotReads::NONE
};

fn plan() -> Plan {
    Plan {
        depth_to_frame: true,
        ..Plan::default()
    }
}

#[test]
fn a_same_frame_capture_admits_the_snapshot_writes_of_a_game_that_reads_them() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (inventory, seen) = SnapGame::inventory(DEPTHS, plan());
    let mut unshadowed = Rig::new(&quad_scene(), SIZE, |settings| {
        settings.game_passes = inventory;
    });
    let reference = unshadowed.render().unwrap();
    assert_ne!(
        Rig::pixel(&reference, 32, 24),
        Rig::pixel(&reference, 3, 20),
        "the covered and uncovered pixels differ in the reference"
    );
    assert_eq!(Rig::pixel(&reference, 3, 20), [255, 255, 0, 255]);

    let (inventory, shadowed_seen) = SnapGame::inventory(DEPTHS, plan());
    let (mut rig, mut scene) = shadowed(inventory);
    for frame in 0..3 {
        let image = capture(&mut rig, &mut scene, frame);
        assert_eq!(image, reference, "frame {frame}");
    }
    assert_eq!(shadowed_seen.lock().unwrap().recorded, [FRAME_PASS; 3]);
    assert_eq!(seen.lock().unwrap().recorded, [FRAME_PASS]);
    assert!(
        shadowed_seen.lock().unwrap().lent.iter().all(Result::is_ok),
        "every frame lent its snapshots"
    );
}

#[test]
fn a_same_frame_capture_runs_the_snapshot_readers_before_the_transparent_passes() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let both = || Plan {
        fill: Fill::AfterTransparent,
        ..plan()
    };
    let (inventory, seen) = SnapGame::inventory(DEPTHS, both());
    let mut unshadowed = Rig::new(&quad_scene(), SIZE, |settings| {
        settings.game_passes = inventory;
    });
    let reference = unshadowed.render().unwrap();
    let order = [FRAME_PASS, TRANSPARENT_PASS];
    assert_eq!(seen.lock().unwrap().recorded, order);

    let (inventory, shadowed_seen) = SnapGame::inventory(DEPTHS, both());
    let (mut rig, mut scene) = shadowed(inventory);
    for frame in 0..3 {
        let image = capture(&mut rig, &mut scene, frame);
        assert_eq!(image, reference, "frame {frame}");
    }
    assert_eq!(
        shadowed_seen.lock().unwrap().recorded,
        [FRAME_PASS, TRANSPARENT_PASS].repeat(3)
    );
}
