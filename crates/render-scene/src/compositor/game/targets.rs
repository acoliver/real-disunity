//! Targets a game declares: allocated once, shared by its passes and, when persistent, kept
//! across frames and renderer replacements.
use super::frame_start;
use super::snapshots::SnapshotReads;
use std::collections::BTreeMap;
use std::ops::Range;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::MutexGuard;
use std::sync::PoisonError;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use worldrunner_render_gpu::configuration::AllocationSet;
use worldrunner_render_gpu::submission::Publications;
use worldrunner_render_gpu::submission::capture::WriteKey;
use worldrunner_render_post::AttachmentView;
use worldrunner_render_post::FrameError;
use worldrunner_render_post::PassDeclaration;
use worldrunner_render_post::PostError;
use worldrunner_render_post::PrepareContext;
use worldrunner_render_post::ResourceRef;

/// How large a declared target is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetExtent {
    /// The frame's raster extent, which follows every frame resize.
    Frame,
    /// A fixed extent in texels, whatever the frame does.
    Fixed([u32; 2]),
}

/// How long a declared target's contents live.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetLifetime {
    /// Written and read within one frame. Nothing is assumed about the contents when a frame
    /// starts, so a pass that reads the target needs a pass of the same frame that wrote it.
    PerFrame,
    /// Kept after the frame that wrote it is submitted, across frames and across renderer
    /// replacement. A layer is readable without a writer once a submitted frame has written it.
    /// A frame that is cancelled before submission leaves it exactly as it was. A frame resize
    /// recreates a target of [`TargetExtent::Frame`], which discards its contents.
    Persistent,
}

/// A named texture a game owns: one 2D texture of one or more layers and one or more mips, with
/// one view per mip of each layer and any sampled view the game makes.
///
/// A renderable colour format makes a target passes render into, one (layer, mip) at a time. A
/// block-compressed format makes a copy target, which only a frame's preparation writes, mip by mip
/// of each layer, by copy ([`GameTargets::plan_prepared`]), and passes sample. Either may carry a
/// mip chain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetDeclaration {
    /// Names the target in the plan, in errors and in the allocation inventory.
    pub name: &'static str,
    /// A renderable colour format, or a block-compressed format the device has the features of.
    pub format: wgpu::TextureFormat,
    pub extent: TargetExtent,
    /// Array layers, at least one.
    pub layers: u32,
    /// Mip levels, at least one and at most the extent's full chain. A pass fills one mip of a
    /// renderable target ([`ColorOutput::Target`](super::ColorOutput::Target) of
    /// [`mip`](Self::mip)) and the frame's preparation fills one of a copy target.
    pub mips: u32,
    pub lifetime: TargetLifetime,
    /// Usage beyond `RENDER_ATTACHMENT | TEXTURE_BINDING`, which every renderable target has, or
    /// beyond `COPY_DST | TEXTURE_BINDING`, which every copy target has.
    pub usage: wgpu::TextureUsages,
}

impl TargetDeclaration {
    /// A one-layer, per-frame target with no extra usage.
    pub const fn new(
        name: &'static str,
        format: wgpu::TextureFormat,
        extent: TargetExtent,
    ) -> Self {
        Self {
            name,
            format,
            extent,
            layers: 1,
            mips: 1,
            lifetime: TargetLifetime::PerFrame,
            usage: wgpu::TextureUsages::empty(),
        }
    }

    #[must_use]
    pub const fn layers(mut self, layers: u32) -> Self {
        self.layers = layers;
        self
    }

    /// A chain of `mips` levels, each half the one above it, rounded down and at least one texel.
    /// A block-compressed level smaller than a block, or not a whole number of blocks, is stored
    /// as the whole blocks that cover it, as wgpu sizes compressed mips.
    #[must_use]
    pub const fn mips(mut self, mips: u32) -> Self {
        self.mips = mips;
        self
    }

    #[must_use]
    pub const fn persistent(mut self) -> Self {
        self.lifetime = TargetLifetime::Persistent;
        self
    }

    #[must_use]
    pub const fn with_usage(mut self, usage: wgpu::TextureUsages) -> Self {
        self.usage = usage;
        self
    }

    /// The resource a pass names to attach or read `layer` of this target: its first mip. A pass
    /// that reads it reads every mip of the layer.
    pub const fn layer(&self, layer: u32) -> ResourceRef {
        self.mip(layer, 0)
    }

