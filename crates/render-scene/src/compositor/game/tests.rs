//! The seam's pure rules: which attachments a pass is given, and the depth rules a draw into it
//! follows. The frames that run them are in `tests/game_passes.rs`.
use super::{
    ColorOutput, GamePassSpec, GameTargets, PassTarget, PostWaterDepth, SnapshotReads,
    reconcile_loads,
};
use std::sync::Arc;
use wgpu::{CompareFunction, StencilOperation, TextureFormat};
use worldrunner_render_post::{
    AttachmentDeclaration, AttachmentLoad, FrameError, GamePlace, PassDeclaration, PassEntry,
    PassSlot, ResourceRef,
};
use worldrunner_render_program::runtime::TargetSignature;
use worldrunner_render_program::state::RasterState;

const SIZE: [u32; 2] = [8, 8];
const COLOR: TextureFormat = TextureFormat::Rgba8UnormSrgb;
const COMBINED: TextureFormat = TextureFormat::Depth32FloatStencil8;

fn raster() -> RasterState {
    RasterState {
        cull: None,
        front_face: wgpu::FrontFace::Ccw,
        alpha_to_coverage: false,
    }
}

fn attachment(
    name: &'static str,
    format: TextureFormat,
    load: AttachmentLoad,
    read_only: bool,
) -> AttachmentDeclaration {
    AttachmentDeclaration {
        resource: ResourceRef::new(name),
        format,
        size: SIZE,
        samples: 1,
        load,
        store: wgpu::StoreOp::Store,
        read_only,
    }
}

fn clear_color() -> AttachmentLoad {
    AttachmentLoad::ClearColor(wgpu::Color::BLACK)
}

fn depth(load: AttachmentLoad, read_only: bool) -> AttachmentDeclaration {
    attachment("scene depth", COMBINED, load, read_only)
}

fn pass(
    slot: PassSlot,
    color: Option<AttachmentDeclaration>,
    depth: Option<AttachmentDeclaration>,
) -> PassDeclaration {
    let mut pass = PassDeclaration::new(slot);
    pass.color = color;
    pass.depth = depth;
    pass
}

fn opaque(load: AttachmentLoad) -> PassDeclaration {
    pass(
        PassSlot::Opaque,
        Some(attachment("scene color", COLOR, clear_color(), false)),
        Some(depth(load, false)),
    )
}

fn prepass() -> PassDeclaration {
    pass(
        PassSlot::DepthNormalPrepass,
        Some(attachment(
            "prepass normals",
            TextureFormat::Rgba16Float,
            clear_color(),
            false,
        )),
        Some(depth(AttachmentLoad::ClearDepth(1.0), false)),
    )
}

fn game(place: GamePlace, occurrence: u32) -> PassDeclaration {
    PassDeclaration::nth(PassSlot::Game(place), occurrence)
}

fn target(passes: &[PassDeclaration], entry: PassEntry) -> PassTarget {
    PassTarget::in_plan(passes, entry).unwrap()
}

#[test]
fn a_forward_opaque_pass_tests_less_equal_and_writes_depth_without_stencil() {
    let passes = [opaque(AttachmentLoad::ClearDepth(1.0))];
    let target = target(&passes, PassEntry::new(PassSlot::Opaque));
    assert_eq!(
        target.signature().unwrap(),
        TargetSignature::attachments(&[COLOR], Some(COMBINED), 1).unwrap()
    );
    let state = target.render_state(raster());
    let depth = state.depth.unwrap();
    assert_eq!(depth.test, CompareFunction::LessEqual);
    assert!(depth.write);
    assert!(state.stencil.is_none());
    assert_eq!(state.colors.len(), 1);
}

#[test]
fn an_opaque_pass_that_loads_a_prepass_depth_tests_equal_and_writes_none() {
    let passes = [prepass(), opaque(AttachmentLoad::Load)];
    let state = target(&passes, PassEntry::new(PassSlot::Opaque)).render_state(raster());
    let depth = state.depth.unwrap();
    assert_eq!(depth.test, CompareFunction::Equal);
    assert!(!depth.write);
    let stencil = state.stencil.unwrap();
    assert_eq!(stencil.ops.front.compare, CompareFunction::Equal);
    assert_eq!(stencil.ops.write_mask, 0);
}

