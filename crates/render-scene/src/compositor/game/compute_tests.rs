//! A game's compute passes are planned like its render passes: placed by their `GamePlace` and
//! occurrence, ordered with every other pass of the frame and validated with them, so a read
//! needs an earlier writer and a level the compute pass writes is read only after it. Targets are
//! made on wgpu's no-op device.
use super::GameComputeSpec;
use crate::compositor::forward_order::forward_order;
use crate::compositor::game::{
    ColorOutput, GamePassSpec, GameTargets, SnapshotReads, TargetDeclaration, TargetExtent,
    TargetStore, reconcile_loads,
};
use std::sync::Arc;
use worldrunner_render_post::{
    AttachmentDeclaration, AttachmentLoad, FeatureDeclaration, FeaturePass, FrameError, GamePlace,
    PassDeclaration, PassEntry, PassSlot, ResourceRef, StorageWrite, validate_frame,
};

const FLOAT: wgpu::TextureFormat = wgpu::TextureFormat::R32Float;
const SIZE: [u32; 2] = [8, 8];

fn seed() -> TargetDeclaration {
    TargetDeclaration::new("compute.seed", FLOAT, TargetExtent::Fixed(SIZE))
}

fn chain() -> TargetDeclaration {
    TargetDeclaration::new("compute.chain", FLOAT, TargetExtent::Fixed(SIZE))
        .mips(3)
        .with_usage(wgpu::TextureUsages::STORAGE_BINDING)
}

fn plain() -> TargetDeclaration {
    TargetDeclaration::new("compute.plain", FLOAT, TargetExtent::Fixed(SIZE))
}

fn digest() -> TargetDeclaration {
    TargetDeclaration::new("compute.digest", FLOAT, TargetExtent::Fixed([3, 1]))
}

