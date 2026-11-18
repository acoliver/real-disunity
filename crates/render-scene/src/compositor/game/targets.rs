//! Targets a game declares: allocated once, shared by its passes and, when persistent, kept
//! across frames and renderer replacements.
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use worldrunner_render_gpu::configuration::AllocationSet;
use worldrunner_render_gpu::submission::capture::WriteKey;
use worldrunner_render_post::{AttachmentView, FrameError, PostError, PrepareContext, ResourceRef};

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

/// A named texture a game owns: one 2D texture of one or more layers, with one render
/// attachment view per layer and any sampled view the game makes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetDeclaration {
    /// Names the target in the plan, in errors and in the allocation inventory.
    pub name: &'static str,
    /// A renderable colour format.
    pub format: wgpu::TextureFormat,
    pub extent: TargetExtent,
    /// Array layers, at least one.
    pub layers: u32,
    pub lifetime: TargetLifetime,
    /// Usage beyond `RENDER_ATTACHMENT | TEXTURE_BINDING`, which every declared target has.
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
            lifetime: TargetLifetime::PerFrame,
            usage: wgpu::TextureUsages::empty(),
        }
    }

    #[must_use]
    pub const fn layers(mut self, layers: u32) -> Self {
        self.layers = layers;
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

    /// The resource a pass names to attach or read `layer` of this target.
    pub const fn layer(&self, layer: u32) -> ResourceRef {
        ResourceRef {
            name: self.name,
            version: 0,
            mip: 0,
            layer,
        }
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

    fn texture_usage(&self) -> wgpu::TextureUsages {
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | self.usage
    }

    /// The refusal of a declaration the device cannot allocate, naming the target.
    pub(super) fn validate(&self, limits: &wgpu::Limits, frame: [u32; 2]) -> Result<(), PostError> {
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
        let features = self
            .format
            .guaranteed_format_features(wgpu::Features::empty());
        if self.format.is_depth_stencil_format()
            || !features
                .allowed_usages
                .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
        {
            return refuse("format is not a renderable colour format");
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
    /// Per layer: whether a submitted frame has written it.
    written: Mutex<Vec<bool>>,
}

impl Backing {
    fn create(
        device: &wgpu::Device,
        declaration: &TargetDeclaration,
        extent: [u32; 2],
        serial: u64,
    ) -> Self {
        Self {
            texture: device.create_texture(&wgpu::TextureDescriptor {
                label: Some(declaration.name),
                size: wgpu::Extent3d {
                    width: extent[0],
                    height: extent[1],
                    depth_or_array_layers: declaration.layers,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: declaration.format,
                usage: declaration.texture_usage(),
                view_formats: &[],
            }),
            written: Mutex::new(vec![false; declaration.layers as usize]),
            declaration: declaration.clone(),
            extent,
            serial,
        }
    }

    fn persistent(&self) -> bool {
        self.declaration.lifetime == TargetLifetime::Persistent
    }

    fn layers(&self) -> MutexGuard<'_, Vec<bool>> {
        // A panic while the lock was held leaves the flags as they were.
        self.written.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn is_written(&self, layer: u32) -> bool {
        self.persistent() && self.layers().get(layer as usize) == Some(&true)
    }

    /// Records what a submitted frame left in layers of this persistent backing: contents a
    /// pass can read without writing them first, or nothing.
    pub(super) fn record(&self, layers: &[(u32, bool)]) {
        let mut written = self.layers();
        for (layer, stored) in layers {
            if let Some(flag) = written.get_mut(*layer as usize) {
                *flag = *stored;
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
                [1, 1],
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
        let backings = declarations
            .iter()
            .zip(placed)
            .map(|(declaration, (source, extent))| match source {
                Source::Kept(backing) => backing,
                Source::Fresh(serial) => {
                    created += 1;
                    Arc::new(Backing::create(device, declaration, extent, serial))
                }
            })
            .collect();
        Self { backings, created }
    }

    pub(crate) fn created(&self) -> u64 {
        self.created
    }

    /// Every texture of the store, which the frame's passes may write.
    pub(crate) fn textures(&self) -> Vec<&wgpu::Texture> {
        self.backings
            .iter()
            .map(|backing| &backing.texture)
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

    fn backing(&self, name: &str) -> Option<&Arc<Backing>> {
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
        let view = AttachmentView::new(resource, &backing.texture)?;
        Ok(if backing.is_written(resource.layer) {
            view.with_retained_contents()
        } else {
            view
        })
    }

    fn written(&self, name: &str, layer: u32) -> bool {
        self.backing(name)
            .is_some_and(|backing| backing.is_written(layer))
    }

    /// What the frame being prepared leaves in the persistent targets of this store, by exact
    /// backing: for each layer, whether the last attachment that wrote it stored its contents.
    /// A stock attachment that shares a target's name is another texture and counts for nothing.
    pub(super) fn frame_effects(
        &self,
        context: &PrepareContext<'_>,
    ) -> Vec<(Arc<Backing>, Vec<(u32, bool)>)> {
        let mut last = BTreeMap::new();
        for attachment in context
            .passes()
            .iter()
            .flat_map(|pass| {
                pass.color
                    .iter()
                    .chain(&pass.additional_colors)
                    .chain(&pass.depth)
            })
            .filter(|attachment| !attachment.read_only)
        {
            let Ok(view) = context.resource(attachment.resource) else {
                continue;
            };
            let Some(index) = self
                .backings
                .iter()
                .position(|backing| backing.persistent() && backing.texture == *view.texture())
            else {
                continue;
            };
            let stored = attachment.store == wgpu::StoreOp::Store;
            last.insert((index, attachment.resource.layer), stored);
        }
        let mut effects: Vec<(Arc<Backing>, Vec<(u32, bool)>)> = Vec::new();
        for ((index, layer), stored) in last {
            let backing = &self.backings[index];
            match effects.last_mut() {
                Some((known, layers)) if Arc::ptr_eq(known, backing) => {
                    layers.push((layer, stored))
                }
                _ => effects.push((backing.clone(), vec![(layer, stored)])),
            }
        }
        effects
    }
}

/// A game's view of its targets for one renderer generation. Cloning shares the same textures.
#[derive(Clone)]
pub struct GameTargets {
    store: Arc<TargetStore>,
}

impl GameTargets {
    pub(crate) fn new(store: Arc<TargetStore>) -> Self {
        Self { store }
    }

    /// The texture of a declared target, for a sampled view or a copy. The texture is the same
    /// object in every frame of a generation, and in later generations of the same renderer
    /// while the declaration is unchanged.
    ///
    /// # Errors
    /// [`FrameError::AttachmentView`] for a name no declaration made.
    pub fn texture(&self, name: &'static str) -> Result<wgpu::Texture, FrameError> {
        self.store
            .texture(name)
            .ok_or(FrameError::AttachmentView(ResourceRef::new(name)))
    }

    /// The checked attachment view of one layer. A persistent layer that a submitted frame has
    /// written carries retained contents, so a pass may read it without writing it first.
    ///
    /// # Errors
    /// [`FrameError::AttachmentView`] for an undeclared target or a layer beyond its count.
    pub fn view(&self, resource: ResourceRef) -> Result<AttachmentView, FrameError> {
        self.store.view(resource)
    }

    /// Whether a submitted frame has written `layer` of the persistent target `name`.
    pub fn is_written(&self, name: &'static str, layer: u32) -> bool {
        self.store.written(name, layer)
    }

    /// Textures the renderer has created for its targets since it was made. It stays level
    /// across steady frames, and across replacements that leave the declarations unchanged.
    pub fn created(&self) -> u64 {
        self.store.created()
    }
}
