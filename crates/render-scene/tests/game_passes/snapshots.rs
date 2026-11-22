//! A game pass placed at `GamePlace::AfterOpaqueSnapshots` samples the frame's opaque
//! snapshots: the lit opaque colour, the device depth and the linear depth. The frames are real:
//! a configured renderer over the synthetic quad scene on lavapipe.
use super::fixture::{Rig, SIZE, quad_scene};
use super::snapshot_game::{
    COLOR_COPY, COLOR_PASS, DEPTH_PROBE, FILL_PASS, FRAME_PASS, Fill, LATE_COLOR_PASS,
    OPAQUE_FILL_PASS, Plan, SnapGame, TRANSPARENT_PASS, built_handle, read_texels, texel,
};
use worldrunner_render_post::{FrameConfig, FrameError};
use worldrunner_render_scene::{
    compositor::{FrameSnapshotError, PostWaterDepth, SnapshotReads, SnapshotStatistics},
    wgpu,
};

/// A pixel the quad covers, and one it does not.
pub const COVERED: (usize, usize) = (32, 24);
pub const UNCOVERED: (usize, usize) = (3, 20);

pub const COLOR: SnapshotReads = SnapshotReads {
    color: true,
    ..SnapshotReads::NONE
};
pub const DEPTHS: SnapshotReads = SnapshotReads {
    linear_depth: true,
    device_depth: true,
    ..SnapshotReads::NONE
};

/// The device depth of the ground plane five metres below the camera, whose orthographic depth
/// runs linearly from 0 at 0.1 m to 1 at 10 m.
pub fn plane_depth() -> f32 {
    (5.0 - 0.1) / (10.0 - 0.1)
}

#[test]
fn a_pass_after_the_opaque_snapshots_samples_the_lit_opaque_colour_and_no_transparent_pass() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let mut stock = Rig::hdr(&quad_scene(), SIZE, |_| {});
    stock.render().unwrap();
    let opaque = read_texels(&stock, stock.renderer.targets().scene().texture(), 8);
    let expected = |at: (usize, usize)| texel(&opaque, 2, at.0, at.1).map(f32::to_bits);
    assert!(
        f32::from_bits(expected(COVERED)[0]) > 0.0,
        "the stock quad is lit"
    );
    assert_ne!(expected(COVERED), expected(UNCOVERED));

    let (inventory, seen) = SnapGame::inventory(
        COLOR,
        Plan {
            copy_color: true,
            fill: Fill::AfterTransparent,
            ..Plan::default()
        },
    );
    let mut rig = Rig::hdr(&quad_scene(), SIZE, |settings| {
        settings.game_passes = inventory;
    });
    rig.render().unwrap();

    let copy = rig.renderer.game_targets().texture(COLOR_COPY).unwrap();
    let copied = read_texels(&rig, &copy, 8);
    for at in [COVERED, UNCOVERED] {
        assert_eq!(
            texel(&copied, 2, at.0, at.1).map(f32::to_bits),
            expected(at),
            "pixel {at:?}"
        );
    }
    let frame = read_texels(&rig, rig.renderer.targets().scene().texture(), 8);
    assert_eq!(
        texel(&frame, 2, COVERED.0, COVERED.1)[..3],
        [1.0, 0.0, 1.0],
        "the pass after the transparent passes drew over the frame"
    );
    assert_eq!(
        seen.lock().unwrap().recorded,
        [COLOR_PASS, TRANSPARENT_PASS],
        "the snapshot reader ran before the transparent passes"
    );
}

#[test]
fn the_colour_snapshot_is_a_copy_that_later_draws_into_the_frame_do_not_reach() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let mut stock = Rig::hdr(&quad_scene(), SIZE, |_| {});
    stock.render().unwrap();
    let opaque = read_texels(&stock, stock.renderer.targets().scene().texture(), 8);

    let (inventory, seen) = SnapGame::inventory(
        COLOR,
        Plan {
            fill: Fill::ThenCopyColor,
            ..Plan::default()
        },
    );
    let mut rig = Rig::hdr(&quad_scene(), SIZE, |settings| {
        settings.game_passes = inventory;
    });
    rig.render().unwrap();
    let frame = read_texels(&rig, rig.renderer.targets().scene().texture(), 8);
    let copy = rig.renderer.game_targets().texture(COLOR_COPY).unwrap();
    let copied = read_texels(&rig, &copy, 8);
    for at in [COVERED, UNCOVERED] {
        assert_eq!(texel(&frame, 2, at.0, at.1)[..3], [1.0, 0.0, 1.0], "{at:?}");
        assert_eq!(
            texel(&copied, 2, at.0, at.1).map(f32::to_bits),
            texel(&opaque, 2, at.0, at.1).map(f32::to_bits),
            "the copy taken before the fill still holds the opaque colour at {at:?}"
        );
    }
    assert_eq!(seen.lock().unwrap().recorded, [FILL_PASS, LATE_COLOR_PASS]);
}