    /// One mip of one layer, the subresource a frame's preparation writes by copy.
    pub const fn mip(&self, layer: u32, mip: u32) -> ResourceRef {
        ResourceRef {
            name: self.name,
            version: 0,
            mip,
            layer,
        }
    }

    /// Every mip of `layer`, from the first.
    pub fn layer_mips(&self, layer: u32) -> impl Iterator<Item = ResourceRef> + '_ {
        (0..self.mips).map(move |mip| self.mip(layer, mip))
    }

    /// The mips `mips` of `layer`, in order.
    pub fn mip_range(
        &self,
        layer: u32,
        mips: std::ops::Range<u32>,
    ) -> impl Iterator<Item = ResourceRef> + '_ {
        mips.map(move |mip| self.mip(layer, mip))
    }

    /// Every layer of the target, for a pass that reads the whole array.
    pub fn all_layers(&self) -> impl Iterator<Item = ResourceRef> + '_ {
        (0..self.layers).map(|layer| self.layer(layer))
    }

    pub(super) fn extent_in(&self, frame: [u32; 2]) -> [u32; 2] {
        match self.extent {
            TargetExtent::Frame => frame,
            TargetExtent::Fixed(extent) => extent,
        }
    }

    /// Whether only copies write the target: a block-compressed format cannot be rendered into.
    fn copy_target(&self) -> bool {
        self.format.is_compressed()
    }

    fn texture_usage(&self) -> wgpu::TextureUsages {
        let written = if self.copy_target() {
            wgpu::TextureUsages::COPY_DST
        } else {
            wgpu::TextureUsages::RENDER_ATTACHMENT
        };
        written | wgpu::TextureUsages::TEXTURE_BINDING | self.usage
    }

    /// The refusal of a declaration the device cannot allocate, naming the target.
    pub(super) fn validate(
        &self,
        (limits, features): (&wgpu::Limits, wgpu::Features),
        frame: [u32; 2],
    ) -> Result<(), PostError> {
        let refuse = |why: &str| {
            Err(PostError::Allocation(format!(
                "game target {}: {why}",
                self.name
            )))
        };
        let [width, height] = self.extent_in(frame);
        if self.layers == 0 || self.layers > limits.max_texture_array_layers {
            return refuse("layer count is outside 1..=max_texture_array_layers");
        }
        if width == 0 || height == 0 || width.max(height) > limits.max_texture_dimension_2d {
            return refuse("extent is empty or beyond max_texture_dimension_2d");
        }
        // wgpu's `max_mips` for a 2D texture: down to one texel along the longer side.
        let chain = u32::BITS - width.max(height).leading_zeros();
        if self.mips == 0 || self.mips > chain {
            return refuse(&format!(
                "mip count {} is outside 1..={chain} for the extent {width}x{height}",
                self.mips
            ));
        }
        if self.copy_target() {
            let missing = self.format.required_features().difference(features);
            if !missing.is_empty() {
                return refuse(&format!("format needs device features {missing:?}"));
            }
            let (block_width, block_height) = self.format.block_dimensions();
            if width % block_width != 0 || height % block_height != 0 {
                return refuse("extent is not a whole number of the format's blocks");
            }
            return Ok(());
        }
        let guaranteed = self
            .format
            .guaranteed_format_features(wgpu::Features::empty());
        if self.format.is_depth_stencil_format()
            || !guaranteed
                .allowed_usages
                .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
        {
            return refuse("format is not a renderable colour format");
        }
        let unsupported = self.usage.difference(guaranteed.allowed_usages);
        if !unsupported.is_empty() {
            return refuse(&format!(
                "{:?} cannot be created with {unsupported:?}",
                self.format
            ));
        }
        Ok(())
    }
}

/// One physical texture of a declared target. It is shared by every renderer generation that
/// keeps it, and it is the only place that knows which of its layers a submitted frame has
/// written, so the bookkeeping can never belong to a different texture than its contents.
pub(crate) struct Backing {
    declaration: TargetDeclaration,
    extent: [u32; 2],
    texture: wgpu::Texture,
    /// Numbers the textures an inventory's renderer has made, which keeps this one apart from
    /// another of the same name in the allocation budget.
    serial: u64,
    /// Per mip of each layer, at `layer * mips + mip`: whether a submitted frame has written it.
    /// This is the published state. The queries of a frame in preparation answer through
    /// [`Self::state`] instead.
    written: Mutex<Vec<bool>>,
    /// The native views of the texture, made when a frame first asks for them and dropped with the
    /// texture.
    views: Mutex<Views>,
    /// Counts the views made for every texture of the lineage, which `views` adds to.
    made: Arc<AtomicU64>,
}

