use worldrunner::scene::{gpu::wgpu, post};
pub(super) struct Factory {
    copy: post::CopyOutput,
    control: super::gpu::Control,
}
impl Factory {
    pub fn new(device: &wgpu::Device, control: super::gpu::Control) -> Self {
        Self {
            copy: post::CopyOutput::exact_srgb8(device),
            control,
        }
    }
}
impl post::StageFactory for Factory {
    fn build(
        &self,
        device: &wgpu::Device,
        targets: &post::FrameTargets,
        persistent: &post::Resources,
    ) -> Result<post::StageSet, post::PostError> {
        match self.control {
            #[cfg(test)]
            super::gpu::Control::WrongStage => Err(post::PostError::OutputStage),
            _ => self.copy.build(device, targets, persistent),
        }
    }
}
