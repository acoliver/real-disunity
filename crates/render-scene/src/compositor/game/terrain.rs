//! The streamed terrain a game's passes draw: the chunks the frame's stock selection admitted.
use crate::draw::{SceneGpu, TerrainChunkDraw};
use std::cell::RefCell;
use std::rc::Rc;
use worldrunner_render_gpu::submission::Publications;
use worldrunner_render_post::FrameError;

/// The chunks of one frame and the transaction that prepared them.
#[derive(Debug)]
struct FrameTerrain {
    owner: Publications,
    chunks: Option<Rc<[TerrainChunkDraw]>>,
}

/// Where a game reads the streamed terrain of the frame in flight.
///
/// Each stock route publishes it after preparing the frame's terrain and before any feature
/// prepares, with the frame's finalized camera and raster extent. It holds the chunks the stock
/// camera draw would submit, in its order, with their neighbours and resident buffers, whether
/// or not the stock passes draw the terrain.
#[derive(Clone, Debug, Default)]
pub struct FrameTerrainHandle(Rc<RefCell<Option<FrameTerrain>>>);

impl FrameTerrainHandle {
    /// The chunks of the frame now being prepared; `None` when the scene does not stream its
    /// terrain.
    ///
    /// # Errors
    /// [`FrameError::FeaturePreparation`] outside a frame, or in a frame whose route prepared no
    /// terrain. A previous frame's chunks are never returned.
    pub fn current(&self) -> Result<Option<Rc<[TerrainChunkDraw]>>, FrameError> {
        let current = Publications::current();
        self.0
            .borrow()
            .as_ref()
            .filter(|frame| current.is_some_and(|owner| owner.same_frame(&frame.owner)))
            .map(|frame| frame.chunks.clone())
            .ok_or_else(|| FrameError::FeaturePreparation {
                feature: "frame terrain",
                reason: "no terrain was prepared for the frame in flight".to_owned(),
            })
    }

    /// Records the chunks `gpu` prepared for this frame's view.
    pub(crate) fn publish(
        &self,
        gpu: &SceneGpu,
        frame: &crate::frame::FrameUniform,
        viewport: [u32; 2],
    ) -> Result<(), FrameError> {
        let refuse = |reason: String| FrameError::FeaturePreparation {
            feature: "frame terrain",
            reason,
        };
        let owner = Publications::current()
            .ok_or_else(|| refuse("terrain is lent only inside a frame".to_owned()))?;
        let chunks = gpu
            .prepared_terrain_chunks(frame, viewport)
            .map_err(|error| refuse(error.to_string()))?
            .map(Rc::from);
        *self.0.borrow_mut() = Some(FrameTerrain { owner, chunks });
        Ok(())
    }
}

impl crate::compositor::FrameRenderer {
    /// Lends the frame's prepared terrain to the game's features, when there are any. Call it
    /// after the route prepares the frame's terrain, with the same frame and raster extent.
    pub(crate) fn publish_game_terrain(
        &self,
        gpu: &SceneGpu,
        frame: &crate::frame::FrameUniform,
        viewport: [u32; 2],
    ) -> Result<(), FrameError> {
        if self.game_features == 0 {
            return Ok(());
        }
        self.game_terrain.publish(gpu, frame, viewport)
    }
}
