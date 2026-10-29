use super::{
    AcquiredFrame, SurfaceError, SurfacePolicy, classify, configuration, configure, validate_extent,
};

fn validate_reconfiguration(
    capabilities: &wgpu::SurfaceCapabilities,
    config: &wgpu::SurfaceConfiguration,
) -> Result<(), SurfaceError> {
    configuration(
        capabilities,
        SurfacePolicy {
            format: config.format,
            usage: config.usage,
            size: [config.width, config.height],
            present_mode: config.present_mode,
            desired_maximum_frame_latency: config.desired_maximum_frame_latency,
            alpha_mode: config.alpha_mode,
        },
    )?;
    Ok(())
}

#[cfg(test)]
mod tests;

/// Owns the surface and its configuration/recovery lifecycle. Window integration only supplies
/// a replacement surface when `acquire` reports `Recreate`; no window-system dependency leaks in.
pub struct Presenter<'window> {
    surface: wgpu::Surface<'window>,
    adapter: wgpu::Adapter,
    config: wgpu::SurfaceConfiguration,
    state: PresenterState,
    generation: u64,
    transition: super::SurfaceTransition,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PresenterState {
    Suspended,
    Ready,
    Lost,
    Unconfigured,
}

impl<'window> Presenter<'window> {
    /// Retains the rendering device's adapter to recheck capabilities on every configuration.
    pub fn new(
        surface: wgpu::Surface<'window>,
        adapter: &wgpu::Adapter,
        device: &wgpu::Device,
        policy: SurfacePolicy,
    ) -> Result<Self, SurfaceError> {
        let config = configuration(&surface.get_capabilities(adapter), policy)?;
        let mut presenter = Self {
            surface,
            adapter: adapter.clone(),
            config,
            state: PresenterState::Unconfigured,
            generation: 0,
            transition: super::SurfaceTransition::new(policy.size),
        };
        presenter.reconfigure(device)?;
        Ok(presenter)
    }

    pub fn configuration(&self) -> &wgpu::SurfaceConfiguration {
        &self.config
    }

    /// Counts successful configurations, not requests or suspended zero-area resizes.
    pub fn configuration_generation(&self) -> u64 {
        self.generation
    }

    pub fn is_suspended(&self) -> bool {
        self.config.width == 0 || self.config.height == 0
    }

    /// An invalid extent leaves the previous configuration intact. Configuration failures
    /// instead make acquisition fail until a subsequent successful resize/replacement.
    /// Returns whether size-dependent targets may be allocated. Loss remains pending until
    /// `replace_surface`, even if a resize suspends and later resumes the presenter.
    pub fn resize(&mut self, device: &wgpu::Device, size: [u32; 2]) -> Result<bool, SurfaceError> {
        validate_extent(size, device.limits().max_texture_dimension_2d)?;
        self.config.width = size[0];
        self.config.height = size[1];
        self.transition.resize(size);
        Ok(!size.contains(&0))
    }

    /// Apply coalesced requests only after all acquired images and admissions retire.
    /// The host calls this on its render execution thread, never in an input callback.
    pub fn apply_pending(&mut self, device: &wgpu::Device) -> Result<AcquiredFrame, SurfaceError> {
        if self.is_suspended() {
            return Ok(AcquiredFrame::Skip);
        }
        if self.state == PresenterState::Lost {
            return Ok(AcquiredFrame::Recreate);
        }
        if self.transition.action(0) == super::SurfaceAction::Configure {
            self.reconfigure(device)?;
        }
        Ok(AcquiredFrame::Reconfigure)
    }

    pub fn has_pending_transition(&self) -> bool {
        !matches!(self.transition.action(0), super::SurfaceAction::None)
    }

    fn reconfigure(&mut self, device: &wgpu::Device) -> Result<bool, SurfaceError> {
        self.state = PresenterState::Unconfigured;
        validate_reconfiguration(&self.surface.get_capabilities(&self.adapter), &self.config)?;
        if !configure(&self.surface, device, &self.config)? {
            self.state = PresenterState::Suspended;
            return Ok(false);
        }
        self.state = PresenterState::Ready;
        self.transition.configured();
        self.generation += 1;
        Ok(true)
    }

    /// Rechecks capabilities on recovery; a replacement cannot silently change color policy.
    pub fn replace_surface(
        &mut self,
        surface: wgpu::Surface<'window>,
        capabilities: &wgpu::SurfaceCapabilities,
        device: &wgpu::Device,
    ) -> Result<bool, SurfaceError> {
        validate_reconfiguration(capabilities, &self.config)?;
        self.surface = surface;
        self.state = PresenterState::Unconfigured;
        self.reconfigure(device)
    }

    pub fn acquire(&mut self, device: &wgpu::Device) -> Result<AcquiredFrame, SurfaceError> {
        if self.is_suspended() {
            return Ok(AcquiredFrame::Skip);
        }
        match self.state {
            PresenterState::Suspended => Ok(AcquiredFrame::Skip),
            PresenterState::Lost => Ok(AcquiredFrame::Recreate),
            PresenterState::Unconfigured => Err(SurfaceError::Unconfigured),
            PresenterState::Ready if self.has_pending_transition() => {
                Ok(AcquiredFrame::Reconfigure)
            }
            PresenterState::Ready => {
                self.handle_acquired(device, self.surface.get_current_texture())
            }
        }
    }

    /// Applies the same recovery rules to externally acquired outcomes (including diagnostic
    /// injection). Usable suboptimal frames never trigger a reconfiguration loop.
    pub fn handle_acquired(
        &mut self,
        _device: &wgpu::Device,
        acquired: wgpu::CurrentSurfaceTexture,
    ) -> Result<AcquiredFrame, SurfaceError> {
        if self.is_suspended() {
            return Ok(AcquiredFrame::Skip);
        }
        if self.state == PresenterState::Lost {
            return Ok(AcquiredFrame::Recreate);
        }
        if self.state == PresenterState::Unconfigured {
            return Err(SurfaceError::Unconfigured);
        }
        match classify(acquired)? {
            AcquiredFrame::Reconfigure => {
                self.transition.outdated();
                Ok(AcquiredFrame::Reconfigure)
            }
            AcquiredFrame::Recreate => {
                self.state = PresenterState::Lost;
                self.transition.lost();
                Ok(AcquiredFrame::Recreate)
            }
            outcome => Ok(outcome),
        }
    }
}