#[test]
fn a_prepass_a_later_opaque_reuses_marks_the_stencil_and_one_it_does_not_reuse_does_not() {
    let reused = [prepass(), opaque(AttachmentLoad::Load)];
    let state =
        target(&reused, PassEntry::new(PassSlot::DepthNormalPrepass)).render_state(raster());
    let stencil = state.stencil.unwrap();
    assert_eq!(stencil.ops.front.pass_op, StencilOperation::Replace);
    assert_eq!(stencil.ops.write_mask, 1);
    assert!(state.depth.unwrap().write);

    let alone = [prepass(), opaque(AttachmentLoad::ClearDepth(1.0))];
    let state = target(&alone, PassEntry::new(PassSlot::DepthNormalPrepass)).render_state(raster());
    assert!(state.stencil.is_none());
}

#[test]
fn a_shadow_caster_pass_has_depth_only_attachments() {
    let caster = pass(
        PassSlot::ShadowCasters(1),
        None,
        Some(attachment(
            "shadow map",
            TextureFormat::Depth32Float,
            AttachmentLoad::ClearDepth(1.0),
            false,
        )),
    );
    let passes = [caster];
    let target = target(&passes, PassEntry::new(PassSlot::ShadowCasters(1)));
    assert_eq!(
        target.signature().unwrap(),
        TargetSignature::depth_attachment(TextureFormat::Depth32Float, 1).unwrap()
    );
    let state = target.render_state(raster());
    assert!(state.colors.is_empty());
    assert_eq!(state.depth.unwrap().test, CompareFunction::LessEqual);
}

#[test]
fn a_game_pass_tests_less_equal_and_writes_unless_it_attaches_depth_read_only() {
    let mut writer = game(GamePlace::BeforeOpaque, 0);
    writer.color = Some(attachment("g0", COLOR, clear_color(), false));
    writer.depth = Some(depth(AttachmentLoad::ClearDepth(1.0), false));
    let mut tester = game(GamePlace::AfterOpaque, 0);
    tester.color = Some(attachment(
        "scene color",
        COLOR,
        AttachmentLoad::Load,
        false,
    ));
    tester.depth = Some(depth(AttachmentLoad::Load, true));
    let mut colour_only = game(GamePlace::AfterTransparent, 0);
    colour_only.color = Some(attachment(
        "scene color",
        COLOR,
        AttachmentLoad::Load,
        false,
    ));
    let passes = [writer, opaque(AttachmentLoad::Load), tester, colour_only];

    let state = target(
        &passes,
        PassEntry::nth(PassSlot::Game(GamePlace::BeforeOpaque), 0),
    )
    .render_state(raster());
    let depth = state.depth.unwrap();
    assert_eq!(depth.test, CompareFunction::LessEqual);
    assert!(depth.write);

    let read_only = target(
        &passes,
        PassEntry::nth(PassSlot::Game(GamePlace::AfterOpaque), 0),
    );
    assert!(!read_only.writes_depth());
    let depth = read_only.render_state(raster()).depth.unwrap();
    assert_eq!(depth.test, CompareFunction::LessEqual);
    assert!(!depth.write);

    let none = target(
        &passes,
        PassEntry::nth(PassSlot::Game(GamePlace::AfterTransparent), 0),
    )
    .render_state(raster());
    assert!(none.depth.is_none() && none.stencil.is_none());
}

#[test]
fn an_entry_the_plan_lacks_is_refused_by_name() {
    let entry = PassEntry::nth(PassSlot::Game(GamePlace::BeforeOpaque), 3);
    assert_eq!(
        PassTarget::in_plan(&[], entry),
        Err(worldrunner_render_post::FrameError::MissingPass(entry))
    );
}

fn loads(passes: &[PassDeclaration]) -> Vec<Vec<AttachmentLoad>> {
    passes
        .iter()
        .map(|pass| {
            pass.color
                .iter()
                .chain(&pass.additional_colors)
                .chain(&pass.depth)
                .map(|attachment| attachment.load)
                .collect()
        })
        .collect()
}

