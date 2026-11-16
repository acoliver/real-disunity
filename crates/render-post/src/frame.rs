//! Geometry declarations for the same linear executor as post stages.
use std::collections::BTreeMap;

use crate::FrameTargets;

mod frozen;
mod operations;
pub(crate) use frozen::validate_stock_order;
pub use operations::{FrameOperations, OperationsGroup, OperationsOutput};
pub(crate) use operations::{GroupOutputs, OperationsSchedule, validate_operations};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PassSlot {
    ShadowCasters(u32),
    DepthNormalPrepass,
    DepthPyramid(u32),
    OcclusionMetadata(u32),
    Ssao,
    SsaoBlurHorizontal,
    SsaoBlur,
    Celestial,
    /// The deferred G-buffer: four material targets that opaque producers write before
    /// Opaque. Present only when the scene selects the G-buffer opaque path.
    OpaqueGbuffer,
    /// Deferred lighting of the G-buffer: one full-screen pass after the G-buffer and before
    /// Opaque, reading the scene depth the depth prepass completed. Present only when the scene
    /// selects a G-buffer opaque path; when it renders from the G-buffer, this pass writes the
    /// scene colour and Opaque draws only the surfaces without a G-buffer producer.
    OpaqueLighting,
    Opaque,
    /// Visible object IDs, tested against completed opaque depth without writing it.
    ContourIds,
    AfterOpaqueSnapshot,
    AfterOpaqueDepth,
    TransparentWater,
    /// Producers of the temporal mask that must follow the water pass's depth.
    TransparentWaterMask,
    TransparentAlpha,
    AfterWaterDepth,
    ReflectionDepthPyramid(u32),
    ReflectionTrace,
    ReflectionMip(u32),
    ScreenSpaceAfterTransparent,
}

/// Identity of one entry in an authored pass list: a role plus which appearance of
/// that role it is. A role may appear in a frame more than once, so a role alone does
/// not name an entry. `new(slot)` is the first appearance, which is what every
/// single-appearance declaration means.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PassEntry {
    slot: PassSlot,
    occurrence: u32,
}
impl PassEntry {
    pub const fn new(slot: PassSlot) -> Self {
        Self::nth(slot, 0)
    }
    pub const fn nth(slot: PassSlot, occurrence: u32) -> Self {
        Self { slot, occurrence }
    }
    pub const fn slot(self) -> PassSlot {
        self.slot
    }
    pub const fn occurrence(self) -> u32 {
        self.occurrence
    }
}
/// The role's name for the first appearance and `role#n` for the nth after it, so a
/// single-appearance entry reads exactly as its role always has.
impl std::fmt::Display for PassEntry {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{:?}", self.slot)?;
        match self.occurrence {
            0 => Ok(()),
            occurrence => write!(formatter, "#{occurrence}"),
        }
    }
}

