//! A game feature that records what the frame terrain handle lends, for the route tests.
use super::{FrameTerrainError, FrameTerrainHandle};
use crate::compositor::{FrameRenderer, GameBuild, GameFeatureFactory, GamePasses};
use std::sync::{Arc, Mutex};
use worldrunner_render_post::{
    AttachmentView, BuildContext, DrawContext, FeatureDeclaration, FrameError, FrameFeature,
    PassDeclaration, PrepareContext, PreparedFeature,
};

/// What each prepared frame lent: the number of chunks, `None` without terrain, or the refusal.
pub(crate) type Lent = Arc<Mutex<Vec<Result<Option<usize>, FrameTerrainError>>>>;

/// Installs the probe on `renderer` and returns what it will record.
pub(crate) fn install(renderer: &mut FrameRenderer, context: &crate::device::GpuContext) -> Lent {
    let lent = Lent::default();
    GamePasses::new()
        .with(Arc::new(Probe { lent: lent.clone() }))
        .install(renderer, context, None)
        .unwrap();
    lent
}

struct Probe {
    lent: Lent,
}

impl GameFeatureFactory for Probe {
    fn name(&self) -> &'static str {
        "terrain probe"
    }

    fn build(&self, context: &GameBuild<'_>) -> Result<Box<dyn FrameFeature>, FrameError> {
        Ok(Box::new(ProbeFeature {
            terrain: context.terrain.clone(),
            lent: self.lent.clone(),
        }))
    }
}

#[derive(Clone)]
struct ProbeFeature {
    terrain: FrameTerrainHandle,
    lent: Lent,
}

impl FrameFeature for ProbeFeature {
    fn declare(&self) -> FeatureDeclaration {
        FeatureDeclaration {
            passes: Vec::new(),
            effects: Vec::new(),
        }
    }

    fn plan(
        &self,
        _passes: &mut Vec<PassDeclaration>,
        _views: &mut Vec<AttachmentView>,
    ) -> Result<FeatureDeclaration, FrameError> {
        Ok(self.declare())
    }

    fn build(&self, _context: &BuildContext<'_>) -> Result<Box<dyn PreparedFeature>, FrameError> {
        Ok(Box::new(self.clone()))
    }
}

impl PreparedFeature for ProbeFeature {
    fn prepare(&mut self, _context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        let answer = self
            .terrain
            .current()
            .map(|lent| lent.map(|chunks| chunks.len()));
        self.lent.lock().unwrap().push(answer);
        Ok(())
    }

    fn draw(&self, _context: &DrawContext, _pass: &mut wgpu::RenderPass<'_>) {}
}