fn targets() -> GameTargets {
    let (device, _queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let store = TargetStore::build(&device, None, &[seed(), chain(), plain(), digest()], SIZE);
    GameTargets::new(Arc::new(store)).with_snapshot_reads(SnapshotReads {
        color: true,
        ..SnapshotReads::NONE
    })
}

fn opaque() -> PassDeclaration {
    let attachment = |name, format, load| AttachmentDeclaration {
        resource: ResourceRef::new(name),
        format,
        size: SIZE,
        samples: 1,
        load,
        store: wgpu::StoreOp::Store,
        read_only: false,
    };
    let mut pass = PassDeclaration::new(PassSlot::Opaque);
    pass.color = Some(attachment(
        "scene color",
        wgpu::TextureFormat::Rgba8UnormSrgb,
        AttachmentLoad::ClearColor(wgpu::Color::BLACK),
    ));
    pass.depth = Some(attachment(
        "scene depth",
        wgpu::TextureFormat::Depth32Float,
        AttachmentLoad::ClearDepth(1.0),
    ));
    pass
}

/// The seed writer at `BeforeOpaque`.
fn writer() -> GamePassSpec {
    GamePassSpec::new(GamePlace::BeforeOpaque, 0).writing([ColorOutput::Target(seed().layer(0))])
}

/// The compute pass at `AfterOpaque`: reads the seed, writes every mip of the chain.
fn compute() -> GameComputeSpec {
    GameComputeSpec::new(GamePlace::AfterOpaque, 0)
        .reading([seed().layer(0)])
        .writing_mips(chain().layer_mips(0))
}

/// The digest at `AfterOpaqueSnapshots`: samples every mip of the chain.
fn reader() -> GamePassSpec {
    GamePassSpec::new(GamePlace::AfterOpaqueSnapshots, 0)
        .writing([ColorOutput::Target(digest().layer(0))])
        .reading([chain().layer(0)])
}

/// What a game plans: its render passes and compute passes, in the order it plans them.
enum Planned {
    Render(GamePassSpec),
    Compute(GameComputeSpec),
}

/// Plans `game` after the stock Opaque, then orders and validates the frame as the stock plan
/// does, returning the ordered passes and the validation's answer.
fn frame(game: &[Planned]) -> (Vec<PassDeclaration>, Result<(), FrameError>) {
    frame_with(vec![opaque()], game)
}

/// As [`frame`], after the stock passes `stock`.
fn frame_with(
    stock: Vec<PassDeclaration>,
    game: &[Planned],
) -> (Vec<PassDeclaration>, Result<(), FrameError>) {
    let targets = targets();
    let (mut passes, mut views) = (stock, Vec::new());
    let mut declared = Vec::new();
    for planned in game {
        declared.push(match planned {
            Planned::Render(spec) => targets.plan_pass(spec, &mut passes, &mut views),
            Planned::Compute(spec) => targets.plan_compute_pass(spec, &mut passes, &mut views),
        });
    }
    let declared: Result<Vec<FeaturePass>, FrameError> = declared.into_iter().collect();
    let declared = match declared {
        Ok(declared) => declared,
        Err(error) => return (passes, Err(error)),
    };
    forward_order(&mut passes);
    reconcile_loads(&mut passes);
    let features = [FeatureDeclaration {
        passes: declared,
        effects: Vec::new(),
    }];
    let validated = validate_frame(&passes, &features, &[]);
    (passes, validated)
}

fn entries(passes: &[PassDeclaration]) -> Vec<PassEntry> {
    passes.iter().map(PassDeclaration::entry).collect()
}

#[test]
fn a_compute_pass_orders_between_the_places_around_it_and_validates_with_them() {
    let (passes, validated) = frame(&[
        Planned::Render(reader()),
        Planned::Compute(compute()),
        Planned::Render(writer()),
    ]);
    assert_eq!(validated, Ok(()));
    let game = |place| PassEntry::new(PassSlot::Game(place));
    assert_eq!(
        entries(&passes),
        [
            game(GamePlace::BeforeOpaque),
            PassEntry::new(PassSlot::Opaque),
            game(GamePlace::AfterOpaque),
            game(GamePlace::AfterOpaqueSnapshots),
        ]
    );
    let planned = &passes[2];
    assert!(planned.color.is_none() && planned.depth.is_none());
    assert_eq!(
        planned.storage_writes(),
        [
            StorageWrite::new(chain().mip(0, 0), FLOAT, [8, 8]),
            StorageWrite::new(chain().mip(0, 1), FLOAT, [4, 4]),
            StorageWrite::new(chain().mip(0, 2), FLOAT, [2, 2]),
        ]
    );
}

#[test]
fn a_compute_pass_that_reads_a_target_no_pass_wrote_is_missing_its_producer() {
    let (_, validated) = frame(&[Planned::Compute(compute())]);
    assert_eq!(validated, Err(FrameError::MissingProducer(seed().layer(0))));
}

#[test]
fn a_level_a_compute_pass_writes_is_read_after_it_and_not_before() {
    let early = GamePassSpec::new(GamePlace::BeforeOpaque, 1)
        .writing([ColorOutput::Target(digest().layer(0))])
        .reading_mips([chain().mip(0, 2)]);
    let (_, validated) = frame(&[
        Planned::Render(writer()),
        Planned::Compute(compute()),
        Planned::Render(early),
    ]);
    assert_eq!(
        validated,
        Err(FrameError::MissingProducer(chain().mip(0, 2)))
    );
}

#[test]
fn a_compute_pass_and_a_render_pass_share_the_occurrences_of_their_place() {
    let clash = GamePassSpec::new(GamePlace::AfterOpaque, 0)
        .writing([ColorOutput::Target(digest().layer(0))]);
    let (_, validated) = frame(&[
        Planned::Render(writer()),
        Planned::Compute(compute()),
        Planned::Render(clash),
    ]);
    assert_eq!(
        validated,
        Err(FrameError::DuplicatePass(PassEntry::new(PassSlot::Game(
            GamePlace::AfterOpaque
        ))))
    );
}

#[test]
fn a_compute_pass_may_read_the_snapshots_its_factory_declared_and_no_others() {
    let reads = GameComputeSpec::new(GamePlace::AfterOpaqueSnapshots, 1)
        .writing_mips([chain().mip(0, 0)])
        .reading_snapshots(SnapshotReads {
            color: true,
            ..SnapshotReads::NONE
        });
    let targets = targets();
    let planned = targets
        .plan_compute_pass(&reads, &mut vec![opaque()], &mut Vec::new())
        .unwrap();
    assert_eq!(planned.reads, [ResourceRef::new("opaque snapshot color")]);
    let more = reads.reading_snapshots(SnapshotReads::ALL);
    let mut passes = vec![opaque()];
    let error = targets
        .plan_compute_pass(&more, &mut passes, &mut Vec::new())
        .unwrap_err();
    assert!(
        matches!(
            error,
            FrameError::FeaturePreparation {
                feature: "frame snapshots",
                ..
            }
        ),
        "{error:?}"
    );
    assert_eq!(passes.len(), 1, "nothing was planned");
}

/// Plans `spec` alone and returns the refusal, checking that nothing was planned.
fn refusal(spec: &GameComputeSpec) -> FrameError {
    let (mut passes, mut views) = (vec![opaque()], Vec::new());
    let error = targets()
        .plan_compute_pass(spec, &mut passes, &mut views)
        .unwrap_err();
    assert_eq!(passes.len(), 1, "nothing was planned for {error:?}");
    assert!(views.is_empty(), "no view was added for {error:?}");
    error
}

#[test]
fn a_compute_pass_after_the_depth_prepass_is_refused() {
    let spec =
        GameComputeSpec::new(GamePlace::AfterDepthPrepass, 0).writing_mips([chain().mip(0, 0)]);
    assert!(matches!(refusal(&spec), FrameError::UnsupportedEffect(_)));
}

#[test]
fn a_write_of_an_undeclared_level_or_a_target_without_storage_is_refused_by_name() {
    let missing = ResourceRef::new("compute.missing");
    let spec = |write| GameComputeSpec::new(GamePlace::AfterOpaque, 0).writing_mips([write]);
    assert_eq!(refusal(&spec(missing)), FrameError::AttachmentView(missing));
    assert_eq!(
        refusal(&spec(chain().mip(0, 3))),
        FrameError::AttachmentView(chain().mip(0, 3))
    );
    assert_eq!(
        refusal(&spec(plain().layer(0))),
        FrameError::AttachmentView(plain().layer(0))
    );
}

#[test]
fn a_level_written_twice_or_read_while_written_is_refused() {
    let twice = GameComputeSpec::new(GamePlace::AfterOpaque, 0)
        .writing_mips([chain().mip(0, 1), chain().mip(0, 1)]);
    assert_eq!(
        refusal(&twice),
        FrameError::AttachmentHazard(chain().mip(0, 1))
    );
    let read = GameComputeSpec::new(GamePlace::AfterOpaque, 0)
        .reading([chain().layer(0)])
        .writing_mips([chain().mip(0, 1)]);
    assert_eq!(
        refusal(&read),
        FrameError::AttachmentHazard(chain().mip(0, 1))
    );
}

/// The stock pass that writes the post-water depth, which comes after the transparent passes.
fn after_water_depth() -> PassDeclaration {
    let mut pass = PassDeclaration::new(PassSlot::AfterWaterDepth);
    pass.color = Some(AttachmentDeclaration {
        resource: ResourceRef::new("post-water depth"),
        format: FLOAT,
        size: SIZE,
        samples: 1,
        load: AttachmentLoad::ClearColor(wgpu::Color::TRANSPARENT),
        store: wgpu::StoreOp::Store,
        read_only: false,
    });
    pass
}

#[test]
fn a_compute_pass_samples_a_stock_product_only_from_a_place_after_its_producer() {
    let sampling = |place| {
        GameComputeSpec::new(place, 0)
            .sampling_frame([ResourceRef::new("post-water depth")])
            .writing_mips([chain().mip(0, 0)])
    };
    let stock = || vec![opaque(), after_water_depth()];
    let (_, early) = frame_with(
        stock(),
        &[Planned::Compute(sampling(GamePlace::AfterOpaque))],
    );
    assert_eq!(
        early,
        Err(FrameError::MissingProducer(ResourceRef::new(
            "post-water depth"
        )))
    );
    let (passes, late) = frame_with(
        stock(),
        &[Planned::Compute(sampling(GamePlace::AfterTransparent))],
    );
    assert_eq!(late, Ok(()));
    assert_eq!(
        passes.last().map(PassDeclaration::entry),
        Some(PassEntry::new(PassSlot::Game(GamePlace::AfterTransparent))),
        "the compute pass runs after the pass that produces what it samples"
    );
}

#[test]
fn a_render_pass_that_attaches_a_level_a_compute_pass_wrote_loads_it() {
    let overlay = GamePassSpec::new(GamePlace::AfterOpaqueSnapshots, 0)
        .writing([ColorOutput::Target(chain().mip(0, 2))]);
    let (passes, validated) = frame(&[
        Planned::Render(writer()),
        Planned::Compute(compute()),
        Planned::Render(overlay),
    ]);
    assert_eq!(
        validated,
        Ok(()),
        "the attachment keeps what the kernels wrote"
    );
    let overlay = passes
        .iter()
        .find(|pass| pass.slot == PassSlot::Game(GamePlace::AfterOpaqueSnapshots))
        .and_then(|pass| pass.color.as_ref())
        .expect("the overlay attaches a colour");
    assert_eq!(overlay.load, AttachmentLoad::Load);
}