/// The native views of one backing's texture. Every view of a declared target has the texture's
/// own format, every aspect and the texture's usages, so the mips and layers it selects tell two
/// views apart.
struct Views {
    /// The view of each mip of each layer, at `layer * mips + mip`, which a pass attaches or
    /// reads alone and a compute pass writes. It carries no declaration of what the subresource holds: the
    /// state a frame starts with is [`Backing::state`], applied to a copy by [`TargetStore::view`].
    attachments: Vec<Option<AttachmentView>>,
    /// The view of every layer of the mips that start at `.0` and number `.1`, which a pass
    /// samples.
    sampled: BTreeMap<(u32, u32), wgpu::TextureView>,
}

impl Backing {
    fn create(
        device: &wgpu::Device,
        declaration: &TargetDeclaration,
        extent: [u32; 2],
        serial: u64,
        made: Arc<AtomicU64>,
    ) -> Self {
        Self {
            texture: device.create_texture(&wgpu::TextureDescriptor {
                label: Some(declaration.name),
                size: wgpu::Extent3d {
                    width: extent[0],
                    height: extent[1],
                    depth_or_array_layers: declaration.layers,
                },
                mip_level_count: declaration.mips,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: declaration.format,
                usage: declaration.texture_usage(),
                view_formats: &[],
            }),
            written: Mutex::new(vec![
                false;
                (declaration.layers * declaration.mips) as usize
            ]),
            views: Mutex::new(Views {
                attachments: vec![None; (declaration.layers * declaration.mips) as usize],
                sampled: BTreeMap::new(),
            }),
            made,
            declaration: declaration.clone(),
            extent,
            serial,
        }
    }

    fn persistent(&self) -> bool {
        self.declaration.lifetime == TargetLifetime::Persistent
    }

    /// Per mip of each layer, at `layer * mips + mip`, whether a submitted frame had written it
    /// when the frame in preparation started. Outside a frame, and in one that has staged nothing on this backing yet, that is
    /// the published state.
    ///
    /// Every query of a frame in preparation uses this one state: the planning query
    /// [`GameTargets::is_written`], the load a pass is planned with, the allocation views declare
    /// and the initial version a capture imports the texture at. A capture submits its first
    /// segment, which publishes what it wrote, before its second segment plans, and the two must
    /// agree. The second segment lists the first one's completed passes with the declaration the
    /// first one used, and the capture imported the texture once before the first segment, so a
    /// view that already reported the first segment's writes would plan a different load and
    /// import the texture differently from the capture.
    fn state(&self) -> Vec<bool> {
        frame_start::recall(self).unwrap_or_else(|| self.subresources().clone())
    }

    /// Whether a submitted frame had left contents in any mip of any layer of this persistent
    /// backing when the frame started, so the allocation a frame imports is no longer fresh
    /// whichever subresources the frame uses.
    fn initialized(&self) -> bool {
        self.persistent() && self.state().contains(&true)
    }

    fn subresources(&self) -> MutexGuard<'_, Vec<bool>> {
        // A panic while the lock was held leaves the flags as they were.
        self.written.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The flag of `mip` of `layer`, or `None` past the texture's layers or mips.
    fn index(&self, (layer, mip): (u32, u32)) -> Option<usize> {
        let mips = self.declaration.mips;
        (layer < self.declaration.layers && mip < mips).then(|| (layer * mips + mip) as usize)
    }

    /// The attachment view of the subresource `resource` names, made the first time it is asked
    /// for and shared by every frame after.
    fn attachment(&self, resource: ResourceRef) -> Result<AttachmentView, FrameError> {
        let index = self
            .index((resource.layer, resource.mip))
            .ok_or(FrameError::AttachmentView(resource))?;
        let mut views = self.views.lock().unwrap_or_else(PoisonError::into_inner);
        let held = match &mut views.attachments[index] {
            Some(held) => held,
            empty => {
                let view = AttachmentView::new(resource, &self.texture)?;
                self.made.fetch_add(1, Ordering::Relaxed);
                empty.insert(view)
            }
        };
        held.with_resource(resource)
    }