#[test]
fn a_frame_without_a_game_pass_keeps_every_clear() {
    let mut passes = vec![prepass(), opaque(AttachmentLoad::ClearDepth(1.0))];
    let before = loads(&passes);
    reconcile_loads(&mut passes);
    assert_eq!(loads(&passes), before);
}

#[test]
fn opaque_and_later_game_passes_load_what_an_earlier_game_pass_wrote() {
    let mut first = game(GamePlace::BeforeOpaque, 0);
    first.color = Some(attachment("scene color", COLOR, clear_color(), false));
    first.depth = Some(depth(AttachmentLoad::ClearDepth(1.0), false));
    let mut second = game(GamePlace::BeforeOpaque, 1);
    second.color = Some(attachment("scene color", COLOR, clear_color(), false));
    let mut passes = vec![first, second, opaque(AttachmentLoad::ClearDepth(1.0))];
    reconcile_loads(&mut passes);
    let loads = loads(&passes);
    // The first game pass is the first writer, so it clears; every later pass loads.
    assert!(matches!(loads[0][0], AttachmentLoad::ClearColor(_)));
    assert!(matches!(loads[0][1], AttachmentLoad::ClearDepth(_)));
    assert_eq!(loads[1], [AttachmentLoad::Load]);
    assert_eq!(loads[2], [AttachmentLoad::Load, AttachmentLoad::Load]);
}

#[test]
fn a_game_pass_loads_a_depth_a_stock_prepass_completed() {
    let mut writer = game(GamePlace::BeforeOpaque, 0);
    writer.depth = Some(depth(AttachmentLoad::ClearDepth(1.0), false));
    let mut passes = vec![prepass(), writer, opaque(AttachmentLoad::Load)];
    reconcile_loads(&mut passes);
    assert_eq!(loads(&passes)[1], [AttachmentLoad::Load]);
}

#[path = "before_post_tests.rs"]
mod before_post_tests;
#[path = "prefix_tests.rs"]
mod prefix_tests;
const COLOR_AND_DEVICE_DEPTH: SnapshotReads = SnapshotReads {
    color: true,
    device_depth: true,
    ..SnapshotReads::NONE
};

#[test]
fn a_pass_after_the_opaque_snapshots_reads_exactly_the_stock_snapshots_it_declares() {
    let targets = GameTargets::new(Arc::default()).with_snapshot_reads(SnapshotReads::ALL);
    let mut passes = vec![opaque(AttachmentLoad::ClearDepth(1.0))];
    let mut views = Vec::new();
    let spec = GamePassSpec::new(GamePlace::AfterOpaqueSnapshots, 0)
        .writing([ColorOutput::Frame])
        .reading_snapshots(COLOR_AND_DEVICE_DEPTH);
    let planned = targets.plan_pass(&spec, &mut passes, &mut views).unwrap();
    assert_eq!(
        planned.reads,
        [
            ResourceRef::new("opaque snapshot color"),
            ResourceRef::new("opaque snapshot device depth"),
        ]
    );
    assert!(
        views.is_empty(),
        "the frame's snapshot planning supplies the views of the stock snapshots"
    );
    assert_eq!(
        planned.slot,
        PassSlot::Game(GamePlace::AfterOpaqueSnapshots)
    );
}

#[test]
fn a_pass_that_reads_a_snapshot_its_factory_did_not_declare_is_refused_naming_both() {
    let targets = GameTargets::new(Arc::default()).with_snapshot_reads(SnapshotReads {
        color: true,
        ..SnapshotReads::NONE
    });
    let mut passes = vec![opaque(AttachmentLoad::ClearDepth(1.0))];
    let spec = GamePassSpec::new(GamePlace::AfterOpaqueSnapshots, 0)
        .writing([ColorOutput::Frame])
        .reading_snapshots(COLOR_AND_DEVICE_DEPTH);
    let error = targets
        .plan_pass(&spec, &mut passes, &mut Vec::new())
        .unwrap_err();
    let FrameError::FeaturePreparation { feature, reason } = error else {
        panic!("expected a named refusal, got {error:?}");
    };
    assert_eq!(feature, "frame snapshots");
    assert!(reason.contains("device_depth: true"), "{reason}");
    assert!(reason.contains("snapshot_reads"), "{reason}");
    assert_eq!(passes.len(), 1, "nothing was planned");
}

