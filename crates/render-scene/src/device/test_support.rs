use super::{DeviceError, GpuContext, REQUIRED_FEATURES, requirements};
use crate::test_support::SceneTestSession;
use worldrunner_render_gpu::{submission::SubmissionAuthority, test_support::TestGpu};

impl GpuContext {
    /// Builds a fresh scene authority on the shared BC-capable test device.
    /// A [`SceneTestSession`] must outlive the context and all its fixtures.
    pub fn shared_for_test() -> Result<Self, DeviceError> {
        Self::shared_for_test_with_features(REQUIRED_FEATURES)
    }

    /// Builds a test context with exactly the requested scene feature profile.
    /// Device callbacks remain fail-fast; loss/destruction tests must use fresh devices.
    pub fn shared_for_test_with_features(
        required_features: wgpu::Features,
    ) -> Result<Self, DeviceError> {
        SceneTestSession::assert_active();
        let adapter = TestGpu::shared_adapter().map_err(|_| DeviceError::NoAdapter)?;
        let missing = required_features.difference(adapter.features());
        if !missing.is_empty() {
            return Err(DeviceError::MissingFeatures {
                adapter: adapter.get_info().name,
                missing,
            });
        }
        let gpu = TestGpu::shared(&requirements(
            &adapter,
            required_features,
            worldrunner_render_post::TimingMode::Disabled,
        ))
        .map_err(|error| DeviceError::DeviceRefused(error.to_string()))?;
        SceneTestSession::retain(&gpu);
        Ok(Self {
            adapter: gpu.adapter,
            device: gpu.device,
            submission: SubmissionAuthority::new(gpu.queue),
        })
    }
}
