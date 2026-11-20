//! The contracts of a factory's declared resources that a game relies on across frames and
//! generations: what a write may touch, what a replacement releases, what the budget counts and
//! what the device refuses, and that a shadowed same-frame capture draws with them.
use super::super::fixture::{Rig, SIZE, quad_scene};
use super::super::shadowed::{capture, shadowed};
use super::{
    BUDGET, ClearKept, Controls, DECLARED_GREEN, KEPT, ResourceGame, SCENE_BLUE, Seen, kept,
    linear, near, output, semantic,
};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use worldrunner_render_plan::TextureHandle;
use worldrunner_render_scene::{
    compositor::{
        FrameInputError, FrameSceneTextureError, GameBufferDeclaration, GameBufferUsage,
        GamePasses, GameResourceError, GameResources, GameTextureDeclaration,
    },
    wgpu,
};

/// A rig whose renderer carries one resource game, the inventory it was built from and the
/// controls that steer the game.
fn single_game() -> (Rig, GamePasses, Arc<Controls>, Seen) {
    let seen = Seen::default();
    let controls = Arc::<Controls>::default();
    let inventory = GamePasses::new().with(Arc::new(ResourceGame {
        out: "resources.first",
        occurrence: 0,
        seen: seen.clone(),
        controls: controls.clone(),
    }));
    let configured = inventory.clone();
    let rig = Rig::new(&quad_scene(), SIZE, move |settings| {
        settings.game_passes = configured;
    });
    (rig, inventory, controls, seen)
}

fn census_total(rig: &Rig) -> u64 {
    rig.context.submission().census().snapshot().total()
}

/// The first game's resources of the generation that built last.
fn first_resources() -> GameResources {
    kept("resources.first", |kept| kept.resources.clone().unwrap())
}

/// Lets go of what the test itself holds of the generation, so only the renderer's own
/// references keep it alive.
fn forget_generation() {
    kept("resources.first", |kept| {
        kept.resources = None;
        kept.scene = None;
        kept.snapshots.clear();
    });
}

#[test]
fn a_shadowed_same_frame_capture_draws_with_a_buffer_rewritten_every_frame_and_a_sampled_texture() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    KEPT.with_borrow_mut(Vec::clear);
    let _clear = ClearKept;
    let (mut rig, mut scene) = shadowed(GamePasses::new().with(Arc::new(ResourceGame {
        out: "resources.first",
        occurrence: 0,
        seen: Seen::default(),
        controls: Arc::default(),
    })));
    for (frame, red) in [(0_u64, 64_u8), (1, 128), (2, 191)] {
        capture(&mut rig, &mut scene, frame);
        let found = output(&rig, "resources.first");
        assert!(
            near(found, [red, DECLARED_GREEN, linear(SCENE_BLUE), 255]),
            "capture {frame} drew the buffer written in it and the lent scene texel: {found:?}"
        );
    }
}

#[test]
fn a_write_outside_a_buffer_is_refused_by_name_and_a_frame_that_does_not_complete_writes_nothing() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    KEPT.with_borrow_mut(Vec::clear);
    let _clear = ClearKept;
    let (mut rig, _inventory, controls, seen) = single_game();
    rig.render().unwrap();
    controls.probe_writes.store(true, Ordering::Relaxed);
    rig.render().unwrap();
    controls.probe_writes.store(false, Ordering::Relaxed);
    let found = output(&rig, "resources.first");
    assert!(
        near(found, [128, DECLARED_GREEN, linear(SCENE_BLUE), 255]),
        "{found:?}"
    );
    let refusals = seen.lock().unwrap().clone();
    let tint = "into game buffer resources.tint of 16 bytes";
    assert_eq!(
        refusals[1..],
        [
            format!("write of 20 bytes at 0 {tint}"),
            format!("write of 8 bytes at 12 {tint}"),
            format!("write of 4 bytes at {} {tint}", u64::MAX - 3),
        ],
        "each write outside the buffer was refused by name, and the frame still drew"
    );

    // A frame the feature refuses after its write copies nothing, and neither does one that is
    // cancelled before submission.
    controls.fail_after_write.store(true, Ordering::Relaxed);
    assert!(rig.render().is_err());
    controls.fail_after_write.store(false, Ordering::Relaxed);
    rig.render_cancelled().unwrap();
    controls.skip_write.store(true, Ordering::Relaxed);
    rig.render().unwrap();
    let found = output(&rig, "resources.first");
    assert!(
        near(found, [128, DECLARED_GREEN, linear(SCENE_BLUE), 255]),
        "the buffer still holds the last completed frame's write: {found:?}"
    );
}

