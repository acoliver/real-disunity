//! Targets a game declares: allocated once, shared by its passes and, when persistent, kept
//! across frames and renderer replacements.
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use worldrunner_render_gpu::configuration::AllocationSet;
use worldrunner_render_post::{AttachmentView, FrameError, PostError, ResourceRef};

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

    pub(super) fn allocate(&self, set: &mut AllocationSet, frame: [u32; 2]) {
        let [width, height] = self.extent_in(frame);
        set.texture(
            self.name,
            [width, height, self.layers],
            self.format,
            [1, 1],
            1,
        );
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

/// One declared target and what the store holds for it.
struct Slot {
    declaration: TargetDeclaration,
    texture: wgpu::Texture,
    extent: [u32; 2],
    /// Per layer: whether a submitted frame has written it.
    written: Vec<bool>,
}

#[derive(Default)]
struct State {
    slots: Vec<Slot>,
    created: u64,
}

/// The textures behind every declared target, shared by every renderer generation of one
/// inventory. Persistent textures outlive the generation that made them while their declaration
/// is unchanged.
#[derive(Default)]
pub(super) struct TargetStore(Mutex<State>);

impl TargetStore {
    fn state(&self) -> MutexGuard<'_, State> {
        // A panic while the lock was held leaves the slots as they were.
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Makes the store hold exactly `declarations` for a frame of `frame` texels. A target whose
    /// shape is unchanged keeps its texture and its written layers; any other is recreated and
    /// reads as unwritten. A target no longer declared is released.
    pub(super) fn resolve(
        &self,
        device: &wgpu::Device,
        declarations: &[TargetDeclaration],
        frame: [u32; 2],
    ) {
        let mut state = self.state();
        let mut previous = std::mem::take(&mut state.slots);
        for declaration in declarations {
            let extent = declaration.extent_in(frame);
            let kept = previous
                .iter()
                .position(|slot| slot.declaration.name == declaration.name)
                .map(|index| previous.swap_remove(index))
                .filter(|slot| {
                    slot.extent == extent
                        && slot.declaration.format == declaration.format
                        && slot.declaration.layers == declaration.layers
                        && slot.declaration.usage == declaration.usage
                });
            let slot = if let Some(mut slot) = kept {
                slot.declaration = declaration.clone();
                slot
            } else {
                state.created += 1;
                Slot {
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
                    extent,
                    written: vec![false; declaration.layers as usize],
                    declaration: declaration.clone(),
                }
            };
            state.slots.push(slot);
        }
    }

    pub(super) fn created(&self) -> u64 {
        self.state().created
    }

    fn texture(&self, name: &str) -> Option<wgpu::Texture> {
        self.state()
            .slots
            .iter()
            .find(|slot| slot.declaration.name == name)
            .map(|slot| slot.texture.clone())
    }

    fn view(&self, resource: ResourceRef) -> Result<AttachmentView, FrameError> {
        let state = self.state();
        let slot = state
            .slots
            .iter()
            .find(|slot| slot.declaration.name == resource.name)
            .ok_or(FrameError::AttachmentView(resource))?;
        let view = AttachmentView::new(resource, &slot.texture)?;
        let kept = slot.declaration.lifetime == TargetLifetime::Persistent
            && slot.written.get(resource.layer as usize) == Some(&true);
        Ok(if kept {
            view.with_retained_contents()
        } else {
            view
        })
    }

    fn written(&self, name: &str, layer: u32) -> bool {
        self.state()
            .slots
            .iter()
            .find(|slot| slot.declaration.name == name)
            .is_some_and(|slot| slot.written.get(layer as usize) == Some(&true))
    }

    /// Records that a submitted frame wrote `layer` of the persistent target `name`.
    pub(super) fn mark_written(&self, name: &str, layer: u32) {
        let mut state = self.state();
        if let Some(slot) = state
            .slots
            .iter_mut()
            .find(|slot| slot.declaration.name == name)
            .filter(|slot| slot.declaration.lifetime == TargetLifetime::Persistent)
            && let Some(written) = slot.written.get_mut(layer as usize)
        {
            *written = true;
        }
    }
}

/// A game's view of its targets for one renderer generation. Cloning shares the same textures.
#[derive(Clone)]
pub struct GameTargets {
    store: Arc<TargetStore>,
}

impl GameTargets {
    pub(super) fn new(store: Arc<TargetStore>) -> Self {
        Self { store }
    }

    /// The texture of a declared target, for a sampled view or a copy. The texture is the same
    /// object in every frame of a generation, and in later generations while the declaration is
    /// unchanged.
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

    /// Textures the inventory has created since it was made. It stays level across steady
    /// frames, and across renderer replacements that leave the declarations unchanged.
    pub fn created(&self) -> u64 {
        self.store.created()
    }
}