    /// The view of every layer of `mips`, made the first time it is asked for and shared by every
    /// frame after. It is a 2D view of a one-layer target and a 2D array of several, as wgpu's
    /// default view is.
    fn sampled(&self, mips: Range<u32>) -> wgpu::TextureView {
        let count = mips.end - mips.start;
        let mut views = self.views.lock().unwrap_or_else(PoisonError::into_inner);
        views
            .sampled
            .entry((mips.start, count))
            .or_insert_with(|| {
                self.made.fetch_add(1, Ordering::Relaxed);
                self.texture.create_view(&wgpu::TextureViewDescriptor {
                    label: Some(self.declaration.name),
                    base_mip_level: mips.start,
                    mip_level_count: Some(count),
                    ..Default::default()
                })
            })
            .clone()
    }

    /// Keeps the published state as the start of `frame`, before a segment of it stages a
    /// record that changes it.
    pub(super) fn note_frame_start(self: &Arc<Self>, frame: &Publications) {
        frame_start::remember(self, frame, self.subresources().clone());
    }

    fn is_written(&self, subresource: (u32, u32)) -> bool {
        self.persistent()
            && self
                .index(subresource)
                .is_some_and(|index| self.state()[index])
    }

    /// Whether a submitted frame had written every mip of `layer` when the frame started.
    fn is_layer_written(&self, layer: u32) -> bool {
        layer < self.declaration.layers
            && (0..self.declaration.mips).all(|mip| self.is_written((layer, mip)))
    }

    /// Records what a submitted frame left in mips of layers of this persistent backing:
    /// contents a pass can read without writing them first, or nothing.
    pub(super) fn record(&self, subresources: &[((u32, u32), bool)]) {
        let indices: Vec<_> = subresources
            .iter()
            .map(|(subresource, stored)| (self.index(*subresource), *stored))
            .collect();
        let mut written = self.subresources();
        for (index, stored) in indices {
            if let Some(flag) = index.and_then(|index| written.get_mut(index)) {
                *flag = stored;
            }
        }
    }

    /// Names this exact texture's bookkeeping in a capture's write contract.
    pub(crate) fn write_key(self: &Arc<Self>) -> WriteKey {
        WriteKey::Resident(u64::try_from(Arc::as_ptr(self) as usize).expect("resident identity"))
    }
}

/// Where the backing of one declaration of a generation comes from.
enum Source {
    /// The previous generation's backing, whose declaration and extent are identical.
    Kept(Arc<Backing>),
    /// A texture this generation creates, numbered `serial` in the inventory's lineage.
    Fresh(u64),
}

/// The textures behind every declared target of one renderer generation. A generation's store
/// is made privately, from the store of the generation it replaces, and becomes the renderer's
/// only once the whole generation is built, so a replacement that fails leaves the live
/// renderer's store untouched. A backing whose declaration is unchanged is shared with the
/// previous generation, together with what a submitted frame wrote to it.
#[derive(Default)]
pub(crate) struct TargetStore {
    backings: Vec<Arc<Backing>>,
    /// Textures created so far in the lineage this store continues.
    created: u64,
    /// Native views made so far for the textures of the lineage this store continues.
    views: Arc<AtomicU64>,
}

impl TargetStore {
    /// Decides which backing each declaration gets: the previous generation's when its
    /// declaration (every field, with the extent it resolves to) is identical, a new texture
    /// otherwise. The allocation inventory and the store are both made from this one decision.
    fn place(
        previous: Option<&Self>,
        declarations: &[TargetDeclaration],
        frame: [u32; 2],
    ) -> Vec<(Source, [u32; 2])> {
        let mut next = previous.map_or(0, |store| store.created);
        declarations
            .iter()
            .map(|declaration| {
                let extent = declaration.extent_in(frame);
                let kept = previous.and_then(|store| {
                    store.backings.iter().find(|backing| {
                        backing.declaration == *declaration && backing.extent == extent
                    })
                });
                let source = kept.map_or_else(
                    || {
                        next += 1;
                        Source::Fresh(next - 1)
                    },
                    |backing| Source::Kept(backing.clone()),
                );
                (source, extent)
            })
            .collect()
    }