#[test]
fn the_colour_snapshot_includes_what_a_pass_after_opaque_drew() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (inventory, seen) = SnapGame::inventory(
        COLOR,
        Plan {
            copy_color: true,
            fill: Fill::AfterOpaque,
            ..Plan::default()
        },
    );
    let mut rig = Rig::hdr(&quad_scene(), SIZE, |settings| {
        settings.game_passes = inventory;
    });
    rig.render().unwrap();
    let copy = rig.renderer.game_targets().texture(COLOR_COPY).unwrap();
    let copied = read_texels(&rig, &copy, 8);
    for at in [COVERED, UNCOVERED] {
        assert_eq!(
            texel(&copied, 2, at.0, at.1)[..3],
            [1.0, 0.0, 1.0],
            "{at:?}"
        );
    }
    assert_eq!(
        seen.lock().unwrap().recorded,
        [OPAQUE_FILL_PASS, COLOR_PASS]
    );
}

#[test]
fn the_device_depth_snapshot_holds_the_frames_depth_and_the_linear_depth_holds_view_metres() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (inventory, _) = SnapGame::inventory(
        DEPTHS,
        Plan {
            probe_depth: true,
            ..Plan::default()
        },
    );
    let mut rig = Rig::new(&quad_scene(), SIZE, |settings| {
        settings.game_passes = inventory;
    });
    rig.render().unwrap();
    let probe = rig.renderer.game_targets().texture(DEPTH_PROBE).unwrap();
    let probed = read_texels(&rig, &probe, 16);

    let [device, linear, ..] = texel(&probed, 4, COVERED.0, COVERED.1);
    assert!(
        (device - plane_depth()).abs() < 1e-5,
        "device depth {device}, expected {}",
        plane_depth()
    );
    assert!((linear - 5.0).abs() < 1e-3, "{linear} m, expected 5 m");

    let [device, linear, ..] = texel(&probed, 4, UNCOVERED.0, UNCOVERED.1);
    assert_eq!(
        device.to_bits(),
        1.0_f32.to_bits(),
        "no surface leaves the cleared depth"
    );
    assert!(
        (linear - 10.0).abs() < 1e-3,
        "{linear} m, expected the 10 m far plane"
    );
}

#[test]
fn a_game_that_reads_snapshots_gets_them_in_a_frame_with_no_water_and_the_stock_frame_none() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let mut stock = Rig::new(&quad_scene(), SIZE, |_| {});
    stock.render().unwrap();
    assert_eq!(
        stock.renderer.snapshot_statistics(),
        SnapshotStatistics::default()
    );

    let (inventory, seen) = SnapGame::inventory(
        DEPTHS,
        Plan {
            probe_depth: true,
            ..Plan::default()
        },
    );
    let mut rig = Rig::new(&quad_scene(), SIZE, |settings| {
        settings.game_passes = inventory;
    });
    let owned = SnapshotStatistics {
        slots: 3,
        device_depth_slots: 3,
        post_water_device_depth_slots: 0,
    };
    assert_eq!(
        rig.renderer.snapshot_statistics(),
        owned,
        "owned when built"
    );
    rig.render().unwrap();
    assert_eq!(rig.renderer.snapshot_statistics(), owned);
    assert_eq!(seen.lock().unwrap().lent, [Ok(())]);
}

#[test]
fn colour_only_readers_leave_the_device_depth_unallocated() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (inventory, _) = SnapGame::inventory(
        COLOR,
        Plan {
            copy_color: true,
            ..Plan::default()
        },
    );
    let mut rig = Rig::hdr(&quad_scene(), SIZE, |settings| {
        settings.game_passes = inventory;
    });
    rig.render().unwrap();
    assert_eq!(
        rig.renderer.snapshot_statistics(),
        SnapshotStatistics {
            slots: 3,
            device_depth_slots: 0,
            post_water_device_depth_slots: 0,
        }
    );
}

#[test]
fn a_game_whose_frame_has_no_reading_pass_gets_the_not_prepared_refusal() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (inventory, seen) = SnapGame::inventory(
        SnapshotReads::ALL,
        Plan {
            fill: Fill::AfterTransparent,
            ..Plan::default()
        },
    );
    let mut rig = Rig::hdr(&quad_scene(), SIZE, |settings| {
        settings.game_passes = inventory;
    });
    rig.render().unwrap();
    assert_eq!(
        seen.lock().unwrap().lent,
        [Err(FrameSnapshotError::NotPrepared)]
    );
}

#[test]
fn a_game_with_no_declared_reads_allocates_no_snapshots_and_is_refused_a_read_by_name() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (inventory, seen) = SnapGame::inventory(
        SnapshotReads::NONE,
        Plan {
            fill: Fill::AfterTransparent,
            ..Plan::default()
        },
    );
    let mut rig = Rig::new(&quad_scene(), SIZE, |settings| {
        settings.game_passes = inventory;
    });
    rig.render().unwrap();
    assert_eq!(
        rig.renderer.snapshot_statistics(),
        SnapshotStatistics::default()
    );
    assert_eq!(
        seen.lock().unwrap().lent,
        [Err(FrameSnapshotError::NotPrepared)]
    );

    let (inventory, _) = SnapGame::inventory(
        SnapshotReads::NONE,
        Plan {
            probe_depth: true,
            ..Plan::default()
        },
    );
    let mut rig = Rig::new(&quad_scene(), SIZE, |settings| {
        settings.game_passes = inventory;
    });
    let error = rig.render().unwrap_err();
    let FrameError::FeaturePreparation { feature, reason } = error else {
        panic!("expected a refusal by name, got {error:?}");
    };
    assert_eq!(feature, "frame snapshots");
    assert!(reason.contains("snapshot_reads"), "{reason}");
}

