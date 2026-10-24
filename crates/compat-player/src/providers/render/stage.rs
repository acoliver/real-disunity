use worldrunner::scene::{gpu::wgpu, post};

pub struct Factory {
    copy: post::CopyOutput,
    #[cfg(test)]
    control: super::super::RenderControl,
}
impl Factory {
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        control: super::super::RenderControl,
    ) -> Self {
        #[cfg(not(test))]
        let _ = control;
        Self {
            copy: post::CopyOutput::new(device, format),
            #[cfg(test)]
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
        #[cfg(test)]
        if matches!(self.control, super::super::RenderControl::WrongStage) {
            return Err(post::PostError::OutputStage);
        }
        self.copy.build(device, targets, persistent)
    }
}
