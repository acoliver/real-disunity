//! The streamed terrain a game's passes draw: the chunks the frame's stock selection admitted.
use super::lent::{FrameInputError, Lent};
use crate::draw::{SceneGpu, TerrainChunkDraw, TerrainChunkViewError};
use std::rc::Rc;
use worldrunner_render_post::FrameError;

#[cfg(test)]
pub(crate) mod probe;

/// Why the frame in flight has no terrain chunks to lend.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum FrameTerrainError {
    #[error("terrain chunks are lent only inside a frame")]
    OutsideFrame,
    #[error("the route of the frame in flight prepared no terrain")]
    NotPrepared,
    #[error("the scene's terrain is not streamed, so it has no chunks to lend")]
    NotStreamed,
    #[error(transparent)]
    View(#[from] TerrainChunkViewError),
}

impl From<FrameInputError> for FrameTerrainError {
    fn from(error: FrameInputError) -> Self {
        match error {
            FrameInputError::OutsideFrame => Self::OutsideFrame,
            FrameInputError::NotPrepared => Self::NotPrepared,
        }
    }
}

impl From<FrameTerrainError> for FrameError {
    fn from(error: FrameTerrainError) -> Self {
        Self::FeaturePreparation {
            feature: "frame terrain",
            reason: error.to_string(),
        }
    }
}

/// Where a game reads the streamed terrain of the frame in flight.
///
/// Each stock route publishes it after preparing the frame's terrain and before any feature
/// prepares, with the frame's finalized camera and raster extent. It holds the chunks the stock
/// camera draw would submit, in its order, with their neighbours and resident buffers, whether
/// or not the stock passes draw the terrain. A route that cannot lend records why, and only a
/// feature that reads the terrain sees it; the frame itself goes on. A depth prefix that a
/// route submits ahead of the frame publishes it too, before the prefix's features prepare,
/// with the chunks the rest of the frame lends: a depth-prefix feature reads the lent terrain only; shadow, occlusion and environment inputs are `NotPrepared` there; see
/// [`GameFeatureFactory::build_depth_prefix`](super::GameFeatureFactory::build_depth_prefix).
///
/// The lent buffers are the renderer's own. Record draws from them in the frame that lent them;
/// do not keep them for later frames, write them or destroy them.
#[derive(Clone, Debug, Default)]
pub struct FrameTerrainHandle(
    /// The frame's chunks, `None` without terrain, or why it cannot lend them.
    Lent<Result<Option<Rc<[TerrainChunkDraw]>>, FrameTerrainError>>,
);

impl FrameTerrainHandle {
    /// The chunks of the frame now being prepared; `None` when the scene has no terrain.
    ///
    /// # Errors
    /// [`FrameTerrainError`] outside a frame, in a frame whose route prepared no terrain, for a
    /// terrain that is not streamed, or when the frame's selection cannot be read. A previous
    /// frame's chunks are never returned.
    pub fn current(&self) -> Result<Option<Rc<[TerrainChunkDraw]>>, FrameTerrainError> {
        self.0.current()?
    }

    /// Records what `gpu` prepared for this frame's view. Outside a frame nothing is lent.
    pub(crate) fn publish(
        &self,
        gpu: &SceneGpu,
        frame: &crate::frame::FrameUniform,
        viewport: [u32; 2],
    ) {
        self.0.publish_with(|| {
            let lent = lend(gpu, frame, viewport);
            #[cfg(test)]
            probe::record_publication(&lent);
            lent
        });
    }
}

fn lend(
    gpu: &SceneGpu,
    frame: &crate::frame::FrameUniform,
    viewport: [u32; 2],
) -> Result<Option<Rc<[TerrainChunkDraw]>>, FrameTerrainError> {
    match gpu.prepared_terrain_chunks(frame, viewport)? {
        Some(chunks) => Ok(Some(Rc::from(chunks))),
        None if gpu.has_terrain() => Err(FrameTerrainError::NotStreamed),
        None => Ok(None),
    }
}

impl crate::compositor::FrameRenderer {
    /// Lends the frame's prepared terrain, and the scene's resident textures, to the game's
    /// features, when there are any. Call it after the route prepares the frame's terrain, with
    /// the same frame and raster extent.
    pub(crate) fn publish_game_terrain(
        &self,
        gpu: &SceneGpu,
        frame: &crate::frame::FrameUniform,
        viewport: [u32; 2],
    ) {
        if self.game.features > 0 {
            self.game_terrain.publish(gpu, frame, viewport);
        }
        self.publish_game_scene(gpu);
    }
}