#[test]
fn replacing_the_renderer_releases_the_old_resources_and_refuses_their_handles() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    KEPT.with_borrow_mut(Vec::clear);
    let _clear = ClearKept;
    let (mut rig, _inventory, _controls, _seen) = single_game();
    rig.render().unwrap();
    let old_tint = kept("resources.first", |kept| kept.tint.unwrap());
    forget_generation();
    rig.context
        .device()
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    rig.renderer.collect();
    let baseline = census_total(&rig);

    let same = rig.renderer.configuration().clone();
    rig.renderer.reconfigure(&rig.context, same).unwrap();
    rig.render().unwrap();
    let fresh = first_resources();
    assert_eq!(
        fresh.import_buffer(semantic("resources.tint"), old_tint),
        Err(GameResourceError::Foreign("game buffer")),
        "a handle of the replaced generation is refused by name"
    );
    drop(fresh);
    forget_generation();
    rig.context
        .device()
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    rig.renderer.collect();
    assert_eq!(
        census_total(&rig),
        baseline,
        "the replaced generation's declared resources are released, not added to"
    );

    let mut removed = rig.renderer.configuration().clone();
    removed.game_passes = GamePasses::new();
    rig.renderer.reconfigure(&rig.context, removed).unwrap();
    rig.render().unwrap();
    rig.context
        .device()
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    rig.renderer.collect();
    assert!(
        census_total(&rig) + 44 <= baseline,
        "removing the factory releases its 44 declared bytes"
    );
}

#[test]
fn declared_bytes_follow_format_layers_and_depth_and_an_overrun_is_refused_before_allocating() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    KEPT.with_borrow_mut(Vec::clear);
    let _clear = ClearKept;
    let (mut rig, _inventory, _controls, _seen) = single_game();
    rig.render().unwrap();
    let resources = first_resources();
    let declare = |label, format, size, dimension, texels| {
        resources.declare_texture(&GameTextureDeclaration {
            label,
            format,
            size,
            dimension,
            texels,
        })
    };
    let held = (resources.bytes(), census_total(&rig));
    declare(
        "budget.layers",
        wgpu::TextureFormat::Rgba8Unorm,
        [4, 4, 2],
        wgpu::TextureViewDimension::D2Array,
        Some(&[0; 128]),
    )
    .unwrap();
    declare(
        "budget.volume",
        wgpu::TextureFormat::R8Unorm,
        [2, 2, 2],
        wgpu::TextureViewDimension::D3,
        Some(&[0; 8]),
    )
    .unwrap();
    declare(
        "budget.depth",
        wgpu::TextureFormat::Depth32Float,
        [4, 4, 2],
        wgpu::TextureViewDimension::D2Array,
        None,
    )
    .unwrap();
    assert_eq!(resources.bytes(), held.0 + 128 + 8 + 128);
    assert_eq!(census_total(&rig), held.1 + 128 + 8 + 128);

    let held = (resources.bytes(), census_total(&rig));
    let overrun = declare(
        "budget.overrun",
        wgpu::TextureFormat::R8Unorm,
        [256, 256, 1],
        wgpu::TextureViewDimension::D2,
        None,
    );
    assert!(
        matches!(&overrun, Err(GameResourceError::Budget { label, bytes: 65536, .. })
            if label == "budget.overrun"),
        "{overrun:?}"
    );
    assert_eq!(
        (resources.bytes(), census_total(&rig)),
        held,
        "nothing was allocated"
    );
}

