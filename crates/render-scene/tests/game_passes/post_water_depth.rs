//! A game pass placed at `GamePlace::AfterTransparent` samples the post-water depth: the frame's
//! depth after every pass before the transparent ones, which includes the depth a game pass wrote
//! at `AfterOpaqueSnapshots`. The frames are real: a configured renderer over the synthetic quad
//! scene on lavapipe, with the water drawn by the stock passes and by the game.
use super::declaring::declaring;
use super::fixture::{Rig, SIZE, quad_scene};
use super::post_water_game::{
    EARLY_READER, Lent, PROBE, Plan, PostWaterGame, READER, Reading, Shared, WRITER, WRITTEN_DEPTH,
};
use super::shadowed::{capture, effects, shadowed};
use super::snapshot_game::{read_texels, texel};
use worldrunner_render_plan::{RenderScene, SceneWaterBody, SceneWaterSettings, SceneWaterSurface};
use worldrunner_render_post::{FrameConfig, FrameError};
use worldrunner_render_scene::compositor::{
    FrameSnapshotError, PostWaterDepth, SnapshotReads, SnapshotStatistics, StockReplacement,
};

/// Pixels the writer's left half and the quad's footprint split four ways.
const LEFT_QUAD: (usize, usize) = (20, 24);
const LEFT_BARE: (usize, usize) = (3, 20);
const RIGHT_QUAD: (usize, usize) = (45, 24);
const RIGHT_BARE: (usize, usize) = (60, 20);
/// Bare ground inside the water body's footprint, outside the writer's half.
const RIGHT_WATER: (usize, usize) = (53, 20);
const ALL_PIXELS: [(usize, usize); 4] = [LEFT_QUAD, LEFT_BARE, RIGHT_QUAD, RIGHT_BARE];

/// The device depth of the ground plane five metres below the camera, whose orthographic depth
/// runs linearly from 0 at 0.1 m to 1 at 10 m.
fn plane_depth() -> f32 {
    (5.0 - 0.1) / (10.0 - 0.1)
}

/// The metres an orthographic device depth `depth` is from the camera.
fn metres(depth: f32) -> f32 {
    0.1 + depth * (10.0 - 0.1)
}

fn reading(reading: Reading) -> Plan {
    Plan {
        reader: Some(reading),
        ..Plan::default()
    }
}

fn writing_and_reading(read: Reading) -> Plan {
    Plan {
        write_depth: true,
        ..reading(read)
    }
}

/// One texel of the probe a frame's reader wrote: post-water device, post-water linear, opaque
/// device and opaque linear depth.
fn probed(rig: &Rig, at: (usize, usize)) -> [f32; 4] {
    let probe = rig.renderer.game_targets().texture(PROBE).unwrap();
    texel(&read_texels(rig, &probe, 16), 4, at.0, at.1)
}

/// Every texel of the probe a frame's reader wrote.
fn whole_probe(rig: &Rig) -> Vec<u8> {
    let probe = rig.renderer.game_targets().texture(PROBE).unwrap();
    read_texels(rig, &probe, 16)
}

fn assert_close(actual: f32, expected: f32, what: &str) {
    assert!(
        (actual - expected).abs() < 1e-3,
        "{what}: {actual}, expected {expected}"
    );
}

/// A water body below the quad, which the stock passes or the game draw.
fn water() -> SceneWaterBody {
    SceneWaterBody {
        material: SceneWaterSettings::STOCK,
        surface: SceneWaterSurface::Rectangle,
        flow_millimeters_per_second: [0; 2],
        height_millimeters: -100,
        east_min_millimeters: -4000,
        north_min_millimeters: -4000,
        east_extent_millimeters: 8000,
        north_extent_millimeters: 8000,
        color_micro: [50_000, 200_000, 250_000],
        absorption_micro: [300_000, 100_000, 50_000],
        roughness_micro: 200_000,
    }
}

/// The quad scene with the water body, which the stock passes or the game draw.
fn water_scene() -> RenderScene {
    let source = quad_scene();
    let effects = effects().with_water_bodies(vec![water()]).unwrap();
    source
        .with_objects_and_effects(source.geometry().objects().to_vec(), Some(effects))
        .unwrap()
}