#[test]
fn depth_tested_overlay_snapshots_gain_the_device_depth_a_game_reads() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (inventory, seen) = SnapGame::inventory(
        DEPTHS,
        Plan {
            depth_to_frame: true,
            ..Plan::default()
        },
    );
    let mut rig = Rig::new(&quad_scene(), SIZE, |settings| {
        settings.ui_depth = true;
        settings.game_passes = inventory;
    });
    rig.render().unwrap();
    assert_eq!(
        rig.renderer.snapshot_statistics(),
        SnapshotStatistics {
            slots: 3,
            device_depth_slots: 3,
            post_water_device_depth_slots: 0,
        }
    );
    assert_eq!(seen.lock().unwrap().recorded, [FRAME_PASS]);
}

#[test]
fn the_handle_refuses_outside_a_frame_by_name() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (inventory, _) = SnapGame::inventory(
        DEPTHS,
        Plan {
            probe_depth: true,
            ..Plan::default()
        },
    );
    let mut rig = Rig::new(&quad_scene(), SIZE, |settings| {
        settings.game_passes = inventory;
    });
    let handle = built_handle();
    let outside = |handle: &worldrunner_render_scene::compositor::FrameSnapshotHandle| {
        handle.current().expect_err("no frame is being prepared")
    };
    assert_eq!(outside(&handle), FrameSnapshotError::OutsideFrame);
    rig.render().unwrap();
    assert_eq!(
        outside(&handle),
        FrameSnapshotError::OutsideFrame,
        "a submitted frame's snapshots are not lent afterwards"
    );
    let FrameError::FeaturePreparation { feature, reason } =
        FrameError::from(FrameSnapshotError::OutsideFrame)
    else {
        panic!("a typed refusal converts to a named feature error");
    };
    assert_eq!(feature, "frame snapshots");
    assert!(
        reason.contains("only while a frame is being encoded"),
        "{reason}"
    );
}

/// What `count` textures of the frame's extent in `format` cost, texel size times texels times
/// count, the way the renderer configuration charges its snapshot textures.
fn charged(format: wgpu::TextureFormat, count: u64) -> u64 {
    let texel = u64::from(format.block_copy_size(None).unwrap());
    count * u64::from(SIZE[0] * SIZE[1]) * texel
}

#[test]
fn the_allocation_budget_charges_the_snapshots_a_game_declares_it_reads() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let resident = |declared| {
        let (inventory, _) = SnapGame::inventory(declared, Plan::default());
        Rig::new(&quad_scene(), SIZE, |settings| {
            settings.game_passes = inventory;
        })
        .resident_bytes()
    };
    // A colour snapshot per frame slot, two depths a slot (linear and post-water), and the
    // uniform and staging buffers of the snapshot executor, one each per slot.
    let shared = charged(wgpu::TextureFormat::Rgba16Float, 3)
        + charged(wgpu::TextureFormat::R32Float, 6)
        + 3 * (304 + 192);
    let device_depth = charged(wgpu::TextureFormat::R32Float, 3);
    let none = resident(SnapshotReads::NONE);
    let colour = resident(COLOR);
    assert_eq!(
        colour - none,
        shared,
        "the shared snapshots, without device depth"
    );
    assert_eq!(
        resident(DEPTHS) - colour,
        device_depth,
        "the device depth copy is reserved when a game reads it, and the colour is not"
    );
    let post_water_linear = SnapshotReads {
        post_water_depth: PostWaterDepth::Linear,
        ..COLOR
    };
    assert_eq!(
        resident(post_water_linear) - colour,
        0,
        "the post-water linear depth is one of the shared depths"
    );
    assert_eq!(
        resident(SnapshotReads::ALL) - colour,
        2 * device_depth,
        "reading everything costs the shared snapshots, the device depth and the post-water \
         device depth once each"
    );
}

#[test]
fn a_replacement_gives_the_new_generation_its_own_snapshots() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (inventory, _) = SnapGame::inventory(DEPTHS, Plan::default());
    let mut rig = Rig::new(&quad_scene(), SIZE, |settings| {
        settings.game_passes = inventory;
    });
    let owned = SnapshotStatistics {
        slots: 3,
        device_depth_slots: 3,
        post_water_device_depth_slots: 0,
    };
    assert_eq!(rig.renderer.snapshot_statistics(), owned);
    let mut larger = rig.renderer.configuration().clone();
    larger.frame = FrameConfig::new([128, 96], larger.frame.format, 1);
    rig.renderer.reconfigure(&rig.context, larger).unwrap();
    assert_eq!(rig.renderer.snapshot_statistics(), owned);
}
