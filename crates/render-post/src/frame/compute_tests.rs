//! Compute passes in the frame's pass list, on wgpu's no-op device: how the frame validates them
//! with the render passes around them, and that the executor records the compute work staged for
//! one at its place among the passes, refusing by name what does not fit the plan.
use std::cell::RefCell;
use std::rc::Rc;

use worldrunner_render_gpu::submission::SubmissionAuthority;
use worldrunner_render_gpu::submission::capture::{
    CaptureError, CaptureWrites, WriteError, WriteKey,
};

use crate::contracts::ContractError;
use crate::{
    AttachmentDeclaration, AttachmentLoad, AttachmentView, BorrowedFeature, CopyOutput,
    DrawContext, FeatureDeclaration, FeaturePass, FrameConfig, FrameError, FramePlan, GamePlace,
    PassDeclaration, PassEntry, PassSlot, PostChain, PrepareContext, PreparedAccess,
    PreparedCommands, PreparedCompute, PreparedFeature, PreparedRefusal, PreparedWrite,
    ResourceRef, Resources, StorageWrite,
};

const SIZE: [u32; 2] = [8, 8];
const FLOAT: wgpu::TextureFormat = wgpu::TextureFormat::R32Float;
const SEED: ResourceRef = ResourceRef::new("seed");
const OUT: ResourceRef = ResourceRef::new("out");
/// A texture a pass may render into and sample, and no kernel write.
const PLAIN: ResourceRef = ResourceRef::new("plain");
const WRITER: PassEntry = PassEntry::nth(PassSlot::Game(GamePlace::BeforeOpaque), 0);
const COMPUTE: PassEntry = PassEntry::nth(PassSlot::Game(GamePlace::AfterOpaque), 0);
const READER: PassEntry = PassEntry::nth(PassSlot::Game(GamePlace::AfterTransparent), 0);

/// Mip `mip` of the three-mip chain kernels write.
const fn chain(mip: u32) -> ResourceRef {
    ResourceRef {
        mip,
        ..ResourceRef::new("chain")
    }
}

fn extent(mip: u32) -> [u32; 2] {
    [SIZE[0] >> mip, SIZE[1] >> mip]
}

type Log = Rc<RefCell<Vec<String>>>;

struct Rig {
    device: wgpu::Device,
    queue: wgpu::Queue,
    seed: wgpu::Texture,
    chain: wgpu::Texture,
    out: wgpu::Texture,
    plain: wgpu::Texture,
}

impl Rig {
    fn new() -> Self {
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let texture = |label, mips, usage| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: SIZE[0],
                    height: SIZE[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: mips,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: FLOAT,
                usage,
                view_formats: &[],
            })
        };
        let attachment =
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
        let storage = wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING;
        Self {
            seed: texture("seed", 1, attachment),
            chain: texture("chain", 3, storage),
            out: texture("out", 1, attachment),
            plain: texture("plain", 1, attachment),
            device,
            queue,
        }
    }

    fn views(&self) -> Vec<AttachmentView> {
        let mut views = vec![
            AttachmentView::new(SEED, &self.seed).unwrap(),
            AttachmentView::new(OUT, &self.out).unwrap(),
            AttachmentView::new(PLAIN, &self.plain).unwrap(),
        ];
        views.extend((0..3).map(|mip| AttachmentView::new(chain(mip), &self.chain).unwrap()));
        views
    }
}

fn color(resource: ResourceRef) -> AttachmentDeclaration {
    AttachmentDeclaration {
        resource,
        format: FLOAT,
        size: SIZE,
        samples: 1,
        load: AttachmentLoad::ClearColor(wgpu::Color::TRANSPARENT),
        store: wgpu::StoreOp::Store,
        read_only: false,
    }
}

fn render(entry: PassEntry, target: ResourceRef) -> PassDeclaration {
    let mut pass = PassDeclaration::nth(entry.slot(), entry.occurrence());
    pass.color = Some(color(target));
    pass
}

fn compute(writes: &[ResourceRef]) -> PassDeclaration {
    let writes = writes
        .iter()
        .map(|resource| StorageWrite::new(*resource, FLOAT, extent(resource.mip)))
        .collect();
    PassDeclaration::compute_pass(COMPUTE.slot(), COMPUTE.occurrence(), writes)
}

