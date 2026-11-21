//! Prepared dispatches and transfers as the commands a frame feature stages on its frame's
//! preparation, which the frame records in staging order after every feature has prepared. A
//! prepared dispatch is compute work alone, so the frame records it into one compute pass with the
//! compute work staged next to it; a transfer's copies end that pass. Their refusals reach the
//! frame as the runtime's own `ProgramError`, inside `FrameError::PreparedRefused`.

use worldrunner_render_post::{PreparedCommands, PreparedCompute, PreparedRefusal, PreparedWrite};

use crate::block_transfer::PreparedBlockTransfer;
use crate::dispatch::PreparedDispatch;

impl PreparedCommands for PreparedDispatch {
    fn writes(&self) -> Vec<PreparedWrite> {
        Self::writes(self).to_vec()
    }

    fn retain(&self, encoder: &wgpu::CommandEncoder) -> Result<(), PreparedRefusal> {
        Self::retain(self, encoder).map_err(PreparedRefusal::new)
    }

    fn record(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        label: &str,
    ) -> Result<(), PreparedRefusal> {
        self.record_labelled(encoder, Some(label))
            .map_err(PreparedRefusal::new)
    }

    fn compute(&self) -> Option<&dyn PreparedCompute> {
        Some(self)
    }
}

impl PreparedCompute for PreparedDispatch {
    fn record_in_pass(
        &self,
        pass: &mut wgpu::ComputePass<'_>,
        label: &str,
    ) -> Result<(), PreparedRefusal> {
        Self::record_in_pass(self, pass, label).map_err(PreparedRefusal::new)
    }
}

impl PreparedCommands for PreparedBlockTransfer {
    fn writes(&self) -> Vec<PreparedWrite> {
        vec![Self::writes(self)]
    }

    fn retain(&self, encoder: &wgpu::CommandEncoder) -> Result<(), PreparedRefusal> {
        Self::retain(self, encoder).map_err(PreparedRefusal::new)
    }

    fn record(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        label: &str,
    ) -> Result<(), PreparedRefusal> {
        encoder.push_debug_group(label);
        Self::record(self, encoder);
        encoder.pop_debug_group();
        Ok(())
    }
}
