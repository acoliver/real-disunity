use super::{
    Extents, Presentation, Renderer, Resident, ViewerError, device_errors, readiness,
    required_features, target,
};

impl Renderer {
    pub(crate) async fn windowless(host: &crate::RetainedHost) -> Result<Self, ViewerError> {
        let options = &host.options;
        let scene = &host.initial_scene;
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await?;
        if let Some(callback) = &options.adapter_callback {
            callback.notify(&adapter)?;
        }
        let context =
            super::timing::context(adapter, required_features(scene), options.gpu_timing).await?;
        let errors = device_errors(context.device(), context.submission());
        let extents = Extents::new(
            options.size,
            context.device().limits().max_texture_dimension_2d,
        )?
        .expect("validated nonzero extent");
        let presentation = Presentation::with_settings(
            &context,
            crate::presentation::COLOR,
            [extents.render, extents.surface],
            options.frame_profile,
            scene.effects(),
            crate::presentation::PresentationPolicy {
                budget: options.presentation_budget,
                recipes: true,
            },
        )?;
        let resident = Resident::new(&context, scene, options.shadows, options.frame_profile)?;
        let execution =
            target::Execution::offscreen(&context, options.size, options.frames_in_flight)?;
        let renderer = Self {
            execution,
            #[cfg(all(test, target_os = "linux"))]
            instance,
            window: None,
            context,
            presentation,
            resident,
            viewport: Some(extents.surface),
            errors,
            clock: crate::clock::Clock::new(),
            frame_time: std::time::Duration::ZERO,
            live_upload: None,
            live_pending: None,
            streaming: readiness::Streaming::new(options.shadows, options.frame_profile),
        };
        renderer.check_errors()?;
        Ok(renderer)
    }
    pub(crate) fn set_clock(&mut self, clock: crate::RenderClock) {
        self.clock.select(clock);
    }
}
