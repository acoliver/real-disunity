//! The opaque snapshots across frame slots, frames that stop reading them, a reader of only the
//! linear depth and a reconfiguration to another extent. The frames
//! are real: a configured renderer over the synthetic quad scene on lavapipe.
use super::fixture::{Rig, SIZE, quad_scene};
use super::snapshot_game::{DEPTH_PROBE, LINEAR_PASS, Plan, SnapGame, read_texels, texel};
use super::snapshots::{COVERED, DEPTHS, UNCOVERED};
use worldrunner_render_post::FrameConfig;
use worldrunner_render_scene::compositor::{FrameSnapshotError, SnapshotReads, SnapshotStatistics};

const LINEAR: SnapshotReads = SnapshotReads {
    linear_depth: true,
    ..SnapshotReads::NONE
};

/// The device depth of the ground plane `metres` below the camera, whose orthographic depth runs
/// linearly from 0 at 0.1 m to 1 at 10 m.
fn device_depth_at(metres: f32) -> f32 {
    (metres - 0.1) / (10.0 - 0.1)
}

fn probing_depths() -> Plan {
    Plan {
        probe_depth: true,
        ..Plan::default()
    }
}

#[test]
fn every_frame_slot_lends_the_snapshots_its_own_frame_wrote() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (inventory, seen) = SnapGame::inventory(DEPTHS, probing_depths());
    let mut rig = Rig::new(&quad_scene(), SIZE, |settings| {
        settings.game_passes = inventory;
    });
    // The camera is at a different height in every frame, so the ground plane's depth tells the
    // frames apart. A slot is revisited after the others, and every frame's own depth is read
    // back before the next frame renders.
    let frames = [
        (0, 3000),
        (1, 5000),
        (2, 8000),
        (1, 4000),
        (0, 6000),
        (2, 2000),
    ];
    for (frame, (slot, height)) in frames.into_iter().enumerate() {
        rig.render_in_slot(slot, height).unwrap();
        let probe = rig.renderer.game_targets().texture(DEPTH_PROBE).unwrap();
        let probed = read_texels(&rig, &probe, 16);
        let [device, linear, ..] = texel(&probed, 4, COVERED.0, COVERED.1);
        let metres = f32::from(u16::try_from(height).unwrap()) / 1000.0;
        assert!(
            (linear - metres).abs() < 1e-3,
            "frame {frame} in slot {slot}: linear depth {linear} m, expected {metres} m"
        );
        assert!(
            (device - device_depth_at(metres)).abs() < 1e-5,
            "frame {frame} in slot {slot}: device depth {device}, expected {}",
            device_depth_at(metres)
        );
    }
    assert_eq!(seen.lock().unwrap().lent, [Ok(()); 6]);
}

#[test]
fn a_frame_that_plans_no_reading_pass_is_not_lent_the_previous_frames_snapshots() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (inventory, seen) = SnapGame::inventory(DEPTHS, probing_depths());
    let mut rig = Rig::new(&quad_scene(), SIZE, |settings| {
        settings.game_passes = inventory;
    });
    rig.render().unwrap();
    seen.lock().unwrap().replan = Some(Plan::default());
    rig.render().unwrap();
    seen.lock().unwrap().replan = Some(probing_depths());
    rig.render().unwrap();

    let seen = seen.lock().unwrap();
    assert_eq!(
        seen.lent,
        [Ok(()), Err(FrameSnapshotError::NotPrepared), Ok(())],
        "the middle frame wrote no snapshots, and the first frame's are not its own"
    );
    assert_eq!(seen.lent_shape, [DEPTHS, SnapshotReads::NONE, DEPTHS]);
}

#[test]
fn a_game_that_reads_only_the_linear_depth_gets_only_that_snapshot() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (inventory, seen) = SnapGame::inventory(
        LINEAR,
        Plan {
            probe_linear: true,
            ..Plan::default()
        },
    );
    let mut rig = Rig::new(&quad_scene(), SIZE, |settings| {
        settings.game_passes = inventory;
    });
    let owned = SnapshotStatistics {
        slots: 3,
        device_depth_slots: 0,
        post_water_device_depth_slots: 0,
    };
    assert_eq!(
        rig.renderer.snapshot_statistics(),
        owned,
        "owned when built"
    );
    rig.render().unwrap();
    assert_eq!(rig.renderer.snapshot_statistics(), owned);

    let probe = rig.renderer.game_targets().texture(DEPTH_PROBE).unwrap();
    let probed = read_texels(&rig, &probe, 16);
    let [covered, ..] = texel(&probed, 4, COVERED.0, COVERED.1);
    assert!((covered - 5.0).abs() < 1e-3, "{covered} m, expected 5 m");
    let [uncovered, ..] = texel(&probed, 4, UNCOVERED.0, UNCOVERED.1);
    assert!(
        (uncovered - 10.0).abs() < 1e-3,
        "{uncovered} m, expected the 10 m far plane"
    );
    let seen = seen.lock().unwrap();
    assert_eq!(seen.lent, [Ok(())]);
    assert_eq!(
        seen.lent_shape,
        [LINEAR],
        "the lent set holds the linear depth and no colour or device depth"
    );
    assert_eq!(seen.recorded, [LINEAR_PASS]);
}

#[test]
fn reconfiguring_to_another_extent_owns_the_device_depth_before_the_next_frame_plans() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (inventory, seen) = SnapGame::inventory(DEPTHS, probing_depths());
    let mut rig = Rig::new(&quad_scene(), SIZE, |settings| {
        settings.game_passes = inventory;
    });
    let owned = SnapshotStatistics {
        slots: 3,
        device_depth_slots: 3,
        post_water_device_depth_slots: 0,
    };
    rig.render().unwrap();
    assert_eq!(rig.renderer.snapshot_statistics(), owned);

    let mut larger = rig.renderer.configuration().clone();
    larger.frame = FrameConfig::new([128, 96], larger.frame.format, 1);
    rig.renderer.reconfigure(&rig.context, larger).unwrap();
    assert_eq!(
        rig.renderer.snapshot_statistics(),
        owned,
        "the new extent's snapshots include the device depth with no frame planned yet"
    );
    rig.render().unwrap();
    assert_eq!(rig.renderer.snapshot_statistics(), owned);
    assert_eq!(seen.lock().unwrap().lent, [Ok(()), Ok(())]);
}