fn feature(entry: PassEntry, reads: &[ResourceRef], colour: bool) -> FeatureDeclaration {
    let mut pass = FeaturePass::nth(entry.slot(), entry.occurrence());
    pass.reads = reads.to_vec();
    pass.color_format = colour.then_some(FLOAT);
    FeatureDeclaration {
        passes: vec![pass],
        effects: Vec::new(),
    }
}

/// A render pass writes the seed, a compute pass reads it and writes the chain's three mips, and
/// a render pass samples the chain's last mip.
fn frame() -> (Vec<PassDeclaration>, Vec<FeatureDeclaration>) {
    (
        vec![
            render(WRITER, SEED),
            compute(&[chain(0), chain(1), chain(2)]),
            render(READER, OUT),
        ],
        vec![
            feature(WRITER, &[], true),
            feature(COMPUTE, &[SEED], false),
            feature(READER, &[chain(2)], true),
        ],
    )
}

fn validate(
    rig: &Rig,
    passes: &[PassDeclaration],
    features: &[FeatureDeclaration],
) -> Result<(), FrameError> {
    crate::frame::validate_plan(passes, features, &[], &rig.views(), &[])
}

#[test]
fn a_pass_leaves_written_what_it_attaches_and_what_it_writes_through_storage_images() {
    let (passes, _) = frame();
    assert_eq!(passes[0].written(), [(SEED, true)]);
    assert_eq!(
        passes[1].written(),
        [(chain(0), true), (chain(1), true), (chain(2), true)]
    );
    assert!(passes[0].storage_writes().is_empty());
}

#[test]
fn a_compute_pass_between_two_render_passes_validates() {
    let rig = Rig::new();
    let (passes, features) = frame();
    assert_eq!(validate(&rig, &passes, &features), Ok(()));
}

#[test]
fn a_compute_pass_that_reads_what_no_earlier_pass_wrote_is_missing_its_producer() {
    let rig = Rig::new();
    let (mut passes, features) = frame();
    passes.swap(0, 1);
    assert_eq!(
        validate(&rig, &passes, &features),
        Err(FrameError::MissingProducer(SEED))
    );
}

#[test]
fn a_render_pass_before_the_compute_pass_cannot_read_what_it_writes() {
    let rig = Rig::new();
    let (mut passes, features) = frame();
    passes.swap(1, 2);
    assert_eq!(
        validate(&rig, &passes, &features),
        Err(FrameError::MissingProducer(chain(2)))
    );
}

#[test]
fn a_compute_pass_with_an_attachment_is_refused() {
    let rig = Rig::new();
    let (mut passes, features) = frame();
    passes[1].color = Some(color(PLAIN));
    assert_eq!(
        validate(&rig, &passes, &features),
        Err(FrameError::AttachmentContract(COMPUTE))
    );
}

#[test]
fn a_feature_that_declares_attachment_formats_in_a_compute_pass_is_refused() {
    let rig = Rig::new();
    let (passes, mut features) = frame();
    features[1] = feature(COMPUTE, &[SEED], true);
    assert_eq!(
        validate(&rig, &passes, &features),
        Err(FrameError::AttachmentContract(COMPUTE))
    );
}

#[test]
fn a_compute_pass_that_reads_a_level_it_writes_is_refused() {
    let rig = Rig::new();
    let (passes, mut features) = frame();
    features[1] = feature(COMPUTE, &[SEED, chain(1)], false);
    assert_eq!(
        validate(&rig, &passes, &features),
        Err(FrameError::AttachmentHazard(chain(1)))
    );
}

#[test]
fn a_level_a_compute_pass_names_twice_is_refused() {
    let rig = Rig::new();
    let (mut passes, features) = frame();
    passes[1] = compute(&[chain(0), chain(1), chain(2), chain(1)]);
    assert_eq!(
        validate(&rig, &passes, &features),
        Err(FrameError::AttachmentHazard(chain(1)))
    );
}

#[test]
fn a_storage_write_of_a_texture_without_storage_binding_is_refused() {
    let rig = Rig::new();
    let (mut passes, features) = frame();
    passes[1] = compute(&[chain(0), chain(1), chain(2), PLAIN]);
    assert_eq!(
        validate(&rig, &passes, &features),
        Err(FrameError::AttachmentView(PLAIN))
    );
}