#[test]
fn a_texture_the_device_cannot_create_is_refused_by_name_rather_than_by_a_wgpu_panic() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    KEPT.with_borrow_mut(Vec::clear);
    let _clear = ClearKept;
    let (mut rig, _inventory, _controls, _seen) = single_game();
    rig.render().unwrap();
    let resources = first_resources();
    let limits = rig.context.device().limits();
    let attempts = [
        (
            "limits.wide",
            wgpu::TextureFormat::R8Unorm,
            [limits.max_texture_dimension_2d + 1, 1, 1],
            wgpu::TextureViewDimension::D2,
        ),
        (
            "limits.layers",
            wgpu::TextureFormat::R8Unorm,
            [1, 1, limits.max_texture_array_layers + 1],
            wgpu::TextureViewDimension::D2Array,
        ),
        (
            // wgpu creates depth textures in two dimensions only.
            "limits.depth_volume",
            wgpu::TextureFormat::Depth32Float,
            [2, 2, 2],
            wgpu::TextureViewDimension::D3,
        ),
        (
            // The rig's device has no texture compression.
            "limits.compressed",
            wgpu::TextureFormat::Bc1RgbaUnorm,
            [4, 4, 1],
            wgpu::TextureViewDimension::D2,
        ),
    ];
    for (label, format, size, dimension) in attempts {
        let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            resources.declare_texture(&GameTextureDeclaration {
                label,
                format,
                size,
                dimension,
                texels: None,
            })
        }));
        let declared = attempt
            .unwrap_or_else(|_| panic!("{label}: declare_texture panicked instead of refusing"));
        assert!(
            matches!(&declared, Err(GameResourceError::Declaration { label: named, .. })
                if named == label),
            "{label}: expected a refused declaration, got {declared:?}"
        );
    }
}

#[test]
fn a_texture_of_any_byte_count_is_declared_and_held_in_the_runtimes_ledger() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    KEPT.with_borrow_mut(Vec::clear);
    let _clear = ClearKept;
    let (mut rig, _inventory, _controls, _seen) = single_game();
    rig.render().unwrap();
    let resources = first_resources();
    let held = (resources.bytes(), resources.runtime().live_bytes());
    // Three one-byte texels: no buffer rule (a multiple of four bytes) applies to a texture.
    let declared = resources.declare_texture(&GameTextureDeclaration {
        label: "ledger.odd",
        format: wgpu::TextureFormat::R8Unorm,
        size: [3, 1, 1],
        dimension: wgpu::TextureViewDimension::D2,
        texels: Some(&[1, 2, 3]),
    });
    assert!(declared.is_ok(), "{declared:?}");
    assert_eq!(
        (resources.bytes(), resources.runtime().live_bytes()),
        (held.0 + 3, held.1 + 3),
        "the store and the runtime's ledger both count the three bytes"
    );
}

#[test]
fn the_program_runtime_and_the_declared_resources_share_the_factorys_one_budget() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    KEPT.with_borrow_mut(Vec::clear);
    let _clear = ClearKept;
    let (mut rig, _inventory, _controls, _seen) = single_game();
    rig.render().unwrap();
    let resources = first_resources();
    let held = resources.bytes() + resources.runtime().live_bytes();
    let room = (BUDGET - held) & !3;
    resources
        .declare_buffer(&GameBufferDeclaration {
            label: "shared.fill",
            usage: GameBufferUsage::Storage,
            contents: &vec![0; usize::try_from(room - 4096).unwrap()],
        })
        .unwrap();
    let refused = resources
        .runtime()
        .create_buffer("shared.runtime", &vec![0; 8192]);
    assert!(
        refused.is_err(),
        "{} declared bytes and {} the runtime holds pass the budget of {BUDGET}",
        resources.bytes(),
        resources.runtime().live_bytes() + 8192,
    );
}

#[test]
fn lent_scene_textures_are_refused_outside_a_frame_and_for_a_handle_beyond_the_table() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    KEPT.with_borrow_mut(Vec::clear);
    let _clear = ClearKept;
    let (mut rig, _inventory, _controls, _seen) = single_game();
    rig.render().unwrap();
    kept("resources.first", |kept| {
        assert_eq!(
            kept.scene.as_ref().unwrap().current().map(|_| ()),
            Err(FrameInputError::OutsideFrame),
            "after the frame, an earlier frame's textures are never returned"
        );
        assert_eq!(
            kept.snapshots[0]
                .import(semantic("resources.scene"), TextureHandle::from_raw(7))
                .map(|_| ()),
            Err(FrameSceneTextureError::Unknown {
                handle: 7,
                resident: 1
            })
        );
    });
}
