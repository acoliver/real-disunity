//! The inventory of a game's feature factories, and their registration on each renderer
//! generation.
use super::camera::FrameCameraHandle;
use super::occlusion::FrameOcclusionHandle;
use super::shadows::{FrameShadowHandle, FrameShadows};
use super::targets::{FrameEffect, GameTargets, TargetDeclaration, TargetStore};
use crate::compositor::FrameRenderer;
use crate::device::GpuContext;
use std::sync::Arc;
use worldrunner_render_gpu::configuration::AllocationSet;
use worldrunner_render_post::{
    AttachmentView, BuildContext, DrawContext, FeatureDeclaration, FrameConfig, FrameError,
    FrameFeature, PassDeclaration, PostError, PrepareContext, PreparedFeature,
};

/// What a factory is given to build the feature of one renderer generation.
pub struct GameBuild<'a> {
    /// The device, queue and submission authority the renderer was built on, for a program
    /// runtime or any other resource the feature owns.
    pub context: &'a GpuContext,
    /// The frame the generation renders: extents, format and samples.
    pub frame: FrameConfig,
    /// The targets the inventory declares, resolved for this generation.
    pub targets: GameTargets,
    /// The camera of the frame in flight, set as each frame plans.
    pub camera: FrameCameraHandle,
    /// The stock shadow cascades of the frame in flight, published by the shadowed route.
    pub shadows: FrameShadowHandle,
    /// The stock ambient occlusion of the frame in flight, set as each frame begins encoding.
    pub occlusion: FrameOcclusionHandle,
}

/// A game's contribution to the retained renderer: the targets it owns and the frame feature
/// that draws into them.
///
/// The factory is called once for each renderer generation: when the renderer is allocated and
/// again after every replacement, so a feature never outlives the resources it was built
/// against. The factory itself is the game's own inventory entry and keeps whatever must
/// outlive a generation.
pub trait GameFeatureFactory: Send + Sync {
    /// Names the contribution in errors.
    fn name(&self) -> &'static str;

    /// The targets this contribution owns, for the allocation budget and the store.
    fn targets(&self) -> Vec<TargetDeclaration> {
        Vec::new()
    }

    /// Builds the feature for one generation. It plans its passes with
    /// [`GameTargets::plan_pass`] and draws in them like any other frame feature.
    ///
    /// # Errors
    /// Whatever refuses the feature; the renderer is not built.
    fn build(&self, context: &GameBuild<'_>) -> Result<Box<dyn FrameFeature>, FrameError>;
}

/// The inventoried game contributions of a renderer configuration. Empty by default, in which
/// case frames are exactly the stock frames. Cloning shares the factories only. The targets
/// belong to the renderer built from the inventory: it carries its persistent targets across its
/// own replacements, and a second renderer built from a clone makes targets of its own.
#[derive(Clone, Default)]
pub struct GamePasses {
    factories: Vec<Arc<dyn GameFeatureFactory>>,
}

impl std::fmt::Debug for GamePasses {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_list()
            .entries(self.factories.iter().map(|factory| factory.name()))
            .finish()
    }
}

impl GamePasses {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a factory to the inventory.
    #[must_use]
    pub fn with(mut self, factory: Arc<dyn GameFeatureFactory>) -> Self {
        self.factories.push(factory);
        self
    }

    pub fn is_empty(&self) -> bool {
        self.factories.is_empty()
    }

    pub fn len(&self) -> usize {
        self.factories.len()
    }

    fn declarations(&self) -> Vec<TargetDeclaration> {
        self.factories
            .iter()
            .flat_map(|factory| factory.targets())
            .collect()
    }

    /// The storage the declared targets add to a renderer's allocation inventory, given the
    /// store of the generation the renderer replaces, if any: a backing that generation keeps is
    /// not new storage.
    pub(crate) fn allocations(
        &self,
        frame: [u32; 2],
        previous: Option<&TargetStore>,
    ) -> AllocationSet {
        let mut set = AllocationSet::default();
        TargetStore::allocate(&mut set, previous, &self.declarations(), frame);
        set
    }

    /// Refuses a declaration the device cannot allocate and a name declared twice.
    pub(crate) fn validate(&self, frame: [u32; 2], limits: &wgpu::Limits) -> Result<(), PostError> {
        let declarations = self.declarations();
        for (index, declaration) in declarations.iter().enumerate() {
            declaration.validate(limits, frame)?;
            if declarations[..index]
                .iter()
                .any(|other| other.name == declaration.name)
            {
                return Err(PostError::Allocation(format!(
                    "game target {} is declared twice",
                    declaration.name
                )));
            }
        }
        Ok(())
    }