#[test]
fn a_render_pass_cannot_attach_what_a_compute_pass_writes_without_loading_it() {
    let rig = Rig::new();
    let (mut passes, features) = frame();
    passes.push(render(
        PassEntry::nth(PassSlot::Game(GamePlace::AfterTransparent), 1),
        chain(0),
    ));
    assert_eq!(
        validate(&rig, &passes, &features),
        Err(FrameError::AttachmentHazard(chain(0)))
    );
}

/// What the compute feature stages for one item.
#[derive(Clone)]
struct Item {
    entry: PassEntry,
    label: &'static str,
    reads: Vec<ResourceRef>,
    writes: Vec<ResourceRef>,
    /// Whether the commands have a compute form.
    compute: bool,
    /// The levels the commands report writing, when not `writes`.
    reported: Option<Vec<ResourceRef>>,
}

impl Item {
    fn new(label: &'static str, reads: &[ResourceRef], writes: &[ResourceRef]) -> Self {
        Self {
            entry: COMPUTE,
            label,
            reads: reads.to_vec(),
            writes: writes.to_vec(),
            compute: true,
            reported: None,
        }
    }
}

/// Commands that log their recording and report the chain mips they were staged as writing.
struct Logged {
    label: &'static str,
    log: Log,
    compute: bool,
    writes: Vec<PreparedWrite>,
}

impl PreparedCommands for Logged {
    fn writes(&self) -> Vec<PreparedWrite> {
        self.writes.clone()
    }

    fn retain(&self, _encoder: &wgpu::CommandEncoder) -> Result<(), PreparedRefusal> {
        Ok(())
    }

    fn record(
        &self,
        _encoder: &mut wgpu::CommandEncoder,
        label: &str,
    ) -> Result<(), PreparedRefusal> {
        self.log
            .borrow_mut()
            .push(format!("outside a pass: {label}"));
        Ok(())
    }

    fn compute(&self) -> Option<&dyn PreparedCompute> {
        self.compute.then_some(self as &dyn PreparedCompute)
    }
}

impl PreparedCompute for Logged {
    fn record_in_pass(
        &self,
        _pass: &mut wgpu::ComputePass<'_>,
        label: &str,
    ) -> Result<(), PreparedRefusal> {
        assert_eq!(label, self.label);
        self.log.borrow_mut().push(format!("compute: {label}"));
        Ok(())
    }
}

/// A feature that logs its draws, and stages `items` while it prepares.
struct Feature {
    name: &'static str,
    log: Log,
    items: Vec<Item>,
    chain: wgpu::Texture,
}

impl PreparedFeature for Feature {
    fn prepare(&mut self, context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        for item in &self.items {
            let commands = Logged {
                label: item.label,
                log: self.log.clone(),
                compute: item.compute,
                writes: item
                    .reported
                    .as_ref()
                    .unwrap_or(&item.writes)
                    .iter()
                    .map(|resource| PreparedWrite::layer(&self.chain, resource.mip, 0))
                    .collect(),
            };
            let access = PreparedAccess {
                reads: &item.reads,
                writes: &item.writes,
            };
            context.record_in_compute_pass(item.entry, item.label, access, Box::new(commands))?;
        }
        Ok(())
    }

    fn draw(&self, _context: &DrawContext, _pass: &mut wgpu::RenderPass<'_>) {
        self.log.borrow_mut().push(format!("draw: {}", self.name));
    }
}