/// An HDR rig with the quad scene and the game, with the water body when `replaces` is given,
/// drawn by the game if `replaces` says it replaces the stock water.
fn build(declared: SnapshotReads, plan: Plan, replaces: Option<StockReplacement>) -> (Rig, Shared) {
    let plan = Plan {
        replaces: replaces.unwrap_or_default(),
        ..plan
    };
    let (inventory, seen) = PostWaterGame::inventory(declared, plan);
    let scene = replaces.map_or_else(quad_scene, |_| water_scene());
    let rig = Rig::hdr(&scene, SIZE, |settings| {
        settings.game_passes = inventory;
        if replaces.is_some() {
            settings.water = vec![water()];
        }
    });
    (rig, seen)
}

#[test]
fn the_post_water_depth_is_the_opaque_depth_where_no_pass_wrote_depth() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let all = Reading::All.reads();
    let (mut rig, seen) = build(all, reading(Reading::All), None);
    rig.render().unwrap();
    for at in ALL_PIXELS {
        let [post_device, post_linear, opaque_device, opaque_linear] = probed(&rig, at);
        assert_eq!(
            post_device.to_bits(),
            opaque_device.to_bits(),
            "{at:?} device"
        );
        assert_eq!(
            post_linear.to_bits(),
            opaque_linear.to_bits(),
            "{at:?} linear"
        );
    }
    let [device, linear, ..] = probed(&rig, LEFT_QUAD);
    assert!(
        (device - plane_depth()).abs() < 1e-5,
        "device depth {device}"
    );
    assert_close(linear, 5.0, "the quad, in metres");
    let [device, linear, ..] = probed(&rig, RIGHT_BARE);
    assert_eq!(
        device.to_bits(),
        1.0_f32.to_bits(),
        "no surface leaves the cleared depth"
    );
    assert_close(linear, 10.0, "the far plane, in metres");
    assert_eq!(seen.lock().unwrap().recorded, [READER]);
}

#[test]
fn the_post_water_depth_holds_the_depth_a_pass_after_the_opaque_snapshots_wrote() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (mut rig, seen) = build(
        Reading::All.reads(),
        writing_and_reading(Reading::All),
        None,
    );
    rig.render().unwrap();

    // The writer's half: its depth, over the quad and over the bare ground alike, which the
    // opaque snapshots, taken before it, do not contain.
    for (at, opaque) in [(LEFT_QUAD, plane_depth()), (LEFT_BARE, 1.0)] {
        let [post_device, post_linear, opaque_device, opaque_linear] = probed(&rig, at);
        assert_eq!(
            post_device.to_bits(),
            WRITTEN_DEPTH.to_bits(),
            "{at:?} device"
        );
        assert_close(
            post_linear,
            metres(WRITTEN_DEPTH),
            &format!("{at:?} linear"),
        );
        assert!(
            (opaque_device - opaque).abs() < 1e-5,
            "{at:?} opaque {opaque_device}"
        );
        assert_close(
            opaque_linear,
            metres(opaque),
            &format!("{at:?} opaque linear"),
        );
    }
    // The other half: nothing wrote depth, so the opaque depth stands.
    for at in [RIGHT_QUAD, RIGHT_BARE] {
        let [post_device, post_linear, opaque_device, opaque_linear] = probed(&rig, at);
        assert_eq!(
            post_device.to_bits(),
            opaque_device.to_bits(),
            "{at:?} device"
        );
        assert_eq!(
            post_linear.to_bits(),
            opaque_linear.to_bits(),
            "{at:?} linear"
        );
    }
    assert_eq!(seen.lock().unwrap().recorded, [WRITER, READER]);
}

