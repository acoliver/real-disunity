use super::{
    GpuContext, ViewerError,
    execution::{ReadyFrame, SurfaceExecution},
};
use worldrunner_render_gpu::flight::{AdmissionError, CompletionTicket, FrameAdmission};
use worldrunner_render_gpu::submission::PreparedFrame;

pub(super) enum Execution {
    Surface(Box<SurfaceExecution>),
    Offscreen {
        texture: wgpu::Texture,
        flight: FrameAdmission,
    },
}
impl Execution {
    pub fn offscreen(
        context: &GpuContext,
        size: [u32; 2],
        capacity: usize,
        format: wgpu::TextureFormat,
    ) -> Result<Self, ViewerError> {
        Ok(Self::Offscreen {
            texture: texture(context.device(), size, format),
            flight: FrameAdmission::new(capacity).map_err(|e| ViewerError::Gpu(e.to_string()))?,
        })
    }
    pub fn resize(&mut self, device: &wgpu::Device, size: [u32; 2]) -> Result<(), ViewerError> {
        match self {
            Self::Surface(surface) => surface.resize(device, size),
            Self::Offscreen {
                texture: target, ..
            } => {
                if !size.contains(&0) {
                    *target = texture(device, size, target.format());
                }
                Ok(())
            }
        }
    }
    pub fn acquire(&mut self, context: &GpuContext) -> Result<Option<ReadyFrame>, ViewerError> {
        match self {
            Self::Surface(surface) => surface.acquire(context),
            Self::Offscreen { texture, flight } => {
                let admission = match flight.try_acquire() {
                    Ok(admission) => admission,
                    Err(AdmissionError::WouldBlock) => return Ok(None),
                    Err(error) => return Err(ViewerError::Gpu(error.to_string())),
                };
                Ok(Some(ReadyFrame {
                    admission,
                    frame: super::execution::OutputFrame {
                        texture: texture.clone(),
                        surface: None,
                    },
                }))
            }
        }
    }
    pub fn submit_prepared(
        &self,
        ready: ReadyFrame,
        prepared: PreparedFrame,
        context: &GpuContext,
    ) -> Result<CompletionTicket, ViewerError> {
        match self {
            Self::Surface(surface) => surface.submit_prepared(ready, prepared, context),
            Self::Offscreen { flight, .. } => flight
                .submit_prepared(ready.admission, prepared)
                .map_err(|e| ViewerError::Gpu(e.to_string())),
        }
    }
    pub fn flight(&self) -> &FrameAdmission {
        match self {
            Self::Surface(surface) => surface.capture_flight(),
            Self::Offscreen { flight, .. } => flight,
        }
    }
    /// A capture's own output target, in `format`: the format the frame chain's output stages are
    /// built for.
    pub fn acquire_capture(
        &self,
        context: &GpuContext,
        size: [u32; 2],
        format: wgpu::TextureFormat,
    ) -> Result<Option<ReadyFrame>, ViewerError> {
        let admission = match self.flight().try_acquire() {
            Ok(admission) => admission,
            Err(AdmissionError::WouldBlock) => return Ok(None),
            Err(error) => return Err(ViewerError::Gpu(error.to_string())),
        };
        Ok(Some(ReadyFrame {
            admission,
            frame: super::execution::OutputFrame {
                texture: texture(context.device(), size, format),
                surface: None,
            },
        }))
    }
    pub fn submit_capture(
        &self,
        ready: ReadyFrame,
        prepared: PreparedFrame,
    ) -> Result<CompletionTicket, ViewerError> {
        self.flight()
            .submit_prepared(ready.admission, prepared)
            .map_err(|e| ViewerError::Gpu(e.to_string()))
    }
    pub fn take_recreated(&mut self) -> bool {
        match self {
            Self::Surface(surface) => surface.take_recreated(),
            Self::Offscreen { .. } => false,
        }
    }
}
fn texture(device: &wgpu::Device, size: [u32; 2], format: wgpu::TextureFormat) -> wgpu::Texture {
    worldrunner_render_post::color_texture(device, "retained output", size, format)
}

#[cfg(all(test, target_os = "linux"))]
impl Execution {
    pub fn surface(&self) -> &SurfaceExecution {
        match self {
            Self::Surface(surface) => surface,
            Self::Offscreen { .. } => panic!("expected surface execution"),
        }
    }
    pub fn surface_mut(&mut self) -> &mut SurfaceExecution {
        match self {
            Self::Surface(surface) => surface,
            Self::Offscreen { .. } => panic!("expected surface execution"),
        }
    }
}
