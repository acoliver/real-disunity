//! A game feature that records what the frame terrain handle lends, for the route tests.
use super::{FrameTerrainError, FrameTerrainHandle};
use crate::compositor::{FrameRenderer, GameBuild, GameFeatureFactory, GamePasses};
use crate::draw::TerrainChunkDraw;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use worldrunner_render_gpu::submission::Publications;
use worldrunner_render_post::{
    AttachmentView, BuildContext, DrawContext, FeatureDeclaration, FrameError, FrameFeature,
    PassDeclaration, PrepareContext, PreparedFeature,
};

/// One publication of the frame terrain handle: the transaction that published and what it lent.
#[derive(Debug)]
pub(crate) struct Published {
    pub(crate) owner: Publications,
    pub(crate) lent: Result<Option<Rc<[TerrainChunkDraw]>>, FrameTerrainError>,
}

thread_local! {
    static PUBLISHED: RefCell<Vec<Published>> = const { RefCell::new(Vec::new()) };
}

pub(super) fn record_publication(frame: &super::FrameTerrain) {
    PUBLISHED.with(|log| {
        log.borrow_mut().push(Published {
            owner: frame.owner.clone(),
            lent: frame.lent.clone(),
        });
    });
}

/// The publications this thread made since the last call, in order.
pub(crate) fn take_publications() -> Vec<Published> {
    PUBLISHED.with(|log| std::mem::take(&mut *log.borrow_mut()))
}

/// Answers shared between the probe and the test that installed it.
type Shared<T> = Arc<Mutex<Vec<T>>>;

/// What each prepared frame lent: the number of chunks, `None` without terrain, or the refusal.
#[derive(Clone, Default)]
pub(crate) struct Lent(Shared<Result<Option<usize>, FrameTerrainError>>);

impl Lent {
    /// The answers recorded so far, in frame order.
    pub(crate) fn answers(&self) -> Vec<Result<Option<usize>, FrameTerrainError>> {
        self.0.lock().unwrap().clone()
    }

    fn record(&self, answer: Result<Option<usize>, FrameTerrainError>) {
        self.0.lock().unwrap().push(answer);
    }
}

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
        self.lent.record(answer);
        Ok(())
    }

    fn draw(&self, _context: &DrawContext, _pass: &mut wgpu::RenderPass<'_>) {}
}
