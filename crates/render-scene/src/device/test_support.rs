use super::{DeviceError, GpuContext, REQUIRED_FEATURES, requirements};
use crate::test_support::SceneTestSession;
use worldrunner_render_gpu::{submission::SubmissionAuthority, test_support::TestGpu};

impl GpuContext {
    /// [`GpuContext::request`] on render-gpu's test instance, for a test that needs a device of
    /// its own. Production [`headless_instance`](worldrunner_render_gpu::device::headless_instance)
    /// keeps `InstanceFlags::DEBUG` in debug builds, so its Vulkan devices load
    /// `VK_EXT_debug_utils`, and parallel test devices crashed inside the loader naming objects
    /// through it (hub issue 12ae16c68a94). The test instance loads no debug utils.
    ///
    /// # Errors
    /// As [`GpuContext::request`].
    pub async fn request_for_test() -> Result<Self, DeviceError> {
        Self::request_with_features_for_test(REQUIRED_FEATURES).await
    }

    /// [`GpuContext::request_with_features`] on render-gpu's test instance; see
    /// [`GpuContext::request_for_test`].
    ///
    /// # Errors
    /// As [`GpuContext::request_with_features`].
    pub async fn request_with_features_for_test(
        required_features: wgpu::Features,
    ) -> Result<Self, DeviceError> {
        let instance = worldrunner_render_gpu::test_support::test_instance();
        Self::from_instance_with_features(&instance, required_features).await
    }

    /// Negotiates the production device request against this context's adapter.
    pub fn requested_features_for_test(&self, required: wgpu::Features) -> wgpu::Features {
        worldrunner_render_gpu::device::negotiate(
            self.adapter.features(),
            &self.adapter.limits(),
            &requirements(
                &self.adapter,
                required,
                worldrunner_render_post::TimingMode::Disabled,
            ),
        )
        .expect("test adapter satisfies the scene device request")
        .enabled_features
    }

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
            unavailable_optional_limits: Vec::new(),
        })
    }
}