#[test]
fn water_the_game_draws_with_no_stock_reader_still_produces_the_post_water_depth_it_declares() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    for draws in [StockReplacement::WATER, StockReplacement::NONE] {
        let (mut rig, seen) = build(
            Reading::All.reads(),
            writing_and_reading(Reading::All),
            Some(draws),
        );
        rig.render().unwrap();
        let [post_device, post_linear, ..] = probed(&rig, LEFT_QUAD);
        assert_eq!(
            post_device.to_bits(),
            WRITTEN_DEPTH.to_bits(),
            "{draws:?} device"
        );
        assert_close(
            post_linear,
            metres(WRITTEN_DEPTH),
            &format!("{draws:?} linear"),
        );
        assert_eq!(
            seen.lock().unwrap().lent,
            [Ok(Lent {
                linear_depth: true,
                device_depth: true,
                post_water_depth: true,
                post_water_device_depth: true,
            })],
            "{draws:?}"
        );

        // Bare ground under the water body, outside the writer's half: the stock water pass
        // writes the surface's depth, 0.1 m above the ground, and one the game draws writes none.
        let [post_device, post_linear, opaque_device, opaque_linear] = probed(&rig, RIGHT_WATER);
        assert_eq!(
            opaque_device.to_bits(),
            1.0_f32.to_bits(),
            "{draws:?} opaque device"
        );
        assert_close(opaque_linear, 10.0, &format!("{draws:?} opaque linear"));
        let surface = if draws.water { 10.0 } else { 5.1 };
        assert_close(
            post_linear,
            surface,
            &format!("{draws:?} post-water linear"),
        );
        assert!(
            (post_device - (surface - 0.1) / 9.9).abs() < 1e-5,
            "{draws:?} post-water device depth {post_device}"
        );
    }
}

#[test]
fn water_the_game_draws_allocates_no_post_water_depth_for_a_game_that_does_not_declare_it() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (mut rig, seen) = build(
        Reading::Opaque.reads(),
        reading(Reading::Opaque),
        Some(StockReplacement::WATER),
    );
    rig.render().unwrap();
    assert_eq!(
        seen.lock().unwrap().lent,
        [Ok(Lent {
            linear_depth: true,
            device_depth: true,
            post_water_depth: false,
            post_water_device_depth: false,
        })],
        "nothing plans the post-water depth for a game that reads only the opaque depths"
    );
    assert_eq!(
        rig.renderer.snapshot_statistics(),
        SnapshotStatistics {
            slots: 3,
            device_depth_slots: 3,
            post_water_device_depth_slots: 0,
        }
    );

    // The resident water holds the snapshot banks whoever draws it, and nothing adds a device
    // depth to them but a game that reads one.
    let rig = rig_declaring_water();
    assert_eq!(
        rig.renderer.snapshot_statistics(),
        SnapshotStatistics {
            slots: 3,
            device_depth_slots: 0,
            post_water_device_depth_slots: 0,
        },
    );
}

/// A rig whose only game draws the water and reads nothing.
fn rig_declaring_water() -> Rig {
    Rig::hdr(&water_scene(), SIZE, |settings| {
        settings.water = vec![water()];
        settings.game_passes = declaring(StockReplacement::WATER);
    })
}

#[test]
fn the_device_companion_is_allocated_only_for_a_game_that_reads_it() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (mut rig, seen) = build(
        Reading::Linear.reads(),
        reading(Reading::Linear),
        Some(StockReplacement::WATER),
    );
    assert_eq!(
        rig.renderer.snapshot_statistics(),
        SnapshotStatistics {
            slots: 3,
            device_depth_slots: 0,
            post_water_device_depth_slots: 0,
        },
        "the linear post-water depth needs no companion"
    );
    rig.render().unwrap();
    let [_, linear, ..] = probed(&rig, LEFT_QUAD);
    assert_close(linear, 5.0, "the quad, in metres");
    assert_eq!(
        seen.lock().unwrap().lent,
        [Ok(Lent {
            linear_depth: false,
            device_depth: false,
            post_water_depth: true,
            post_water_device_depth: false,
        })]
    );

    let (mut rig, _) = rig_reading_device_depth();
    let owned = SnapshotStatistics {
        slots: 3,
        device_depth_slots: 0,
        post_water_device_depth_slots: 3,
    };
    assert_eq!(
        rig.renderer.snapshot_statistics(),
        owned,
        "owned when built"
    );
    rig.render().unwrap();
    assert_eq!(rig.renderer.snapshot_statistics(), owned);
    let mut larger = rig.renderer.configuration().clone();
    larger.frame = FrameConfig::new([128, 96], larger.frame.format, 1);
    rig.renderer.reconfigure(&rig.context, larger).unwrap();
    assert_eq!(
        rig.renderer.snapshot_statistics(),
        owned,
        "owned after a replacement"
    );
}

