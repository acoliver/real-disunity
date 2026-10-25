//! The prefix and suffix are owned by the existing exclusive capture service.
use super::{AdoptedFit, DepthReducer, PendingReduction, RangeFit, ReductionTicket, SdsmError};
use worldrunner_render_gpu::submission::{
    Publications,
    capture::{
        CaptureContinuation, CaptureError, CaptureWrites, PreparedPrefix, PreparedSuffix, WriteKey,
    },
};

/// Frozen numerical inputs accompany the caller's owned scene/view snapshot.
#[derive(Debug)]
pub struct CaptureInput<F> {
    pub frame: F,
    pub source_frame: u64,
    pub clip_meters: [f32; 2],
    pub range: RangeFit,
}
#[derive(Debug)]
pub struct DepthPrefix<F> {
    prefix: PreparedPrefix<CaptureInput<F>>,
    pending: PendingReduction,
}
#[derive(Debug)]
pub struct DepthContinuation<F> {
    continuation: CaptureContinuation<CaptureInput<F>>,
    pending: PendingReduction,
}
impl DepthReducer {
    /// Encode geometry/linearization first, then this frame's reduction. Nothing
    /// is mapped or fitted before submission. Dropping the prefix cancels both.
    pub fn begin_capture<F: Send + Sync + 'static, E: From<SdsmError>>(
        &self,
        context: &crate::device::GpuContext,
        contract: (CaptureWrites, WriteKey),
        input: CaptureInput<F>,
        ticket: ReductionTicket,
        prepass: impl FnOnce(&mut wgpu::CommandEncoder, &mut Publications, &F) -> Result<(), E>,
    ) -> Result<DepthPrefix<F>, CaptureError<E>> {
        input
            .range
            .validate()
            .map_err(|error| CaptureError::Preparation(E::from(SdsmError::from(error))))?;
        if input.source_frame != ticket.source_frame {
            return Err(CaptureError::Preparation(E::from(SdsmError::FrameIdentity)));
        }
        let authority = context.submission();
        let device = context.device();
        let mut pending = None;
        let prefix =
            authority.begin_capture(device, contract.0, input, |encoder, edits, frozen| {
                prepass(encoder, edits, &frozen.frame)?;
                pending = Some(
                    self.encode(device, encoder, ticket, contract.1)
                        .map_err(E::from)?,
                );
                Ok(())
            })?;
        Ok(DepthPrefix {
            prefix,
            pending: pending.expect("successful prefix encoded reduction"),
        })
    }
}
impl<F> DepthPrefix<F> {
    pub fn observer(&self) -> worldrunner_render_gpu::submission::capture::CaptureObserver {
        self.prefix.observer()
    }
    pub fn submit(self) -> Result<DepthContinuation<F>, CaptureError<std::convert::Infallible>> {
        Ok(DepthContinuation {
            continuation: self.prefix.submit()?,
            pending: self.pending,
        })
    }
}
impl<F> DepthContinuation<F> {
    /// Capture deliberately bypasses interactive latency and hysteresis. The
    /// suffix is prepared only after successful mapping and validated metre bounds.
    pub fn resume<E: From<SdsmError>>(
        self,
        device: &wgpu::Device,
        suffix: impl FnOnce(
            &mut wgpu::CommandEncoder,
            &mut Publications,
            &F,
            AdoptedFit,
        ) -> Result<(), E>,
    ) -> Result<PreparedSuffix<CaptureInput<F>>, CaptureError<E>> {
        self.continuation.resume(
            device,
            |device, submission| self.pending.wait(device, submission).map_err(E::from),
            |encoder, edits, frozen, depth| {
                let depth = frozen
                    .range
                    .fit(*depth, frozen.clip_meters)
                    .map_err(SdsmError::from)
                    .map_err(E::from)?;
                suffix(
                    encoder,
                    edits,
                    &frozen.frame,
                    AdoptedFit {
                        source_frame: frozen.source_frame,
                        adopted_frame: frozen.source_frame,
                        depth,
                    },
                )
            },
        )
    }
}