    /// Makes the declared targets of a new generation from `previous`, the store of the
    /// generation `renderer` replaces, and registers every factory's feature on `renderer`. This
    /// runs for each renderer generation, so a replacement registers again. Nothing of the live
    /// generation is touched: the new store becomes the renderer's own, and the caller's, only
    /// when the whole generation is built.
    pub(crate) fn install(
        &self,
        renderer: &mut FrameRenderer,
        context: &GpuContext,
        previous: Option<&TargetStore>,
    ) -> Result<(), PostError> {
        if self.factories.is_empty() {
            return Ok(());
        }
        let frame = renderer.targets().config();
        let store = Arc::new(TargetStore::build(
            context.device(),
            previous,
            &self.declarations(),
            frame.size,
        ));
        for factory in &self.factories {
            let feature = factory.build(&GameBuild {
                context,
                frame,
                targets: GameTargets::new(store.clone()),
                camera: renderer.game_camera.clone(),
                shadows: renderer.game.shadows.clone(),
                occlusion: renderer.game.occlusion.clone(),
            })?;
            renderer.chain.register(
                context.device(),
                Box::new(Registered {
                    feature,
                    store: store.clone(),
                }),
            )?;
            renderer.game.features += 1;
        }
        renderer.game.store = Some(store);
        Ok(())
    }
}

/// What the game inventory installed on one renderer generation.
#[derive(Default)]
pub(crate) struct Installed {
    /// How many features the factories registered on the chain.
    pub(crate) features: usize,
    /// The textures behind the declared targets: made from the previous generation's, and handed
    /// on to the next one.
    pub(crate) store: Option<Arc<TargetStore>>,
    /// Where the shadowed route lends its cascades to the features of this generation.
    pub(crate) shadows: FrameShadowHandle,
    /// Where every route lends its ambient occlusion to the features of this generation.
    pub(crate) occlusion: FrameOcclusionHandle,
}

/// A factory's feature as the chain holds it: the feature itself, plus the one thing the
/// inventory owns, which layers of persistent targets a submitted frame has written.
struct Registered {
    feature: Box<dyn FrameFeature>,
    store: Arc<TargetStore>,
}

impl FrameFeature for Registered {
    fn declare(&self) -> FeatureDeclaration {
        self.feature.declare()
    }

    fn plan(
        &self,
        passes: &mut Vec<PassDeclaration>,
        views: &mut Vec<AttachmentView>,
    ) -> Result<FeatureDeclaration, FrameError> {
        self.feature.plan(passes, views)
    }

    fn build(&self, context: &BuildContext<'_>) -> Result<Box<dyn PreparedFeature>, FrameError> {
        Ok(Box::new(RegisteredPrepared {
            prepared: self.feature.build(context)?,
            store: self.store.clone(),
        }))
    }
}

struct RegisteredPrepared {
    prepared: Box<dyn PreparedFeature>,
    store: Arc<TargetStore>,
}

impl PreparedFeature for RegisteredPrepared {
    fn prepare(&mut self, context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        self.prepared.prepare(context)
    }

    fn commit(&mut self, context: &mut PrepareContext<'_>) {
        self.prepared.commit(context);
        // What a frame left in the persistent targets is recorded when it is submitted, never
        // before: a frame that is cancelled, or whose capture fails, leaves them as they were.
        // Each publication names the exact backing it records, which an owned capture needs to
        // admit it; a frame that writes no declared target publishes nothing.
        for FrameEffect { backing, layers } in self.store.frame_effects(context) {
            let key = backing.write_key();
            // The capture records a refusal and fails before submission, as for stock state.
            let _ = context
                .publications()
                .stage_write(key, move || backing.record(&layers));
        }
    }

    fn finish_draws(&mut self, context: &mut PrepareContext<'_>) {
        self.prepared.finish_draws(context);
    }

    fn draw(&self, context: &DrawContext, pass: &mut wgpu::RenderPass<'_>) {
        self.prepared.draw(context, pass);
    }
}

impl FrameRenderer {
    /// Lends the stock shadow cascades of the frame being prepared to the game's features, when
    /// the renderer has any. `lend` runs only then, so a renderer without game features builds
    /// nothing.
    pub(crate) fn publish_game_shadows(&self, lend: impl FnOnce() -> Option<FrameShadows>) {
        if self.game.features > 0
            && let Some(shadows) = lend()
        {
            self.game.shadows.publish(shadows);
        }
    }

    /// The textures the game's targets add to a capture's write contract, each with the initial
    /// version the frame imports it at: the targets the frame's passes attach.
    pub(crate) fn game_capture_imports(&self) -> Vec<(wgpu::Texture, Option<u32>)> {
        self.game.store.as_ref().map_or_else(Vec::new, |store| {
            store
                .imports()
                .into_iter()
                .map(|(texture, initial)| (texture.clone(), initial))
                .collect()
        })
    }

    /// The keys of the retained-state bookkeeping the game's frames publish on submission, one
    /// for each persistent target.
    pub(crate) fn game_capture_publications(
        &self,
    ) -> Vec<worldrunner_render_gpu::submission::capture::WriteKey> {
        self.game
            .store
            .as_ref()
            .map_or_else(Vec::new, |store| store.write_keys())
    }
}
