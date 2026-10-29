use super::{GpuContext, Renderer, ViewerError};
use crate::{GpuTiming, GpuTimingUnavailable, RetainedHost};

/// Stock geometry and output stages use pass-level timestamp writes.
pub(crate) fn features() -> wgpu::Features {
    wgpu::Features::TIMESTAMP_QUERY
}

#[cfg(test)]
pub(crate) fn adapter_timing(renderer: &Renderer) -> (wgpu::Features, wgpu::Backend) {
    let context = &renderer.context;
    let supported = context.adapter().features() & wgpu::Features::TIMESTAMP_QUERY;
    let enabled = context.device().features() & wgpu::Features::TIMESTAMP_QUERY;
    assert_eq!(
        enabled, supported,
        "requested supported timestamps must be enabled"
    );
    (supported, context.adapter().get_info().backend)
}

pub(super) async fn context(
    adapter: wgpu::Adapter,
    required: wgpu::Features,
    requested: bool,
) -> Result<GpuContext, ViewerError> {
    let timing = if requested && adapter.features().contains(features()) {
        features()
    } else {
        wgpu::Features::empty()
    };
    Ok(GpuContext::from_adapter_with_features(adapter, required | timing).await?)
}

impl Renderer {
    pub(super) fn collect_timing(&self, host: &mut RetainedHost) -> Result<(), ViewerError> {
        if !host.options.gpu_timing {
            return Ok(());
        }
        let missing = features().difference(self.context.device().features());
        if !missing.is_empty() {
            host.gpu_timing =
                GpuTiming::Unavailable(GpuTimingUnavailable::MissingFeatures(missing));
            return Ok(());
        }
        if matches!(host.gpu_timing, GpuTiming::Disabled) {
            host.gpu_timing = GpuTiming::Pending;
        }
        if let Some(frame) = self.presentation.scheduler.take_timings()?.pop() {
            host.gpu_timing = observation(frame);
        }
        Ok(())
    }
}

fn observation(frame: crate::FrameTiming) -> GpuTiming {
    match frame.passes.iter().find(|pass| pass.duration_ns.is_none()) {
        Some(pass) => GpuTiming::Unavailable(GpuTimingUnavailable::InvalidTimestampPair {
            frame_id: frame.frame_id,
            pass_id: pass.pass_id,
        }),
        None => GpuTiming::Complete(frame),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_without_timestamp_feature_reports_exact_missing_feature() {
        let mut host = crate::RetainedHost::new(
            crate::renderer::retained_tests::source(),
            crate::tests::options(),
        )
        .unwrap();
        let renderer = pollster::block_on(Renderer::windowless(&host)).unwrap();
        assert!(
            !renderer
                .context
                .device()
                .features()
                .contains(wgpu::Features::TIMESTAMP_QUERY)
        );
        host.options.gpu_timing = true;
        renderer.collect_timing(&mut host).unwrap();
        assert_eq!(
            host.gpu_timing(),
            &GpuTiming::Unavailable(GpuTimingUnavailable::MissingFeatures(
                wgpu::Features::TIMESTAMP_QUERY
            ))
        );
    }

    #[test]
    fn unusable_counter_pairs_are_typed_unavailable_instead_of_completed_measurements() {
        let pass_id = crate::PassId::Geometry(crate::PassSlot::Opaque);
        let result = observation(crate::FrameTiming {
            frame_id: 7,
            passes: vec![crate::PassTiming {
                pass_id,
                duration_ns: None,
            }],
        });
        assert_eq!(
            result,
            GpuTiming::Unavailable(GpuTimingUnavailable::InvalidTimestampPair {
                frame_id: 7,
                pass_id,
            })
        );
    }

    #[test]
    fn disabled_request_does_not_enable_queries_on_the_device_or_renderer() {
        let host = crate::RetainedHost::new(
            crate::renderer::retained_tests::source(),
            crate::tests::options(),
        )
        .unwrap();
        let renderer = pollster::block_on(Renderer::windowless(&host)).unwrap();
        assert!(!renderer.context.device().features().contains(features()));
        assert_eq!(
            renderer.presentation.scheduler.configuration().timing,
            worldrunner_render_post::TimingMode::Disabled
        );
    }
}
