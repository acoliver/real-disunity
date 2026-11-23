//! A game that reads the opaque snapshots in a scene that also has stock water. When the game
//! draws the water the frame plans no water pass, so the snapshot request comes from the game's
//! declared reads alone. With the stock draws the game's request joins the water's own requests
//! for the final depth and the depth after the water. The frames are real: a configured HDR
//! renderer over the synthetic quad scene with one water body, on lavapipe.
use super::fixture::{Rig, SIZE, quad_scene, water_body};
use super::shadowed::effects;
use super::snapshot_game::{
    COLOR_COPY, COLOR_PASS, DEPTH_PROBE, Fill, Plan, Seen, SnapGame, TRANSPARENT_PASS, read_texels,
    texel,
};
use super::snapshots::{COLOR, COVERED, DEPTHS, plane_depth};
use std::sync::{Arc, Mutex};
use worldrunner_render_plan::RenderScene;
use worldrunner_render_scene::compositor::{
    FrameSnapshotError, GamePasses, SnapshotReads, SnapshotStatistics, StockReplacement,
};

/// A pixel in the ring of open water around the quad: 3.5 m east of the view centre, where the
/// quad ends at 3 m and the water at 4 m.
const WATER_ONLY: (usize, usize) = (53, 24);

fn depth_plan() -> Plan {
    Plan {
        probe_depth: true,
        ..Plan::default()
    }
}

/// The quad scene with the water body in its effects, which the renderer must be configured with
/// the same bodies to draw.
pub(super) fn water_scene() -> RenderScene {
    let source = quad_scene();
    let effects = effects().with_water_bodies(vec![water_body()]).unwrap();
    source
        .with_objects_and_effects(source.geometry().objects().to_vec(), Some(effects))
        .unwrap()
}

/// An HDR rig with one water body and the game installed, which draws the water or not as its
/// declaration says.
fn rig(inventory: GamePasses) -> Rig {
    Rig::hdr(&water_scene(), SIZE, |settings| {
        settings.water = vec![water_body()];
        settings.game_passes = inventory;
    })
}

fn water_calls(rig: &Rig) -> u64 {
    rig.gpu.statistics().passes.water.calls
}

/// Renders one frame of a game that reads both depth snapshots into the depth probe, and checks
/// what the game was lent and what it read at the covered pixel and in the open water.
fn read_depths_in_a_frame(replaces: StockReplacement) -> Rig {
    let plan = Plan {
        replaces,
        ..depth_plan()
    };
    let (inventory, seen) = SnapGame::inventory(DEPTHS, plan);
    let mut rig = rig(inventory);
    rig.render().expect("the frame renders without refusal");
    assert_eq!(seen.lock().unwrap().lent, [Ok(())], "{replaces:?}: lent");

    let probe = rig.renderer.game_targets().texture(DEPTH_PROBE).unwrap();
    let probed = read_texels(&rig, &probe, 16);
    let [device, linear, ..] = texel(&probed, 4, COVERED.0, COVERED.1);
    assert!(
        (device - plane_depth()).abs() < 1e-5,
        "{replaces:?}: device depth {device}, expected {}",
        plane_depth()
    );
    assert!(
        (linear - 5.0).abs() < 1e-3,
        "{replaces:?}: {linear} m, expected 5 m"
    );

    // The snapshots are of the opaque frame, so the water, which is drawn later, is not in them.
    let [device, linear, ..] = texel(&probed, 4, WATER_ONLY.0, WATER_ONLY.1);
    assert_eq!(
        device.to_bits(),
        1.0_f32.to_bits(),
        "{replaces:?}: the open water leaves the cleared device depth"
    );
    assert!(
        (linear - 10.0).abs() < 1e-3,
        "{replaces:?}: {linear} m, expected the 10 m far plane in the open water"
    );
    rig
}

#[test]
fn a_game_reading_depths_gets_them_when_it_draws_the_water_and_no_water_pass_runs() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let rig = read_depths_in_a_frame(StockReplacement::WATER);
    assert_eq!(water_calls(&rig), 0, "no stock water pass records draws");
    assert_eq!(
        rig.renderer.snapshot_statistics(),
        SnapshotStatistics {
            slots: 3,
            device_depth_slots: 3,
            post_water_device_depth_slots: 0,
        },
        "the banks the game declared"
    );
}

#[test]
fn a_game_reading_depths_gets_them_beside_the_stock_water_passes() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let rig = read_depths_in_a_frame(StockReplacement::NONE);
    assert!(water_calls(&rig) > 0, "the stock water pass records draws");
    assert_eq!(
        rig.renderer.snapshot_statistics(),
        SnapshotStatistics {
            slots: 3,
            device_depth_slots: 3,
            post_water_device_depth_slots: 0,
        },
        "the device depth bank is owned alongside the water's banks"
    );
}

#[test]
fn a_game_reading_colour_gets_the_opaque_colour_when_it_draws_the_water() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let mut stock = Rig::hdr(&quad_scene(), SIZE, |_| {});
    stock.render().unwrap();
    let opaque = read_texels(&stock, stock.renderer.targets().scene().texture(), 8);
    let expected = texel(&opaque, 2, COVERED.0, COVERED.1).map(f32::to_bits);
    assert!(f32::from_bits(expected[0]) > 0.0, "the stock quad is lit");

    let (inventory, seen) = SnapGame::inventory(
        COLOR,
        Plan {
            copy_color: true,
            fill: Fill::AfterTransparent,
            replaces: StockReplacement::WATER,
            ..Plan::default()
        },
    );
    let mut rig = rig(inventory);
    rig.render().expect("the frame renders without refusal");
    assert_eq!(water_calls(&rig), 0, "no stock water pass records draws");
    assert_eq!(seen.lock().unwrap().lent, [Ok(())]);
    assert_eq!(
        seen.lock().unwrap().recorded,
        [COLOR_PASS, TRANSPARENT_PASS],
        "the colour reader ran before the transparent passes"
    );

    let copy = rig.renderer.game_targets().texture(COLOR_COPY).unwrap();
    let copied = read_texels(&rig, &copy, 8);
    assert_eq!(
        texel(&copied, 2, COVERED.0, COVERED.1).map(f32::to_bits),
        expected,
        "the colour snapshot at the covered pixel is the opaque colour"
    );
}

#[test]
fn a_game_with_no_declared_reads_is_lent_nothing_beside_water_it_draws() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (inventory, seen): (_, Arc<Mutex<Seen>>) = SnapGame::inventory(
        SnapshotReads::NONE,
        Plan {
            fill: Fill::AfterTransparent,
            replaces: StockReplacement::WATER,
            ..Plan::default()
        },
    );
    let mut rig = rig(inventory);
    rig.render().expect("the frame renders without refusal");
    assert_eq!(water_calls(&rig), 0);
    assert_eq!(
        seen.lock().unwrap().lent,
        [Err(FrameSnapshotError::NotPrepared)],
        "water that the game draws asks for no snapshot on its behalf"
    );
}
