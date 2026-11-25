//! The inventory of a game's feature factories, and their registration on each renderer
//! generation.
use super::camera::FrameCameraHandle;
use super::engine::EngineConstants;
use super::environment::FrameEnvironmentHandle;
use super::occlusion::FrameOcclusionHandle;
use super::replacement::StockReplacement;
use super::resources::GameResources;
use super::resources::SharedTextures;
use super::scene_meshes::FrameSceneMeshesHandle;
use super::scene_textures::FrameSceneTexturesHandle;
use super::shadows::FrameShadowHandle;
use super::shadows::FrameShadows;
use super::snapshots::FrameSnapshotHandle;
use super::snapshots::SnapshotReads;
use super::targets::GameTargets;
use super::targets::TargetDeclaration;
use super::targets::TargetStore;
use super::terrain::FrameTerrainHandle;
use crate::compositor::FrameRenderer;
use crate::device::GpuContext;
use std::sync::Arc;
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_gpu::configuration::AllocationSet;
use worldrunner_render_post::AttachmentView;
use worldrunner_render_post::BuildContext;
use worldrunner_render_post::DrawContext;
use worldrunner_render_post::FeatureDeclaration;
use worldrunner_render_post::FrameConfig;
use worldrunner_render_post::FrameError;
use worldrunner_render_post::FrameFeature;
use worldrunner_render_post::PassDeclaration;
use worldrunner_render_post::PostError;
use worldrunner_render_post::PrepareContext;
use worldrunner_render_post::PreparedFeature;

