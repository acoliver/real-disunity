//! The two ways to plan a frame: building it, which counts what it writes and names the record
//! that refused it, and checking it, which counts nothing.

use super::{
    OffscreenError, OffscreenFrame, OffscreenOperations, OffscreenState, ProgramArena,
    ProgramRuntime, Refusal,
};

impl OffscreenState {
    /// Resolves and packs one frame into the typed operations that run it, in call order.
    ///
    /// Nothing is recorded and nothing is submitted. Call it while no frame is being recorded,
    /// so a render texture that changed can release its old storage at once. The render textures
    /// the operations write count as written from here on, since the host runs a frame it builds.
    ///
    /// # Errors
    /// Fails by name on a record that names something the frame or the state lacks, a pass or
    /// variant the shader has none of, a vertex channel the mesh lacks, a copy that does not fit
    /// and a format this renderer does not draw into. Nothing of the frame is kept. The
    /// [`Refusal`] also says which record of the frame caused it when the state met the error
    /// while planning one.
    pub fn build(
        &mut self,
        runtime: &mut ProgramRuntime,
        arena: &ProgramArena,
        frame: &OffscreenFrame,
    ) -> Result<OffscreenOperations, Refusal> {
        let (operations, written) = self.assemble(runtime, arena, frame)?;
        self.textures.mark_written(written);
        Ok(operations)
    }

    /// Plans one frame as [`build`](Self::build) does and gives the operations up. The render
    /// textures the frame would write do not count as written, because nothing of this frame is
    /// run: a host builds the parts of a refused frame this way to find the one at fault, and the
    /// frame it then runs is the one that counts.
    ///
    /// # Errors
    /// The errors of [`build`](Self::build), without the record.
    pub fn check(
        &mut self,
        runtime: &mut ProgramRuntime,
        arena: &ProgramArena,
        frame: &OffscreenFrame,
    ) -> Result<(), OffscreenError> {
        self.assemble(runtime, arena, frame)
            .map(|_operations| ())
            .map_err(|refusal| refusal.error)
    }
}