/// Encodes `frame()` with the compute feature staging `items`, returning what the executor said
/// and the log of what was recorded, in order.
fn run(items: Vec<Item>) -> (Result<(), FrameError>, Vec<String>) {
    let rig = Rig::new();
    let output = wgpu::TextureFormat::Rgba8Unorm;
    let mut chain = PostChain::new(
        &rig.device,
        FrameConfig::new(SIZE, output, 1),
        Resources::default(),
        CopyOutput::new(&rig.device, output),
        crate::SceneDepth::Independent,
    )
    .unwrap();
    let (passes, features) = frame();
    let views = rig.views();
    let log = Log::default();
    let feature = |name, items| Feature {
        name,
        log: log.clone(),
        items,
        chain: rig.chain.clone(),
    };
    let (mut writer, mut stager, mut reader) = (
        feature("writer", Vec::new()),
        feature("stager", items),
        feature("reader", Vec::new()),
    );
    let mut borrowed: Vec<_> = features
        .into_iter()
        .zip([&mut writer, &mut stager, &mut reader])
        .map(|(declaration, prepared)| BorrowedFeature {
            declaration,
            prepared: prepared as &mut dyn PreparedFeature,
        })
        .collect();
    let mut encoder = worldrunner_render_gpu::submission::SubmissionAuthority::new(rig.queue)
        .begin(&rig.device)
        .unwrap();
    let result = chain.encode_geometry(
        &rig.device,
        &mut encoder,
        &FramePlan {
            passes: &passes,
            views: &views,
            requested: &[],
            operations: &[],
        },
        &mut borrowed,
    );
    drop(borrowed);
    let logged = log.borrow().clone();
    (result, logged)
}

/// The three dispatches of the chain: mip 0 from the seed, then each mip from the one before.
fn dispatches() -> Vec<Item> {
    vec![
        Item::new("mip 0", &[SEED], &[chain(0)]),
        Item::new("mip 1", &[chain(0)], &[chain(1)]),
        Item::new("mip 2", &[chain(1)], &[chain(2)]),
    ]
}

#[test]
fn the_work_staged_for_a_compute_pass_is_recorded_between_the_passes_around_it() {
    let (result, log) = run(dispatches());
    assert_eq!(result, Ok(()));
    assert_eq!(
        log,
        [
            "draw: writer",
            "compute: mip 0",
            "compute: mip 1",
            "compute: mip 2",
            "draw: reader"
        ]
    );
}

#[test]
fn work_staged_for_a_render_pass_or_an_entry_the_plan_lacks_is_refused_by_name() {
    let mut item = dispatches().remove(0);
    item.entry = WRITER;
    let (result, log) = run(vec![item.clone()]);
    assert_eq!(
        result,
        Err(FrameError::NotComputePass {
            label: "mip 0",
            entry: WRITER
        })
    );
    assert!(log.is_empty(), "nothing recorded: {log:?}");
    let missing = PassEntry::nth(PassSlot::Game(GamePlace::AfterOpaque), 7);
    item.entry = missing;
    assert_eq!(run(vec![item]).0, Err(FrameError::MissingPass(missing)));
}

#[test]
fn commands_without_a_compute_form_are_refused_by_name() {
    let mut items = dispatches();
    items[1].compute = false;
    assert_eq!(
        run(items).0,
        Err(FrameError::NotComputeWork {
            label: "mip 1",
            entry: COMPUTE
        })
    );
}

#[test]
fn a_level_the_pass_does_not_declare_is_an_unplanned_write() {
    let mut items = dispatches();
    items[2].writes = vec![SEED];
    assert_eq!(run(items).0, Err(FrameError::UnplannedWrite(SEED)));
}

#[test]
fn a_declared_level_no_item_writes_is_an_unstaged_write() {
    let mut items = dispatches();
    items.pop();
    assert_eq!(run(items).0, Err(FrameError::UnstagedWrite(chain(2))));
}

#[test]
fn a_read_of_a_level_no_earlier_item_of_the_pass_wrote_has_no_producer() {
    let mut items = dispatches();
    items.swap(1, 2);
    assert_eq!(run(items).0, Err(FrameError::MissingProducer(chain(1))));
}

#[test]
fn an_item_that_reads_the_level_it_writes_is_refused() {
    let mut items = dispatches();
    items[1].reads.push(chain(1));
    assert_eq!(run(items).0, Err(FrameError::AttachmentHazard(chain(1))));
}

#[test]
fn several_items_may_write_one_level_of_the_pass() {
    let mut items = dispatches();
    items.push(Item::new("mip 1 again", &[chain(0)], &[chain(1)]));
    let (result, log) = run(items);
    assert_eq!(result, Ok(()));
    assert_eq!(log[4], "compute: mip 1 again");
}

#[test]
fn an_item_whose_commands_write_another_level_than_it_declares_is_refused() {
    let mut items = dispatches();
    items[1].reported = Some(vec![chain(2)]);
    assert_eq!(
        run(items).0,
        Err(FrameError::PreparedWriteMismatch {
            label: "mip 1",
            declared: vec![chain(1)],
            written: vec![chain(2)],
        })
    );
}

