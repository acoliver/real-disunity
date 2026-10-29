use super::{
    AdoptedFit, AdoptionState, DepthRange, DepthReducer, PendingReduction, SdsmError, SdsmSettings,
};
use crate::frame::metadata::FrameProduct;
use worldrunner_render_gpu::submission::capture::WriteKey;
use worldrunner_render_post::AttachmentView;

/// Resource ownership and adoption are shared by interactive and capture callers.
/// Rebinding a resized/replaced metadata product invalidates every outstanding ticket.
#[derive(Debug)]
pub struct DepthFitting {
    reducer: DepthReducer,
    state: AdoptionState,
    source: (wgpu::Texture, wgpu::Texture),
}
impl DepthFitting {
    pub fn new(
        device: &wgpu::Device,
        settings: SdsmSettings,
        source: (&AttachmentView, &AttachmentView),
    ) -> Result<Self, SdsmError> {
        let reducer = make_reducer(device, source)?;
        Ok(Self {
            reducer,
            state: AdoptionState::new(settings)?,
            source: (source.0.texture().clone(), source.1.texture().clone()),
        })
    }
    pub fn rebind(
        &mut self,
        device: &wgpu::Device,
        source: (&AttachmentView, &AttachmentView),
    ) -> Result<bool, SdsmError> {
        if self.source.0 == *source.0.texture() && self.source.1 == *source.1.texture() {
            return Ok(false);
        }
        let reducer = make_reducer(device, source)?;
        self.state.invalidate();
        self.reducer = reducer;
        self.source = (source.0.texture().clone(), source.1.texture().clone());
        Ok(true)
    }
    pub fn encode(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        source_frame: u64,
        key: WriteKey,
    ) -> Result<PendingReduction, SdsmError> {
        self.reducer
            .encode(device, encoder, self.state.ticket(source_frame), key)
    }
    pub fn complete(
        &mut self,
        ticket: &super::ReductionTicket,
        reading: Result<DepthRange, SdsmError>,
    ) -> Result<(), SdsmError> {
        match reading {
            Ok(depth) => self.state.complete(ticket, depth),
            Err(error) => {
                self.state.invalidate();
                Err(error)
            }
        }
    }
    pub fn adopt(
        &mut self,
        frame: u64,
        clip_meters: [f32; 2],
    ) -> Result<Option<AdoptedFit>, SdsmError> {
        self.state.adopt(frame, clip_meters)
    }
    pub fn adopt_at(
        &mut self,
        frame: u64,
        clip: [f32; 2],
        elapsed: std::time::Duration,
    ) -> Result<Option<AdoptedFit>, SdsmError> {
        self.state.adopt_at(frame, clip, elapsed)
    }
    pub fn invalidate_pending(&mut self) {
        self.state.invalidate_pending();
    }
    pub fn invalidate(&mut self) {
        self.state.invalidate();
    }
    pub(super) fn set_timing(&mut self, timing: worldrunner_render_post::TimingState) {
        self.reducer.set_timing(timing);
    }
    pub fn reducer(&self) -> &DepthReducer {
        &self.reducer
    }
    pub fn ticket(&self, source_frame: u64) -> super::ReductionTicket {
        self.state.ticket(source_frame)
    }
}
fn make_reducer(
    device: &wgpu::Device,
    source: (&AttachmentView, &AttachmentView),
) -> Result<DepthReducer, SdsmError> {
    DepthReducer::new(
        device,
        (FrameProduct::linear_depth(0), source.0),
        (FrameProduct::normals(), source.1),
    )
}
