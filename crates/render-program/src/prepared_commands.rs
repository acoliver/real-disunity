//! Prepared dispatches and transfers as the commands a frame feature stages on its frame's
//! preparation, which the frame records in staging order after every feature has prepared. Their
//! refusals reach the frame as the runtime's own `ProgramError`, inside
//! `FrameError::PreparedRefused`.

use worldrunner_render_post::{PreparedCommands, PreparedRefusal, PreparedWrite};

use crate::block_transfer::PreparedBlockTransfer;
use crate::dispatch::PreparedDispatch;

impl PreparedCommands for PreparedDispatch {
    fn writes(&self) -> Vec<PreparedWrite> {
        Self::writes(self).to_vec()
    }

    fn retain(&self, encoder: &wgpu::CommandEncoder) -> Result<(), PreparedRefusal> {
        Self::retain(self, encoder).map_err(PreparedRefusal::new)
    }

    fn record(&self, encoder: &mut wgpu::CommandEncoder) -> Result<(), PreparedRefusal> {
        Self::record(self, encoder).map_err(PreparedRefusal::new)
    }
}

impl PreparedCommands for PreparedBlockTransfer {
    fn writes(&self) -> Vec<PreparedWrite> {
        vec![Self::writes(self)]
    }

    fn retain(&self, encoder: &wgpu::CommandEncoder) -> Result<(), PreparedRefusal> {
        Self::retain(self, encoder).map_err(PreparedRefusal::new)
    }

    fn record(&self, encoder: &mut wgpu::CommandEncoder) -> Result<(), PreparedRefusal> {
        Self::record(self, encoder);
        Ok(())
    }
}