fn rig_reading_device_depth() -> (Rig, Shared) {
    let reads = SnapshotReads {
        post_water_depth: PostWaterDepth::LinearAndDevice,
        ..SnapshotReads::NONE
    };
    build(reads, reading(Reading::Linear), None)
}

#[test]
fn the_allocation_budget_charges_the_post_water_device_depth_a_game_declares() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let resident = |declared| build(declared, Plan::default(), None).0.resident_bytes();
    let texels = u64::from(SIZE[0] * SIZE[1]);
    let linear = resident(Reading::Linear.reads());
    let device = resident(SnapshotReads {
        post_water_depth: PostWaterDepth::LinearAndDevice,
        ..Reading::Linear.reads()
    });
    assert!(
        device >= linear + 3 * texels * 4,
        "three R32Float slots are reserved: {device} over {linear}"
    );
}

#[test]
fn three_shadowed_same_frame_captures_lend_the_post_water_depth_like_the_unshadowed_frame() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let plan = || writing_and_reading(Reading::All);
    let (inventory, _) = PostWaterGame::inventory(Reading::All.reads(), plan());
    let mut unshadowed = Rig::new(&quad_scene(), SIZE, |settings| {
        settings.game_passes = inventory;
    });
    unshadowed.render().unwrap();
    let expected = whole_probe(&unshadowed);
    assert_eq!(
        probed(&unshadowed, LEFT_QUAD)[0].to_bits(),
        WRITTEN_DEPTH.to_bits()
    );

    let (inventory, seen) = PostWaterGame::inventory(Reading::All.reads(), plan());
    let (mut rig, mut scene) = shadowed(inventory);
    for frame in 0..3 {
        let image = capture(&mut rig, &mut scene, frame);
        assert_eq!(
            Rig::pixel(&image, LEFT_QUAD.0, LEFT_QUAD.1),
            [255, 0, 255, 255],
            "frame {frame}: the writer's half"
        );
        assert!(
            whole_probe(&rig) == expected,
            "frame {frame}: the depths the game sampled are the unshadowed frame's"
        );
    }
    let seen = seen.lock().unwrap();
    assert_eq!(seen.recorded, [WRITER, READER].repeat(3));
    assert!(
        seen.lent.iter().all(Result::is_ok),
        "every frame lent its snapshots"
    );
}

#[test]
fn a_pass_before_the_transparent_passes_cannot_read_the_post_water_depth() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let plan = Plan {
        early_reader: true,
        ..reading(Reading::Linear)
    };
    let (mut rig, seen) = build(Reading::All.reads(), plan, None);
    let error = rig.render().unwrap_err();
    assert!(
        matches!(error, FrameError::MissingProducer(resource) if resource.name == "post-water depth"),
        "{error:?}"
    );
    assert!(!seen.lock().unwrap().recorded.contains(&EARLY_READER));
}

#[test]
fn a_pass_that_reads_a_post_water_depth_its_factory_did_not_declare_is_refused_by_name() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    for (declared, read, missing) in [
        (
            SnapshotReads::NONE,
            Reading::Linear,
            "post_water_depth: Linear",
        ),
        (
            Reading::Linear.reads(),
            Reading::All,
            "post_water_depth: LinearAndDevice",
        ),
    ] {
        let (mut rig, _) = build(declared, reading(read), None);
        let error = rig.render().unwrap_err();
        let FrameError::FeaturePreparation { feature, reason } = error else {
            panic!("expected a refusal by name, got {error:?}");
        };
        assert_eq!(feature, "frame snapshots");
        assert!(reason.contains(missing), "{reason}");
        assert!(reason.contains("snapshot_reads"), "{reason}");
    }
}

#[test]
fn a_game_that_declares_the_post_water_depth_but_has_no_reading_pass_is_not_prepared() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (mut rig, seen) = build(Reading::All.reads(), Plan::default(), None);
    rig.render().unwrap();
    assert_eq!(
        seen.lock().unwrap().lent,
        [Err(FrameSnapshotError::NotPrepared)]
    );
}