/// Encodes `frame()` with `dispatches()` staged as the suffix of a capture whose contract
/// declares the writes to the seed and the output, and to the chain only when `own_chain`.
/// Returns what the capture said, the key of the chain's allocation and the log of what the
/// frame recorded.
fn captured(own_chain: bool) -> (Result<(), CaptureError<FrameError>>, WriteKey, Vec<String>) {
    let rig = Rig::new();
    let output = wgpu::TextureFormat::Rgba8Unorm;
    let mut chain = PostChain::new(
        &rig.device,
        FrameConfig::new(SIZE, output, 1),
        Resources::default(),
        CopyOutput::new(&rig.device, output),
        crate::SceneDepth::Independent,
    )
    .unwrap();
    let mut key = |texture: &wgpu::Texture| chain.capture_write_keys(&[texture]).unwrap()[0];
    let (seed_key, out_key, chain_key) = (key(&rig.seed), key(&rig.out), key(&rig.chain));
    let mut owned = vec![seed_key, out_key];
    if own_chain {
        owned.push(chain_key);
    }
    let publication = WriteKey::Resident(1);
    let suffix: Vec<_> = owned.iter().copied().chain([publication]).collect();
    let contract = CaptureWrites::new([], suffix, owned).unwrap();

    let (passes, features) = frame();
    let views = rig.views();
    let log = Log::default();
    let feature = |name, items| Feature {
        name,
        log: log.clone(),
        items,
        chain: rig.chain.clone(),
    };
    let (mut writer, mut stager, mut reader) = (
        feature("writer", Vec::new()),
        feature("stager", dispatches()),
        feature("reader", Vec::new()),
    );
    let mut borrowed: Vec<_> = features
        .into_iter()
        .zip([&mut writer, &mut stager, &mut reader])
        .map(|(declaration, prepared)| BorrowedFeature {
            declaration,
            prepared: prepared as &mut dyn PreparedFeature,
        })
        .collect();
    let authority = SubmissionAuthority::new(rig.queue.clone());
    let continuation = authority
        .begin_capture(
            &rig.device,
            contract,
            (),
            |_, _, ()| Ok::<_, FrameError>(()),
        )
        .unwrap()
        .submit()
        .unwrap();
    let result = continuation.resume(
        &rig.device,
        |_, _| Ok::<_, FrameError>(()),
        |encoder, edits, (), ()| {
            edits.stage_write(publication, || {}).unwrap();
            chain.encode_geometry(
                &rig.device,
                encoder,
                &FramePlan {
                    passes: &passes,
                    views: &views,
                    requested: &[],
                    operations: &[],
                },
                &mut borrowed,
            )
        },
    );
    let outcome = match result {
        Ok(suffix) => suffix
            .submit()
            .unwrap()
            .finish(&rig.device, |_, _| Ok::<_, ()>(()))
            .map_err(|_| unreachable!("the capture finishes")),
        Err(error) => {
            authority.drain_captures(&rig.device).unwrap();
            Err(error)
        }
    };
    drop(borrowed);
    let logged = log.borrow().clone();
    (outcome, chain_key, logged)
}

#[test]
fn a_compute_pass_writes_in_a_capture_only_the_allocations_the_capture_declares() {
    let (owned, _, log) = captured(true);
    assert!(owned.is_ok(), "the capture declares every write: {owned:?}");
    assert!(
        log.contains(&"compute: mip 2".to_owned()),
        "the compute pass recorded: {log:?}"
    );

    let (refused, chain_key, log) = captured(false);
    // The frame's encoding refuses the undeclared write, so the capture's suffix never runs.
    let Err(CaptureError::Preparation(FrameError::ResourceContract(ContractError::CaptureWrite(
        WriteError::Undeclared(undeclared),
    )))) = refused
    else {
        panic!("the compute pass's write of the chain is not declared: {refused:?}");
    };
    assert_eq!(undeclared, chain_key);
    assert!(
        !log.iter().any(|entry| entry.starts_with("compute:")),
        "nothing was recorded in the compute pass: {log:?}"
    );
}