/// What a factory is given to build the feature of one renderer generation.
pub struct GameBuild<'a> {
    /// The device, queue and submission authority the renderer was built on. A feature that
    /// keeps its resources with the renderer declares them to [`Self::resources`] instead.
    pub context: &'a GpuContext,
    /// The buffers, textures and samplers the factory declares, and the program runtime its
    /// features draw with, created and owned by this generation and shared by the factory's
    /// depth-prefix and ordinary features.
    pub resources: GameResources,
    /// The frame the generation renders: extents, format and samples.
    pub frame: FrameConfig,
    /// The targets the inventory declares, resolved for this generation.
    pub targets: GameTargets,
    /// The camera of the frame in flight, set as each frame plans.
    pub camera: FrameCameraHandle,
    /// The streamed terrain chunks of the frame in flight, set as each frame prepares its
    /// terrain.
    pub terrain: FrameTerrainHandle,
    /// The stock shadow cascades of the frame in flight, published by the shadowed route.
    pub shadows: FrameShadowHandle,
    /// The stock ambient occlusion of the frame in flight, set as each frame begins encoding.
    pub occlusion: FrameOcclusionHandle,
    /// The installed specular environment cube of the frame in flight, set before each frame
    /// plans.
    pub environment: FrameEnvironmentHandle,
    /// The scene's resident textures, lent in every frame and in a depth prefix submitted ahead
    /// of the frame.
    pub scene_textures: FrameSceneTexturesHandle,
    /// The scene's resident meshes, lent wherever the textures are.
    pub scene_meshes: FrameSceneMeshesHandle,
    /// The constants of this generation: placeholder texels, the split-sum BRDF table and the
    /// full-screen triangle.
    pub engine: EngineConstants,
    /// The opaque snapshots of the frame in flight, published once the frame is planned and only
    /// when a pass of it declared a read of them.
    pub snapshots: FrameSnapshotHandle,
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

    /// The bytes [`GameBuild::resources`] may hold: its declared buffers and textures with what
    /// its program runtime allocates and the terrain material arrays
    /// [`EngineConstants::material_array`] lends its features. None by default, so a factory
    /// whose features ask for arrays states a budget that covers them.
    fn resource_budget(&self) -> AllocationBudget {
        AllocationBudget(0)
    }

    /// The frame's opaque snapshots that this contribution's passes may read. The default reads
    /// none, and then a renderer with no water, reflection or depth-tested overlay allocates no
    /// snapshots at all. A factory that samples them states it here so that the renderer reserves
    /// and allocates them when it is built, and so that a same-frame capture admits their writes
    /// before the first frame plans. Each pass then declares what it samples with
    /// [`GamePassSpec::reading_snapshots`], and a pass that declares more than this is refused at
    /// planning.
    fn snapshot_reads(&self) -> SnapshotReads {
        SnapshotReads::NONE
    }

    /// The stock surfaces this contribution's passes draw themselves. The default replaces
    /// nothing. When any factory of the inventory declares the terrain surface or the water, the
    /// renderer built from it draws neither in its stock frames, and the declaration holds for
    /// every frame of that generation.
    fn replaces_stock(&self) -> StockReplacement {
        StockReplacement::NONE
    }

    /// Builds the feature for one generation. It plans its passes with
    /// [`GameTargets::plan_pass`] and draws in them like any other frame feature.
    ///
    /// # Errors
    /// Whatever refuses the feature; the renderer is not built.
    fn build(&self, context: &GameBuild<'_>) -> Result<Box<dyn FrameFeature>, FrameError>;

    /// Builds the feature that adds the game's passes after the depth prepass
    /// ([`GamePlace::AfterDepthPrepass`](worldrunner_render_post::GamePlace::AfterDepthPrepass)),
    /// or `None`, the default, for a game that adds none.
    ///
    /// These passes complete the depth prefix that the depth pyramid, the occlusion metadata,
    /// SSAO and shadow depth fitting read, so a route that submits that prefix ahead of the frame
    /// runs them inside it. That is why they belong to a feature of their own: the renderer plans
    /// and prepares it once for the prefix, with only the prefix's passes in the list, and does
    /// not prepare or draw it again in the rest of the frame. In a frame drawn whole it plans,
    /// prepares and draws like the ordinary feature. It is built before the ordinary feature of
    /// the same generation, from the same `context` except that `context.targets` plans only
    /// this place, and it is prepared before it in a frame drawn whole.
    ///
    /// A depth-prefix feature reads the lent terrain only; shadow, occlusion and environment
    /// inputs are `NotPrepared` there. A route that submits the prefix ahead of the frame lends
    /// `context.terrain` inside the prefix, before this feature prepares, with the chunks the
    /// rest of the frame lends. It prepares this feature before the frame publishes
    /// `context.shadows`, `context.occlusion` and `context.environment`, so there each refuses
    /// with `NotPrepared`, and the shadow casters of a fitted frame draw after it. The opaque
    /// snapshots (`context.snapshots`) are written after Opaque, so no pass of this feature can
    /// declare a read of them.
    ///
    /// The feature plans nothing in a frame without a depth prepass (no SSAO, reflections or
    /// depth fitting), where [`GameTargets::plan_pass`] refuses the place with
    /// `FrameError::MissingPass`; check the pass list for `PassSlot::DepthNormalPrepass` first.
    /// Whenever it plans no pass, the frame prepares, commits and finishes it as an ordinary
    /// feature without passes (a prefix encoded separately leaves it to the rest of the frame),
    /// so its `prepare` must accept a frame that lists none of its entries.
    ///
    /// # Errors
    /// Whatever refuses the feature; the renderer is not built.
    fn build_depth_prefix(
        &self,
        _context: &GameBuild<'_>,
    ) -> Result<Option<Box<dyn FrameFeature>>, FrameError> {
        Ok(None)
    }
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

    /// The opaque snapshots any factory of the inventory declared it reads.
    pub(crate) fn snapshot_reads(&self) -> SnapshotReads {
        self.factories
            .iter()
            .fold(SnapshotReads::NONE, |reads, factory| {
                reads.union(factory.snapshot_reads())
            })
    }

    /// The stock surfaces any factory of the inventory draws itself.
    pub(crate) fn stock_replacement(&self) -> StockReplacement {
        self.factories
            .iter()
            .fold(StockReplacement::NONE, |replaced, factory| {
                replaced.union(factory.replaces_stock())
            })
    }

    /// Whether a game pass factory draws the stock terrain surface itself.
    pub fn replaces_stock_terrain(&self) -> bool {
        self.stock_replacement().terrain_surface
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
    pub(crate) fn validate(
        &self,
        frame: [u32; 2],
        device: (&wgpu::Limits, wgpu::Features),
    ) -> Result<(), PostError> {
        let declarations = self.declarations();
        for (index, declaration) in declarations.iter().enumerate() {
            declaration.validate(device, frame)?;
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
        renderer.game.replaces = self.stock_replacement();
        let frame = renderer.targets().config();
        let store = Arc::new(TargetStore::build(
            context.device(),
            previous,
            &self.declarations(),
            frame.size,
        ));
        let engine = EngineConstants::new(context.device(), context.submission());
        renderer.game.engine = Some(engine.clone());
        let shared = SharedTextures::default();
        for factory in &self.factories {
            let resources = GameResources::new(
                context.device(),
                context.submission(),
                factory.resource_budget(),
                shared.clone(),
            );
            renderer.game.resources.push(resources.clone());
            let build = |targets: GameTargets| GameBuild {
                context,
                resources: resources.clone(),
                frame,
                targets: targets.with_snapshot_reads(factory.snapshot_reads()),
                camera: renderer.game_camera.clone(),
                terrain: renderer.game_terrain.clone(),
                shadows: renderer.game.shadows.clone(),
                occlusion: renderer.game.occlusion.clone(),
                environment: renderer.game.environment.clone(),
                scene_textures: renderer.game_textures.clone(),
                scene_meshes: renderer.game.meshes.clone(),
                engine: engine.charged_to(&resources),
                snapshots: renderer.game.snapshots.clone(),
            };
            let prefix =
                factory.build_depth_prefix(&build(GameTargets::for_depth_prefix(store.clone())))?;
            let feature = factory.build(&build(GameTargets::new(store.clone())))?;
            if let Some(prefix) = prefix {
                renderer.chain.register_prefix(context.device(), prefix)?;
                renderer.game.features += 1;
            }
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
        renderer.game.snapshot_reads = self.snapshot_reads();
        Ok(())
    }
}

/// What the game inventory installed on one renderer generation.
#[derive(Default)]
pub(crate) struct Installed {
    /// The stock surfaces the inventory's factories draw themselves, set when the inventory is
    /// installed. The stock water registers after that, so a game that draws the water leaves
    /// the chain with no stock water feature.
    pub(crate) replaces: StockReplacement,
    /// How many features the factories registered on the chain.
    pub(crate) features: usize,
    /// The textures behind the declared targets: made from the previous generation's, and handed
    /// on to the next one.
    pub(crate) store: Option<Arc<TargetStore>>,
    /// Where the shadowed route lends its cascades to the features of this generation.
    pub(crate) shadows: FrameShadowHandle,
    /// Where every route lends its ambient occlusion to the features of this generation.
    pub(crate) occlusion: FrameOcclusionHandle,
    /// Where every route lends its specular environment to the features of this generation.
    pub(crate) environment: FrameEnvironmentHandle,
    /// Where every route lends the scene's resident meshes to the features of this generation.
    pub(crate) meshes: FrameSceneMeshesHandle,
    /// The engine constants of this generation, which every route lends the scene's textures
    /// and terrain to.
    pub(crate) engine: Option<EngineConstants>,
    /// Each factory's declared resources, released with this generation.
    pub(crate) resources: Vec<GameResources>,
    /// Where the renderer lends the frame's opaque snapshots to the features of this generation.
    pub(crate) snapshots: FrameSnapshotHandle,
    /// The snapshots the inventory's factories declared they read, which this generation
    /// allocates and a capture admits whether or not a frame reads them.
    pub(crate) snapshot_reads: SnapshotReads,
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
        let mut frame = context.publications();
        for effect in self.store.frame_effects(context) {
            effect.stage(&mut frame);
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

    /// The writes the game's passes after the depth prepass add to a capture's prefix: each
    /// target's texture, imported as [`Self::game_capture_imports`] imports it for the suffix,
    /// since those passes may attach any of them. A renderer whose game has no depth-prefix
    /// feature adds none. The prefix publishes none of the game's bookkeeping; the suffix, which
    /// lists the prefix's entries as completed, does.
    ///
    /// # Errors
    /// [`FrameError`] when the renderer's registry refuses a texture.
    pub(crate) fn capture_prefix_game_keys(
        &mut self,
    ) -> Result<Vec<worldrunner_render_gpu::submission::capture::WriteKey>, FrameError> {
        if !self.chain.has_prefix_features() {
            return Ok(Vec::new());
        }
        let game = self.game_capture_imports();
        let imports: Vec<_> = game
            .iter()
            .map(|(texture, initial)| (texture, *initial))
            .collect();
        self.chain
            .capture_write_keys_initial(&imports)
            .map_err(FrameError::from)
    }
}
