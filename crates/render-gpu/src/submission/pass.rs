//! A native render pass bound to the transaction owning its actual encoder.
use super::{Publications, TransactionError};

/// Opens only on the active transaction's encoder and owns its pass until consumed.
/// Dropping this boundary closes the pass without publishing the transaction.
/// A native pass cannot replace the boundary:
/// ```compile_fail
/// use worldrunner_render_gpu::submission::RecordingRenderPass;
/// fn substitute(pass: wgpu::RenderPass<'_>) {
///     let _: RecordingRenderPass<'_> = pass;
/// }
/// ```
/// The encoder remains borrowed while the boundary owns its native pass:
/// ```compile_fail
/// use worldrunner_render_gpu::submission::RecordingRenderPass;
/// fn overlap(encoder: &mut wgpu::CommandEncoder, descriptor: &wgpu::RenderPassDescriptor<'_>) {
///     let pass = RecordingRenderPass::begin(encoder, descriptor).unwrap();
///     encoder.insert_debug_marker("cannot record during a pass");
///     drop(pass);
/// }
/// ```
/// Extracted publications cannot authorize a foreign native pass as a new boundary:
/// ```compile_fail
/// use worldrunner_render_gpu::submission::RecordingRenderPass;
/// fn replace(recording: RecordingRenderPass<'_>, foreign: wgpu::RenderPass<'_>) {
///     let (_, publications) = recording.into_recording_parts();
///     let pass = foreign.forget_lifetime();
///     let _ = RecordingRenderPass { pass, publications };
/// }
/// ```
/// Extraction consumes the boundary:
/// ```compile_fail
/// use worldrunner_render_gpu::submission::RecordingRenderPass;
/// fn reuse(recording: RecordingRenderPass<'_>) {
///     let _ = recording.into_recording_parts();
///     let _ = recording.into_recording_parts();
/// }
/// ```
#[must_use = "record the pass or drop it to close it"]
pub struct RecordingRenderPass<'encoder> {
    pass: wgpu::RenderPass<'encoder>,
    publications: Publications,
}

impl<'encoder> RecordingRenderPass<'encoder> {
    /// Validates ownership before opening the native pass.
    pub fn begin(
        encoder: &'encoder mut wgpu::CommandEncoder,
        descriptor: &wgpu::RenderPassDescriptor<'_>,
    ) -> Result<Self, TransactionError> {
        let publications = Publications::for_encoder(encoder)?;
        Ok(Self {
            pass: encoder.begin_render_pass(descriptor),
            publications,
        })
    }

    /// Sets a caller-owned binding before the consuming scene recorder runs.
    pub fn set_bind_group(&mut self, index: u32, group: &wgpu::BindGroup, offsets: &[u32]) {
        self.pass.set_bind_group(index, group, offsets);
    }

    /// Transfers the native pass and its validated publication scope to the caller.
    /// Subsequent recording and publication use belongs to the caller.
    /// ```
    /// use worldrunner_render_gpu::submission::RecordingRenderPass;
    /// fn close(recording: RecordingRenderPass<'_>) {
    ///     let (pass, publications) = recording.into_recording_parts();
    ///     drop(pass);
    ///     drop(publications);
    /// }
    /// ```
    pub fn into_recording_parts(self) -> (wgpu::RenderPass<'encoder>, Publications) {
        (self.pass, self.publications)
    }
}