    /// Adds the storage the declarations need to `set`: each kept backing as the one it
    /// already is, so it counts once across generations, and each new texture as storage of its
    /// own, which counts beside the old one until that retires.
    pub(crate) fn allocate(
        set: &mut AllocationSet,
        previous: Option<&Self>,
        declarations: &[TargetDeclaration],
        frame: [u32; 2],
    ) {
        let placed = Self::place(previous, declarations, frame);
        for (declaration, (source, extent)) in declarations.iter().zip(placed) {
            let serial = match source {
                Source::Kept(backing) => backing.serial,
                Source::Fresh(serial) => serial,
            };
            set.backing_texture(
                declaration.name,
                serial,
                [extent[0], extent[1], declaration.layers],
                declaration.format,
                [declaration.mips, 1],
            );
        }
    }

    /// Makes the store of a generation of `frame` texels from `previous`, creating the textures
    /// no previous backing serves. Nothing about `previous` changes.
    pub(crate) fn build(
        device: &wgpu::Device,
        previous: Option<&Self>,
        declarations: &[TargetDeclaration],
        frame: [u32; 2],
    ) -> Self {
        let placed = Self::place(previous, declarations, frame);
        let mut created = previous.map_or(0, |store| store.created);
        let views = previous.map_or_else(Arc::default, |store| store.views.clone());
        let backings = declarations
            .iter()
            .zip(placed)
            .map(|(declaration, (source, extent))| match source {
                Source::Kept(backing) => backing,
                Source::Fresh(serial) => {
                    created += 1;
                    Arc::new(Backing::create(
                        device,
                        declaration,
                        extent,
                        serial,
                        views.clone(),
                    ))
                }
            })
            .collect();
        Self {
            backings,
            created,
            views,
        }
    }

    pub(crate) fn created(&self) -> u64 {
        self.created
    }

    /// Every texture of the store, which the frame's passes may write, with the initial version
    /// the frame imports it at: the one [`Self::view`] declares to the stock compile, which is
    /// `Some(0)` for a persistent backing a submitted frame had written when the frame started
    /// and `None` otherwise.
    pub(crate) fn imports(&self) -> Vec<(&wgpu::Texture, Option<u32>)> {
        self.backings
            .iter()
            .map(|backing| (&backing.texture, backing.initialized().then_some(0)))
            .collect()
    }

    /// The keys of the retained-state bookkeeping a frame publishes for the persistent backings.
    pub(crate) fn write_keys(&self) -> Vec<WriteKey> {
        self.backings
            .iter()
            .filter(|backing| backing.persistent())
            .map(Backing::write_key)
            .collect()
    }

    pub(super) fn backing(&self, name: &str) -> Option<&Arc<Backing>> {
        self.backings
            .iter()
            .find(|backing| backing.declaration.name == name)
    }

    fn texture(&self, name: &str) -> Option<wgpu::Texture> {
        self.backing(name).map(|backing| backing.texture.clone())
    }

    fn view(&self, resource: ResourceRef) -> Result<AttachmentView, FrameError> {
        let backing = self
            .backing(resource.name)
            .ok_or(FrameError::AttachmentView(resource))?;
        let view = backing.attachment(resource)?;
        Ok(if backing.is_written((resource.layer, resource.mip)) {
            view.with_retained_contents()
        } else if backing.initialized() {
            view.with_initialized_allocation()
        } else {
            view
        })
    }

    fn written(&self, name: &str, layer: u32) -> bool {
        self.backing(name)
            .is_some_and(|backing| backing.is_layer_written(layer))
    }

    fn mip_written(&self, name: &str, subresource: (u32, u32)) -> bool {
        self.backing(name)
            .is_some_and(|backing| backing.is_written(subresource))
    }

    /// The number of mips of the target `name`, or `None` for a name no declaration made.
    fn mips(&self, name: &str) -> Option<u32> {
        self.backing(name).map(|backing| backing.declaration.mips)
    }

    /// The persistent backing of `texture`, by its position in the store.
    fn persistent_index(&self, texture: &wgpu::Texture) -> Option<usize> {
        self.backings
            .iter()
            .position(|backing| backing.persistent() && backing.texture == *texture)
    }

