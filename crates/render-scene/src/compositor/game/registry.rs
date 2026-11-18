//! The inventory of a game's feature factories, and their registration on each renderer
//! generation.
use super::camera::FrameCameraHandle;
use super::targets::{GameTargets, TargetDeclaration, TargetStore};
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
/// case frames are exactly the stock frames. Cloning shares the factories and the targets, so a
/// configuration derived from another keeps the persistent targets of the first.
#[derive(Clone, Default)]
pub struct GamePasses {
    factories: Vec<Arc<dyn GameFeatureFactory>>,
    store: Arc<TargetStore>,
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

    /// The targets of every factory, as of now. The inventory resolves them when a renderer is
    /// built; this handle reads the textures that resolution made.
    pub fn targets(&self) -> GameTargets {
        GameTargets::new(self.store.clone())
    }

    fn declarations(&self) -> Vec<TargetDeclaration> {
        self.factories
            .iter()
            .flat_map(|factory| factory.targets())
            .collect()
    }

    /// The storage the declared targets add to a renderer's allocation inventory.
    pub(crate) fn allocations(&self, frame: [u32; 2]) -> AllocationSet {
        let mut set = AllocationSet::default();
        for declaration in self.declarations() {
            declaration.allocate(&mut set, frame);
        }
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

    /// Resolves the declared targets and registers every factory's feature on `renderer`. This
    /// runs for each renderer generation, so a replacement registers again.
    pub(crate) fn install(
        &self,
        renderer: &mut FrameRenderer,
        context: &GpuContext,
    ) -> Result<(), PostError> {
        if self.factories.is_empty() {
            return Ok(());
        }
        let frame = renderer.targets().config();
        self.store
            .resolve(context.device(), &self.declarations(), frame.size);
        for factory in &self.factories {
            let feature = factory.build(&GameBuild {
                context,
                frame,
                targets: self.targets(),
                camera: renderer.game_camera.clone(),
            })?;
            renderer.chain.register(
                context.device(),
                Box::new(Registered {
                    feature,
                    store: self.store.clone(),
                }),
            )?;
            renderer.game_features += 1;
        }
        Ok(())
    }
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
        // Written layers are recorded when the frame is submitted, never before: a frame that is
        // cancelled leaves persistent targets as they were, contents and bookkeeping alike.
        let writes: Vec<_> = context
            .passes()
            .iter()
            .flat_map(|pass| {
                pass.color
                    .iter()
                    .chain(&pass.additional_colors)
                    .chain(&pass.depth)
            })
            .filter(|attachment| !attachment.read_only)
            .map(|attachment| attachment.resource)
            .collect();
        let store = self.store.clone();
        context.publish(move || {
            for resource in writes {
                store.mark_written(resource.name, resource.layer);
            }
        });
    }

    fn finish_draws(&mut self, context: &mut PrepareContext<'_>) {
        self.prepared.finish_draws(context);
    }

    fn draw(&self, context: &DrawContext, pass: &mut wgpu::RenderPass<'_>) {
        self.prepared.draw(context, pass);
    }
}
