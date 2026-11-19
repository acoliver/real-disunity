//! Prepared dispatches and transfers as the commands a frame feature stages on its frame's
//! preparation, which the frame records in staging order after every feature has prepared.

use worldrunner_render_post::{FrameError, PreparedCommands};

use crate::block_transfer::PreparedBlockTransfer;
use crate::dispatch::PreparedDispatch;
use crate::error::ProgramError;

fn refused(error: &ProgramError) -> FrameError {
    FrameError::FeaturePreparation {
        feature: "render-program",
        reason: error.to_string(),
    }
}

impl PreparedCommands for PreparedDispatch {
    fn retain(&self, encoder: &wgpu::CommandEncoder) -> Result<(), FrameError> {
        Self::retain(self, encoder).map_err(|error| refused(&error))
    }

    fn record(&self, encoder: &mut wgpu::CommandEncoder) {
        Self::record(self, encoder);
    }
}

impl PreparedCommands for PreparedBlockTransfer {
    fn retain(&self, encoder: &wgpu::CommandEncoder) -> Result<(), FrameError> {
        Self::retain(self, encoder).map_err(|error| refused(&error))
    }

    fn record(&self, encoder: &mut wgpu::CommandEncoder) {
        Self::record(self, encoder);
    }
}
