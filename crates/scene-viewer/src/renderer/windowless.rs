use super::{Extents, Renderer, ViewerError, device_errors, readiness, required_features, target};

impl Renderer {
    /// A renderer with no window, on `shared`'s device when one is given, otherwise on a device
    /// of its own.
    pub(crate) async fn windowless(
        host: &mut crate::RetainedHost,
        shared: Option<super::Shared>,
    ) -> Result<Self, ViewerError> {
        Self::windowless_with_format(host, shared, crate::presentation::COLOR).await
    }

    /// A windowless renderer whose output stands in for a surface of `format`: the frame chain
    /// is built for `format` and every frame it draws targets a texture of that format.
    pub(crate) async fn windowless_with_format(
        host: &mut crate::RetainedHost,
        shared: Option<super::Shared>,
        format: wgpu::TextureFormat,
    ) -> Result<Self, ViewerError> {
        let (instance, context) = windowless_context(host, shared).await?;
        let scene = host.initial_scene();
        let options = &host.options;
        let shadow_policy = host.readiness.shadow_policy;
        let errors = device_errors(context.device(), context.submission());
        let extents = Extents::new(
            options.size,
            context.device().limits().max_texture_dimension_2d,
        )?
        .expect("validated nonzero extent");
        let mut presentation = super::assembly::configured_presentation(
            &context,
            format,
            [extents.render, extents.surface],
            options.frame_profile,
            scene.effects(),
            &crate::presentation::PresentationPolicy {
                receiver_gradients: crate::presentation::receiver_reservation(options.shadows),
                budget: options.presentation_budget,
                capture_pool: crate::CapturePoolPolicy::Fixed(host.capture_pool_bytes()),
                recipes: true,
                game: options.game.clone(),
            },
        )?;
        let mut streaming = readiness::Streaming::with_shadow_policy(
            options.shadows,
            options.frame_profile,
            shadow_policy,
        );
        streaming.tiers.policy = options.texture_tiers;
        #[cfg(test)]
        {
            streaming.tiers.memory_override = host.texture_tier_memory_override;
        }
        let resident = super::environment::new_resident(
            &context,
            scene,
            options.shadows,
            options.frame_profile,
            &mut presentation,
            &mut streaming.tiers,
        )?;
        let execution = target::Execution::offscreen(
            &context,
            options.size,
            options.frames_in_flight,
            presentation.output_format(),
        )?;
        let renderer = Self::assemble(super::assembly::RendererParts {
            execution,
            #[cfg(all(test, target_os = "linux"))]
            instance,
            window: None,
            context,
            presentation,
            resident,
            viewport: Some(extents.surface),
            errors,
            streaming,
        })?;
        #[cfg(not(all(test, target_os = "linux")))]
        drop(instance);
        Ok(renderer)
    }
    pub(crate) fn set_clock(&mut self, clock: crate::RenderClock) {
        self.clock.select(clock);
    }
}

pub(super) async fn windowless_context(
    host: &mut crate::RetainedHost,
    shared: Option<super::Shared>,
) -> Result<(wgpu::Instance, super::GpuContext), ViewerError> {
    if let Some(shared) = shared {
        return adopt(host, shared);
    }
    let instance = windowless_instance();
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions::default())
        .await?;
    let size = host.options.size;
    let resolved = super::startup::resolve(host, &adapter, size, None, None)?;
    let context = super::timing::context(
        adapter,
        required_features(&resolved.scene),
        resolved.startup.gpu_timing,
    )
    .await?;
    Ok((instance, context))
}

#[cfg(not(test))]
fn windowless_instance() -> wgpu::Instance {
    wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env())
}

/// The scene-viewer tests' instance is render-gpu's test instance: parallel test devices on the
/// production instance crashed inside the Vulkan loader (hub issue 12ae16c68a94).
#[cfg(test)]
fn windowless_instance() -> wgpu::Instance {
    worldrunner_render_gpu::test_support::test_instance()
}

/// The device a caller made before the session. Its adapter was chosen without the scene, so a
/// feature the scene needs and the device lacks is refused by name, as a window's start does.
fn adopt(
    host: &mut crate::RetainedHost,
    shared: super::Shared,
) -> Result<(wgpu::Instance, super::GpuContext), ViewerError> {
    let super::Shared {
        instance, context, ..
    } = shared;
    let adapter = context.adapter();
    let size = host.options.size;
    let resolved = super::startup::resolve(host, adapter, size, None, None)?;
    let missing = required_features(&resolved.scene).difference(context.device().features());
    if !missing.is_empty() {
        return Err(
            worldrunner_render_scene::device::DeviceError::MissingFeatures {
                adapter: adapter.get_info().name,
                missing,
            }
            .into(),
        );
    }
    Ok((instance, context))
}