#[test]
fn a_pass_reads_the_post_water_depth_through_both_stock_resources_it_declares() {
    let targets = GameTargets::new(Arc::default()).with_snapshot_reads(SnapshotReads::ALL);
    let mut passes = vec![opaque(AttachmentLoad::ClearDepth(1.0))];
    let spec = GamePassSpec::new(GamePlace::AfterTransparent, 0)
        .writing([ColorOutput::Frame])
        .reading_snapshots(SnapshotReads {
            post_water_depth: PostWaterDepth::LinearAndDevice,
            ..SnapshotReads::NONE
        });
    let planned = targets
        .plan_pass(&spec, &mut passes, &mut Vec::new())
        .unwrap();
    assert_eq!(
        planned.reads,
        [
            ResourceRef::new("post-water depth"),
            ResourceRef::new("post-water device depth"),
        ]
    );
}

#[test]
fn the_post_water_reads_are_snapshot_reads_a_factory_must_declare() {
    let reading = |post_water_depth| SnapshotReads {
        post_water_depth,
        ..SnapshotReads::NONE
    };
    let linear = reading(PostWaterDepth::Linear);
    let both = reading(PostWaterDepth::LinearAndDevice);
    assert!(linear.any() && both.any() && !SnapshotReads::NONE.any());
    assert!(linear.covers(linear) && both.covers(linear) && both.covers(both));
    assert!(
        !linear.covers(both),
        "the device depth is a read of its own"
    );
    assert!(!SnapshotReads::NONE.covers(linear));
    assert_eq!(linear.union(both), both);
    assert_eq!(both.union(linear), both);
    assert!(SnapshotReads::ALL.covers(both));
}

mod resize_keeps_device_depth {
    use super::super::{GameBuild, GameFeatureFactory, GamePasses, SnapshotReads};
    use crate::compositor::{FrameRenderer, SnapshotStatistics};
    use crate::device::GpuContext;
    use std::sync::Arc;
    use worldrunner_render_post::{
        BuildContext, FeatureDeclaration, FrameConfig, FrameError, FrameFeature, PreparedFeature,
    };

    struct DepthReader;
    struct Idle;

    impl FrameFeature for Idle {
        fn declare(&self) -> FeatureDeclaration {
            FeatureDeclaration {
                passes: Vec::new(),
                effects: Vec::new(),
            }
        }
        fn build(&self, _: &BuildContext<'_>) -> Result<Box<dyn PreparedFeature>, FrameError> {
            Err(FrameError::UnsupportedEffect("not drawn"))
        }
    }

    impl GameFeatureFactory for DepthReader {
        fn name(&self) -> &'static str {
            "depth reader"
        }
        fn snapshot_reads(&self) -> SnapshotReads {
            SnapshotReads {
                device_depth: true,
                ..SnapshotReads::NONE
            }
        }
        fn build(&self, _: &GameBuild<'_>) -> Result<Box<dyn FrameFeature>, FrameError> {
            Ok(Box::new(Idle))
        }
    }

    #[test]
    fn a_resize_keeps_the_device_depth_a_game_reads() {
        let _session = crate::test_support::SceneTestSession::acquire();
        let context = GpuContext::shared_for_test().unwrap();
        let device = context.device();
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let mut renderer =
            FrameRenderer::with_output(device, FrameConfig::new([8, 8], format, 1), format)
                .unwrap();
        GamePasses::new()
            .with(Arc::new(DepthReader))
            .install(&mut renderer, &context, None)
            .unwrap();
        renderer.ensure_game_snapshots(device);
        let owned = SnapshotStatistics {
            slots: 3,
            device_depth_slots: 3,
            post_water_device_depth_slots: 0,
        };
        assert_eq!(renderer.snapshot_statistics(), owned);
        assert!(renderer.resize(device, [16, 16]).unwrap());
        assert_eq!(renderer.snapshot_statistics(), owned);
    }
}