    /// What the frame being prepared leaves in the persistent targets of this store, by exact
    /// backing: for each mip of each layer, whether the last write stored its contents. The
    /// frame's prepared writes come first and always store; an attachment or a compute pass's
    /// storage write of the subresource later decides.
    /// A stock attachment that shares a target's name is another texture and counts for nothing.
    pub(super) fn frame_effects(&self, context: &PrepareContext<'_>) -> Vec<FrameEffect> {
        let mut last = BTreeMap::new();
        for view in context.prepared_views() {
            if let Some(index) = self.persistent_index(view.texture()) {
                let resource = view.resource();
                last.insert((index, resource.layer, resource.mip), true);
            }
        }
        for (resource, stored) in context.passes().iter().flat_map(PassDeclaration::written) {
            let Ok(view) = context.resource(resource) else {
                continue;
            };
            let Some(index) = self.persistent_index(view.texture()) else {
                continue;
            };
            last.insert((index, resource.layer, resource.mip), stored);
        }
        let mut effects: Vec<FrameEffect> = Vec::new();
        for ((index, layer, mip), stored) in last {
            let backing = &self.backings[index];
            match effects.last_mut() {
                Some(effect) if Arc::ptr_eq(&effect.backing, backing) => {
                    effect.written.push(((layer, mip), stored));
                }
                _ => effects.push(FrameEffect {
                    backing: backing.clone(),
                    written: vec![((layer, mip), stored)],
                }),
            }
        }
        effects
    }
}

/// What one frame leaves in one persistent backing: for each (layer, mip) it wrote, whether the
/// last write stored its contents.
pub(super) struct FrameEffect {
    pub(super) backing: Arc<Backing>,
    pub(super) written: Vec<((u32, u32), bool)>,
}

impl FrameEffect {
    /// Stages the recording of this effect, to run when `frame` is submitted. The capture records
    /// a refusal and fails before submission, as for stock state.
    pub(super) fn stage(self, frame: &mut Publications) {
        let Self { backing, written } = self;
        backing.note_frame_start(frame);
        let key = backing.write_key();
        let _ = frame.stage_write(key, move || backing.record(&written));
    }
}

/// A game's view of its targets for one renderer generation. Cloning shares the same textures.
#[derive(Clone)]
pub struct GameTargets {
    store: Arc<TargetStore>,
    /// Plans the passes after the depth prepass, and only those.
    depth_prefix: bool,
    snapshots: SnapshotReads,
}

impl GameTargets {
    pub(crate) fn new(store: Arc<TargetStore>) -> Self {
        Self {
            store,
            depth_prefix: false,
            snapshots: SnapshotReads::NONE,
        }
    }

    /// The targets as a factory's depth-prefix feature plans with them.
    pub(crate) fn for_depth_prefix(store: Arc<TargetStore>) -> Self {
        Self {
            store,
            depth_prefix: true,
            snapshots: SnapshotReads::NONE,
        }
    }

    /// Whether these targets plan the passes after the depth prepass, the only place a
    /// depth-prefix feature draws in.
    pub fn depth_prefix(&self) -> bool {
        self.depth_prefix
    }

    /// These targets for the factory that declared `reads`, which is what its passes may read of
    /// the frame's opaque snapshots.
    #[must_use]
    pub(crate) fn with_snapshot_reads(mut self, reads: SnapshotReads) -> Self {
        self.snapshots = reads;
        self
    }

    /// The opaque snapshots the factory these targets belong to declared it reads.
    pub fn snapshot_reads(&self) -> SnapshotReads {
        self.snapshots
    }

    /// The texture of a declared target, for a sampled view or a copy. The texture is the same
    /// object in every frame of a generation, and in later generations of the same renderer
    /// while the declaration is unchanged. Its default view covers every layer and every mip.
    ///
    /// # Errors
    /// [`FrameError::AttachmentView`] for a name no declaration made.
    pub fn texture(&self, name: &'static str) -> Result<wgpu::Texture, FrameError> {
        self.store
            .texture(name)
            .ok_or(FrameError::AttachmentView(ResourceRef::new(name)))
    }