/// A specific texture version and subresource, never an implicit latest value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ResourceRef {
    pub name: &'static str,
    pub version: u32,
    pub mip: u32,
    pub layer: u32,
}
impl ResourceRef {
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            version: 0,
            mip: 0,
            layer: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AttachmentLoad {
    Load,
    ClearColor(wgpu::Color),
    ClearDepth(f32),
}

#[derive(Clone, Debug, PartialEq)]
pub struct AttachmentDeclaration {
    pub resource: ResourceRef,
    pub format: wgpu::TextureFormat,
    pub size: [u32; 2],
    pub samples: u32,
    pub load: AttachmentLoad,
    pub store: wgpu::StoreOp,
    /// Attach a depth target without writing it, so the same pass may also sample it.
    /// Requires `load: Load` and `store: Store`.
    pub read_only: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PassDeclaration {
    /// The role: timing label, capture label and the `DrawContext` a feature sees.
    pub slot: PassSlot,
    /// Which appearance of `slot` this is; `slot` and `occurrence` together are the entry.
    pub occurrence: u32,
    pub color: Option<AttachmentDeclaration>,
    /// Additional indexed color outputs after location zero.
    pub additional_colors: Vec<AttachmentDeclaration>,
    pub depth: Option<AttachmentDeclaration>,
}
impl PassDeclaration {
    pub const fn new(slot: PassSlot) -> Self {
        Self::nth(slot, 0)
    }
    pub const fn nth(slot: PassSlot, occurrence: u32) -> Self {
        Self {
            slot,
            occurrence,
            color: None,
            additional_colors: Vec::new(),
            depth: None,
        }
    }
    pub const fn entry(&self) -> PassEntry {
        PassEntry::nth(self.slot, self.occurrence)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FeaturePass {
    /// The role of the entry this feature draws in.
    pub slot: PassSlot,
    /// Which appearance of `slot` it draws in; `slot` and `occurrence` together name the entry.
    pub occurrence: u32,
    pub reads: Vec<ResourceRef>,
    pub color_format: Option<wgpu::TextureFormat>,
    pub additional_color_formats: Vec<wgpu::TextureFormat>,
    pub depth_format: Option<wgpu::TextureFormat>,
    pub samples: u32,
}
impl FeaturePass {
    pub const fn new(slot: PassSlot) -> Self {
        Self::nth(slot, 0)
    }
    pub const fn nth(slot: PassSlot, occurrence: u32) -> Self {
        Self {
            slot,
            occurrence,
            reads: Vec::new(),
            color_format: None,
            additional_color_formats: Vec::new(),
            depth_format: None,
            samples: 1,
        }
    }
    pub const fn entry(&self) -> PassEntry {
        PassEntry::nth(self.slot, self.occurrence)
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct FeatureDeclaration {
    pub passes: Vec<FeaturePass>,
    pub effects: Vec<&'static str>,
}
impl FeatureDeclaration {
    pub fn new(slot: PassSlot) -> Self {
        Self {
            passes: vec![FeaturePass::new(slot)],
            effects: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum FrameError {
    #[error("streaming terrain already prepared for a different view in this transaction")]
    StreamingSelectionConflict,
    #[error("{feature} preparation failed: {reason}")]
    FeaturePreparation {
        feature: &'static str,
        reason: String,
    },
    #[error(transparent)]
    Transaction(#[from] worldrunner_render_gpu::submission::TransactionError),
    #[error("invalid or unsupported SSR setting: {0}")]
    SsrSettings(&'static str),
    #[error("invalid or unsupported water setting: {0}")]
    WaterSettings(&'static str),
    /// Two transparent features draw in the same entry. Transparent entries run in the order the
    /// frame lists them, but the executor does not sort packets across features inside one entry.
    #[error("{0} has more than one transparent feature; list each in its own entry")]
    TransparentOrdering(PassEntry),
    #[error("scene exposure differs from the compiled fixed-exposure profile")]
    FixedExposureMismatch,
    #[error("invalid or unsupported SSAO setting: {0}")]
    SsaoSettings(&'static str),
    /// The list names one entry twice. A role may repeat; an entry (role and occurrence) may not.
    #[error("pass entry appears more than once in the frame: {0}")]
    DuplicatePass(PassEntry),
    #[error(transparent)]
    ResourceContract(#[from] crate::contracts::ContractError),
    #[error("feature requests an undeclared pass entry {0}")]
    MissingPass(PassEntry),
    #[error("resource has no preceding producer: {0:?}")]
    MissingProducer(ResourceRef),
    #[error("resource is loaded or sampled after discard: {0:?}")]
    LoadAfterDiscard(ResourceRef),
    #[error("resource is sampled while attached or written ambiguously: {0:?}")]
    AttachmentHazard(ResourceRef),
    #[error("attachment or pipeline contract is incompatible in slot {0}")]
    AttachmentContract(PassEntry),
    #[error("frame does not support requested effect {0}")]
    UnsupportedEffect(&'static str),
    #[error("attachment view does not match its declared subresource: {0:?}")]
    AttachmentView(ResourceRef),
    /// A frozen plan lists a pass before one that the stock passes need it to follow.
    #[error("pass {pass} is listed before {needs}, which the stock passes need it to follow")]
    StockOrder { pass: PassEntry, needs: PassEntry },
    /// A pass reads the depth that Opaque draws with before a prepass or Opaque has written it.
    #[error("pass {pass} reads {resource:?} before the opaque depth is complete")]
    IncompleteDepth {
        pass: PassEntry,
        resource: ResourceRef,
    },
    /// A geometry pass attaches a texture that operations groups write.
    #[error("pass {pass} attaches {resource:?}, which operations write; passes may only read it")]
    OffscreenWrite {
        pass: PassEntry,
        resource: ResourceRef,
    },
}

/// Validates the authored sequence without allocating, preparing or encoding anything.
pub fn validate_frame(
    passes: &[PassDeclaration],
    features: &[FeatureDeclaration],
    requested: &[&'static str],
) -> Result<(), FrameError> {
    validate_frame_inputs(passes, features, requested, &[], &[])
}

/// The checks `encode_geometry` makes before anything is prepared, in its order.
pub(crate) fn validate_plan(
    passes: &[PassDeclaration],
    features: &[FeatureDeclaration],
    requested: &[&'static str],
    views: &[AttachmentView],
    operations: &[OperationsGroup<'_>],
) -> Result<(), FrameError> {
    validate_operations(passes, views, operations)?;
    validate_frame_inputs(passes, features, requested, views, operations)?;
    validate_views(passes, views)
}

pub(crate) fn validate_frame_inputs(
    passes: &[PassDeclaration],
    features: &[FeatureDeclaration],
    requested: &[&'static str],
    views: &[AttachmentView],
    operations: &[OperationsGroup<'_>],
) -> Result<(), FrameError> {
    validate_transparency(features)?;
    let repeated = passes.iter().enumerate().find_map(|(index, pass)| {
        passes[..index]
            .iter()
            .any(|previous| previous.entry() == pass.entry())
            .then(|| pass.entry())
    });
    if let Some(entry) = repeated {
        return Err(FrameError::DuplicatePass(entry));
    }
    for effect in requested {
        if !features
            .iter()
            .any(|feature| feature.effects.contains(effect))
        {
            return Err(FrameError::UnsupportedEffect(effect));
        }
    }
    for feature in features {
        for feature_pass in &feature.passes {
            if !passes
                .iter()
                .any(|pass| pass.entry() == feature_pass.entry())
            {
                return Err(FrameError::MissingPass(feature_pass.entry()));
            }
        }
    }
    let outputs = GroupOutputs::new(passes, operations);
    // A retained view a group writes has contents only from the group's position on.
    let mut produced: BTreeMap<_, _> = views
        .iter()
        .filter(|view| view.retained && !outputs.writes(view.resource()))
        .map(|view| {
            (
                view.resource(),
                AttachmentDeclaration {
                    resource: view.resource(),
                    format: view.format(),
                    size: view.size(),
                    samples: view.texture().sample_count(),
                    load: AttachmentLoad::Load,
                    store: wgpu::StoreOp::Store,
                    read_only: false,
                },
            )
        })
        .collect();
    for (index, pass) in passes.iter().enumerate() {
        outputs.refuse_writes(pass)?;
        produced.extend(outputs.left_before(index));
        let attachments: Vec<_> = pass
            .color
            .iter()
            .chain(&pass.additional_colors)
            .chain(&pass.depth)
            .collect();
        validate_attachments(pass, &produced)?;
        for feature in features
            .iter()
            .flat_map(|feature| &feature.passes)
            .filter(|feature| feature.entry() == pass.entry())
        {
            validate_feature(pass, feature, &attachments, &produced)?;
        }
        for attachment in attachments {
            produced.insert(attachment.resource, attachment.clone());
        }
    }
    Ok(())
}
/// Each transparent entry may hold one transparent feature. Entries run in the order the frame
/// lists them, so water and alpha in separate entries compose in that order; several features in
/// one entry would need their packets sorted against each other, which the executor does not do.
fn validate_transparency(features: &[FeatureDeclaration]) -> Result<(), FrameError> {
    let mut seen = std::collections::BTreeSet::new();
    features
        .iter()
        .flat_map(|feature| &feature.passes)
        .filter(|pass| {
            matches!(
                pass.slot,
                PassSlot::TransparentWater | PassSlot::TransparentAlpha
            )
        })
        .find(|pass| !seen.insert(pass.entry()))
        .map_or(Ok(()), |pass| {
            Err(FrameError::TransparentOrdering(pass.entry()))
        })
}
/// Structural sample-count check, for callers that cannot ask the adapter.
///
/// 2x and 8x are admitted only when `extended` says the device enabled adapter-specific
/// format features, and then for formats that can be multisampled at all. Whether the
/// adapter supports the exact count is checked by `FrameConfig::validate_adapter`
/// before a frame is built.
pub(crate) fn sample_count_admissible(
    format: wgpu::TextureFormat,
    samples: u32,
    extended: bool,
) -> bool {
    let probe = match samples {
        1 => 1,
        4 => 4,
        2 | 8 if extended => 4,
        _ => return false,
    };
    format
        .guaranteed_format_features(wgpu::Features::empty())
        .flags
        .sample_count_supported(probe)
}
fn validate_attachments(
    pass: &PassDeclaration,
    produced: &BTreeMap<ResourceRef, AttachmentDeclaration>,
) -> Result<(), FrameError> {
    let attachments: Vec<_> = pass
        .color
        .iter()
        .chain(&pass.additional_colors)
        .chain(&pass.depth)
        .collect();
    if attachments.is_empty() {
        return Err(FrameError::AttachmentContract(pass.entry()));
    }
    validate_attachment_shapes(pass, &attachments)?;
    for attachment in &attachments {
        let depth = pass
            .depth
            .as_ref()
            .is_some_and(|item| std::ptr::eq(item, *attachment));
        if attachment.size.contains(&0)
            || !sample_count_admissible(attachment.format, attachment.samples, true)
            || attachment.format.is_depth_stencil_format() != depth
            || (depth && !attachment.format.has_depth_aspect())
            || !attachment
                .format
                .guaranteed_format_features(wgpu::Features::empty())
                .allowed_usages
                .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
            || matches!(attachment.load, AttachmentLoad::ClearDepth(value) if !value.is_finite() || !(0.0..=1.0).contains(&value))
            || matches!(
                (depth, attachment.load),
                (true, AttachmentLoad::ClearColor(_)) | (false, AttachmentLoad::ClearDepth(_))
            )
        {
            return Err(FrameError::AttachmentContract(pass.entry()));
        }
        if produced.contains_key(&attachment.resource)
            && matches!(
                attachment.resource.name,
                "opaque snapshot color" | "opaque snapshot depth" | "post-water depth"
            )
        {
            return Err(FrameError::AttachmentHazard(attachment.resource));
        }
        if attachment.load == AttachmentLoad::Load {
            let previous = require_producer(produced, attachment.resource)?;
            if previous.format != attachment.format
                || previous.size != attachment.size
                || previous.samples != attachment.samples
            {
                return Err(FrameError::AttachmentContract(pass.entry()));
            }
        } else if produced.contains_key(&attachment.resource) {
            return Err(FrameError::AttachmentHazard(attachment.resource));
        }
    }
    Ok(())
}
fn validate_attachment_shapes(
    pass: &PassDeclaration,
    attachments: &[&AttachmentDeclaration],
) -> Result<(), FrameError> {
    for (index, attachment) in attachments.iter().enumerate() {
        if attachments[..index]
            .iter()
            .any(|other| other.resource == attachment.resource)
        {
            return Err(FrameError::AttachmentHazard(attachment.resource));
        }
        if attachment.size != attachments[0].size || attachment.samples != attachments[0].samples {
            return Err(FrameError::AttachmentContract(pass.entry()));
        }
    }
    Ok(())
}

fn validate_feature(
    pass: &PassDeclaration,
    feature: &FeaturePass,
    attachments: &[&AttachmentDeclaration],
    produced: &BTreeMap<ResourceRef, AttachmentDeclaration>,
) -> Result<(), FrameError> {
    if attachments
        .iter()
        .any(|attachment| attachment.samples != feature.samples)
        || feature
            .color_format
            .is_some_and(|format| pass.color.as_ref().is_none_or(|a| a.format != format))
        || feature.additional_color_formats
            != pass
                .additional_colors
                .iter()
                .map(|attachment| attachment.format)
                .collect::<Vec<_>>()
        || feature
            .depth_format
            .is_some_and(|format| pass.depth.as_ref().is_none_or(|a| a.format != format))
    {
        return Err(FrameError::AttachmentContract(pass.entry()));
    }
    for resource in &feature.reads {
        let sampled_depth = pass.depth.as_ref().is_some_and(|depth| {
            depth.resource == *resource && depth.read_only && depth.load == AttachmentLoad::Load
        });
        if !sampled_depth && attachments.iter().any(|a| a.resource == *resource) {
            return Err(FrameError::AttachmentHazard(*resource));
        }
        require_producer(produced, *resource)?;
    }
    Ok(())
}

fn require_producer(
    produced: &BTreeMap<ResourceRef, AttachmentDeclaration>,
    resource: ResourceRef,
) -> Result<&AttachmentDeclaration, FrameError> {
    match produced.get(&resource) {
        None => Err(FrameError::MissingProducer(resource)),
        Some(previous) if previous.store == wgpu::StoreOp::Discard => {
            Err(FrameError::LoadAfterDiscard(resource))
        }
        Some(previous) => Ok(previous),
    }
}

pub struct BuildContext<'a> {
    pub device: &'a wgpu::Device,
    pub targets: &'a FrameTargets,
}
/// One buffer operation staged by a preparation, encoded in staging order.
enum Staged {
    Copy {
        source: (wgpu::Buffer, u64),
        destination: (wgpu::Buffer, u64),
        bytes: u64,
    },
    Dispatch {
        label: &'static str,
        pipeline: wgpu::ComputePipeline,
        group: wgpu::BindGroup,
        workgroups: [u32; 3],
    },
}
pub struct PrepareContext<'a> {
    device: &'a wgpu::Device,
    encoder: &'a mut wgpu::CommandEncoder,
    publications: worldrunner_render_gpu::submission::Publications,
    staged: Vec<Staged>,
    views: Vec<AttachmentView>,
    completed: Vec<PassEntry>,
}
impl<'a> PrepareContext<'a> {
    pub fn new(
        device: &'a wgpu::Device,
        encoder: &'a mut wgpu::CommandEncoder,
    ) -> Result<Self, worldrunner_render_gpu::submission::TransactionError> {
        let publications = worldrunner_render_gpu::submission::Publications::for_encoder(encoder)?;
        Ok(Self {
            device,
            encoder,
            publications,
            staged: Vec::new(),
            views: Vec::new(),
            completed: Vec::new(),
        })
    }
    /// Whether a validated geometry prefix already encoded this entry in the same frame.
    pub fn completed_in_prefix(&self, entry: PassEntry) -> bool {
        self.completed.contains(&entry)
    }
    pub(crate) fn set_completed_prefix(&mut self, completed: &[PassEntry]) {
        self.completed = completed.to_vec();
    }
    pub fn publications(&self) -> worldrunner_render_gpu::submission::Publications {
        self.publications.clone()
    }
    /// Stages an infallible resident edit in the active prepared frame.
    pub fn publish(&mut self, publish: impl FnOnce() + 'static) {
        self.publications.stage(publish);
    }
    pub(crate) fn set_views(&mut self, views: &[AttachmentView]) {
        self.views = views.to_vec();
    }
    /// Frame-local checked inputs, available only after full plan validation.
    pub fn resource(&self, resource: ResourceRef) -> Result<&AttachmentView, FrameError> {
        self.views
            .iter()
            .find(|view| view.resource() == resource)
            .ok_or(FrameError::AttachmentView(resource))
    }
    pub fn device(&self) -> &wgpu::Device {
        self.device
    }
    pub fn copy_buffer(&mut self, source: &wgpu::Buffer, destination: &wgpu::Buffer, bytes: u64) {
        self.copy_buffer_region((source, 0), (destination, 0), bytes);
    }
    /// Stages a copy of `bytes` bytes between buffer offsets, encoded after the copies staged
    /// before it. A gather that reads bytes uploaded in this context therefore sees them.
    /// Offsets and `bytes` must be multiples of `wgpu::COPY_BUFFER_ALIGNMENT`.
    pub fn copy_buffer_region(
        &mut self,
        source: (&wgpu::Buffer, u64),
        destination: (&wgpu::Buffer, u64),
        bytes: u64,
    ) {
        self.staged.push(Staged::Copy {
            source: (source.0.clone(), source.1),
            destination: (destination.0.clone(), destination.1),
            bytes,
        });
    }
    /// Stages one compute dispatch, encoded in its own compute pass after the copies and
    /// dispatches staged before it, so it reads bytes uploaded or copied earlier in this context.
    pub fn dispatch(
        &mut self,
        label: &'static str,
        pipeline: &wgpu::ComputePipeline,
        group: &wgpu::BindGroup,
        workgroups: [u32; 3],
    ) {
        self.staged.push(Staged::Dispatch {
            label,
            pipeline: pipeline.clone(),
            group: group.clone(),
            workgroups,
        });
    }
    /// Stages `bytes` for `destination` at `offset`, copied in staging order like
    /// [`Self::copy_buffer_region`]. The bytes are written into the frame authority's reusable
    /// staging now; a cancelled frame never copies them. `bytes` must be a multiple of
    /// `wgpu::COPY_BUFFER_ALIGNMENT` long and fit `destination`, which must allow `COPY_DST`.
    /// An empty upload copies nothing and stages nothing.
    pub fn upload_buffer(&mut self, destination: &wgpu::Buffer, offset: u64, bytes: &[u8]) {
        let size = bytes.len() as u64;
        assert!(
            destination.usage().contains(wgpu::BufferUsages::COPY_DST)
                && offset.is_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT)
                && offset
                    .checked_add(size)
                    .is_some_and(|end| end <= destination.size()),
            "upload of {size} bytes at {offset} does not fit {destination:?}"
        );
        if bytes.is_empty() {
            return;
        }
        let source = self.publications.stage_upload(self.device, bytes);
        self.staged.push(Staged::Copy {
            source,
            destination: (destination.clone(), offset),
            bytes: size,
        });
    }
    /// Publishes staged copies and dispatches only after all fallible preparations succeed.
    pub fn finish(self) -> Vec<wgpu::Buffer> {
        let mut retained = Vec::with_capacity(self.staged.len());
        for staged in self.staged {
            match staged {
                Staged::Copy {
                    source,
                    destination,
                    bytes,
                } => {
                    self.encoder.copy_buffer_to_buffer(
                        &source.0,
                        source.1,
                        &destination.0,
                        destination.1,
                        bytes,
                    );
                    retained.push(source.0);
                }
                Staged::Dispatch {
                    label,
                    pipeline,
                    group,
                    workgroups,
                } => {
                    let mut pass = self
                        .encoder
                        .begin_compute_pass(&wgpu::ComputePassDescriptor {
                            label: Some(label),
                            timestamp_writes: None,
                        });
                    pass.set_pipeline(&pipeline);
                    pass.set_bind_group(0, &group, &[]);
                    pass.dispatch_workgroups(workgroups[0], workgroups[1], workgroups[2]);
                }
            }
        }
        retained
    }
}
/// Later geometry slices will add view identifiers/bindings and validated
/// inter-slot compute, copy and fullscreen operations. Those operations are not
/// part of this geometry-only interface; stock effect validation remains separate.
pub struct DrawContext {
    /// The role of the entry being drawn, which is `entry.slot()`.
    pub slot: PassSlot,
    /// The entry being drawn. A feature declared in two entries of one role tells them
    /// apart by `entry.occurrence()`.
    pub entry: PassEntry,
}
impl DrawContext {
    pub const fn new(entry: PassEntry) -> Self {
        Self {
            slot: entry.slot(),
            entry,
        }
    }
}

/// Features receive neither a queue nor authority to open or replace attachments.
pub trait FrameFeature {
    fn declare(&self) -> FeatureDeclaration;
    /// Adds owned attachments/passes before validation, never uploads or encodes.
    fn plan(
        &self,
        _passes: &mut Vec<PassDeclaration>,
        _views: &mut Vec<AttachmentView>,
    ) -> Result<FeatureDeclaration, FrameError> {
        Ok(self.declare())
    }
    fn build(&self, context: &BuildContext<'_>) -> Result<Box<dyn PreparedFeature>, FrameError>;
}
/// Preparation may stage uploads, but must not publish resident state.
/// Stage publication in `commit`, after every preparation succeeds. The enclosing
/// frame publishes those edits only after submission.
pub trait PreparedFeature {
    fn prepare(&mut self, context: &mut PrepareContext<'_>) -> Result<(), FrameError>;
    fn commit(&mut self, _context: &mut PrepareContext<'_>) {}
    /// Stages completed draw results after the scheduler's checked encoder closes its passes.
    fn finish_draws(&mut self, _context: &mut PrepareContext<'_>) {}
    fn draw(&self, context: &DrawContext, pass: &mut wgpu::RenderPass<'_>);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeatureId(pub(crate) usize);

pub(crate) struct RegisteredFeature {
    pub declaration: FeatureDeclaration,
    pub prepared: Option<Box<dyn PreparedFeature>>,
    pub source: Box<dyn FrameFeature>,
}

/// Borrowed features use the same prepare/draw boundary as retained registrations.
/// This lets a resident scene keep its existing exclusive handle-editing API.
pub struct BorrowedFeature<'a> {
    pub declaration: FeatureDeclaration,
    pub prepared: &'a mut dyn PreparedFeature,
}

/// Views are borrowed for this encode only; plans never retain surface views.
#[derive(Clone, Debug)]
pub struct AttachmentView {
    retained: bool,
    resource: ResourceRef,
    view: wgpu::TextureView,
    format: wgpu::TextureFormat,
}
impl AttachmentView {
    /// Declares initialized contents from an earlier ordered submission. The caller
    /// must retain the allocation and submit encoders in preparation order.
    #[must_use]
    pub fn with_retained_contents(mut self) -> Self {
        self.retained = true;
        self
    }
    /// Whether this view declares initialized contents from an earlier ordered submission.
    pub fn retained(&self) -> bool {
        self.retained
    }

    pub const fn resource(&self) -> ResourceRef {
        self.resource
    }
    pub fn texture(&self) -> &wgpu::Texture {
        self.view.texture()
    }
    /// The exact checked attachment view, including both aspects of combined depth/stencil.
    pub fn view(&self) -> &wgpu::TextureView {
        &self.view
    }
    /// A sampled view of the checked mip and layer, selecting only depth for combined formats.
    pub fn sampled_view(&self) -> wgpu::TextureView {
        if !self.format.has_stencil_aspect() || !self.format.has_depth_aspect() {
            return self.view.clone();
        }
        self.texture().create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2),
            aspect: wgpu::TextureAspect::DepthOnly,
            base_mip_level: self.resource.mip,
            mip_level_count: Some(1),
            base_array_layer: self.resource.layer,
            array_layer_count: Some(1),
            ..Default::default()
        })
    }
    /// The checked view's attachment format, not an inferred pipeline format.
    pub const fn format(&self) -> wgpu::TextureFormat {
        self.format
    }
    /// The selected mip's extent, independent of the parent texture's base size.
    pub fn size(&self) -> [u32; 2] {
        [
            (self.texture().width() >> self.resource.mip).max(1),
            (self.texture().height() >> self.resource.mip).max(1),
        ]
    }
    /// Relabels a checked view without changing its selected mip or layer.
    pub fn with_resource(&self, resource: ResourceRef) -> Result<Self, FrameError> {
        if self.resource.mip != resource.mip || self.resource.layer != resource.layer {
            return Err(FrameError::AttachmentView(self.resource));
        }
        Ok(Self {
            resource,
            retained: self.retained,
            view: self.view.clone(),
            format: self.format,
        })
    }
    /// Creates the exact declared single-mip, single-layer view, avoiding caller/view drift.
    pub fn new(resource: ResourceRef, texture: &wgpu::Texture) -> Result<Self, FrameError> {
        if texture.dimension() != wgpu::TextureDimension::D2
            || resource.mip >= texture.mip_level_count()
            || resource.layer >= texture.depth_or_array_layers()
        {
            return Err(FrameError::AttachmentView(resource));
        }
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(texture.format()),
            dimension: Some(wgpu::TextureViewDimension::D2),
            base_mip_level: resource.mip,
            mip_level_count: Some(1),
            base_array_layer: resource.layer,
            array_layer_count: Some(1),
            ..Default::default()
        });
        Ok(Self {
            resource,
            view,
            retained: false,
            format: texture.format(),
        })
    }
}

pub(crate) fn validate_views(
    passes: &[PassDeclaration],
    views: &[AttachmentView],
) -> Result<(), FrameError> {
    for (index, view) in views.iter().enumerate() {
        if views[..index].iter().any(|other| {
            other.resource == view.resource
                || (other.view.texture() == view.view.texture()
                    && other.resource.mip == view.resource.mip
                    && other.resource.layer == view.resource.layer)
        }) {
            return Err(FrameError::AttachmentHazard(view.resource));
        }
    }
    for attachment in passes.iter().flat_map(|pass| {
        pass.color
            .iter()
            .chain(&pass.additional_colors)
            .chain(&pass.depth)
    }) {
        let view = views
            .iter()
            .find(|view| view.resource == attachment.resource)
            .ok_or(FrameError::AttachmentView(attachment.resource))?;
        let texture = view.view.texture();
        if view.format() != attachment.format
            || texture.sample_count() != attachment.samples
            || view.size() != attachment.size
            || !texture
                .usage()
                .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
        {
            return Err(FrameError::AttachmentView(attachment.resource));
        }
    }
    Ok(())
}

/// Complete borrowed geometry contract, validated before feature preparation.
pub struct FramePlan<'a> {
    pub passes: &'a [PassDeclaration],
    pub views: &'a [AttachmentView],
    pub requested: &'a [&'static str],
    /// Operations groups the executor encodes at their positions in `passes`.
    pub operations: &'a [OperationsGroup<'a>],
}