    /// The texture of a declared target through [`Self::sampled_view`], for the binding sourced
    /// from `semantic` in a draw or set that samples it.
    ///
    /// # Errors
    /// [`FrameError::AttachmentView`] for a name no declaration made.
    pub fn import(
        &self,
        name: &'static str,
        semantic: worldrunner_render_program::binding::EngineSemantic,
    ) -> Result<worldrunner_render_program::runtime::Import, FrameError> {
        Ok(worldrunner_render_program::runtime::Import::texture(
            semantic,
            self.texture(name)?,
            self.sampled_view(name)?,
        ))
    }

    /// The view a pass that reads the target binds: every layer and every mip, which is what a
    /// pass's read of a layer declares it samples. It is a 2D view of a one-layer target and a
    /// 2D array of several, as wgpu's default view is and as a program binding of the texture
    /// is checked.
    ///
    /// # Errors
    /// [`FrameError::AttachmentView`] for a name no declaration made.
    pub fn sampled_view(&self, name: &'static str) -> Result<wgpu::TextureView, FrameError> {
        let backing = self.backing(name)?;
        Ok(backing.sampled(0..backing.declaration.mips))
    }

    /// The view a pass that reads some mips of the target binds: every layer of the mips `mips`.
    /// It is a 2D view of a one-layer target and a 2D array of several. A pass samples it while it
    /// renders into a mip outside the range, because the views cover different subresources.
    ///
    /// # Errors
    /// [`FrameError::AttachmentView`] for a name no declaration made, naming the target, and for
    /// an empty range or one past the target's mips, naming the first mip it cannot cover.
    pub fn sampled_mips(
        &self,
        name: &'static str,
        mips: std::ops::Range<u32>,
    ) -> Result<wgpu::TextureView, FrameError> {
        let backing = self.backing(name)?;
        let count = backing.declaration.mips;
        if mips.is_empty() || mips.end > count {
            let mip = if mips.is_empty() {
                mips.start
            } else {
                mips.start.max(count)
            };
            return Err(FrameError::AttachmentView(ResourceRef {
                mip,
                ..ResourceRef::new(name)
            }));
        }
        Ok(backing.sampled(mips))
    }

    /// The checked attachment view of one mip of one layer. A persistent subresource that a
    /// submitted frame had written when the frame being planned started carries retained
    /// contents, so a pass may read it without writing it first. Like [`Self::is_written`], the
    /// view reports the frame's start in every segment of the frame.
    ///
    /// # Errors
    /// [`FrameError::AttachmentView`] for an undeclared target or a layer or mip beyond its
    /// count.
    pub fn view(&self, resource: ResourceRef) -> Result<AttachmentView, FrameError> {
        self.store.view(resource)
    }

    /// Whether a submitted frame had written every mip of `layer` of the persistent target
    /// `name` when the frame being planned started, which a pass that reads the layer needs.
    /// Every segment of one frame, both parts of an owned capture
    /// included, gets the same answer, so a feature plans the same passes in each. A segment
    /// that records its writes when it is submitted does not change the answer for the segments
    /// after it. A query made between frames answers with what has been published.
    pub fn is_written(&self, name: &'static str, layer: u32) -> bool {
        self.store.written(name, layer)
    }

    /// Whether a submitted frame had written `mip` of `layer` of the persistent target `name`
    /// when the frame being planned started, as [`Self::is_written`] answers.
    pub fn is_mip_written(&self, name: &'static str, layer: u32, mip: u32) -> bool {
        self.store.mip_written(name, (layer, mip))
    }

    /// The backing of the declared target `name`.
    fn backing(&self, name: &'static str) -> Result<&Arc<Backing>, FrameError> {
        self.store
            .backing(name)
            .ok_or(FrameError::AttachmentView(ResourceRef::new(name)))
    }

    /// The mips of the declared target `name`, or `None` for a name no declaration made.
    pub(super) fn mips(&self, name: &str) -> Option<u32> {
        self.store.mips(name)
    }

    /// Textures the renderer has created for its targets since it was made. It stays level
    /// across steady frames, and across replacements that leave the declarations unchanged.
    pub fn created(&self) -> u64 {
        self.store.created()
    }

    /// Native texture views the renderer has made of its targets since it was made. It stays
    /// level across steady frames, and across replacements that leave the declarations unchanged.
    pub fn views_created(&self) -> u64 {
        self.store.views.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
#[path = "targets_tests.rs"]
mod targets_tests;
