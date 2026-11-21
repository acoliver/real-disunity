use super::*;
use crate::{FrameProfile, RetainedUpdate};
use worldrunner_render_plan::{SceneOpaquePath, SceneSsr};
use worldrunner_render_scene::compositor::opaque::OpaqueAdmissionError;
use worldrunner_render_scene::{Viewport, device::GpuContext, offscreen::OffscreenTarget};

struct Fixture {
    resident: Resident,
    presentation: Presentation,
    host: RetainedHost,
    streaming: Streaming,
    target: OffscreenTarget,
}

impl Fixture {
    fn new(context: &GpuContext, rebuild: bool) -> Self {
        let source = super::super::retained_tests::source();
        let size = Viewport {
            width: 64,
            height: 64,
        };
        let profile = FrameProfile::Hdr {
            samples: 1,
            tone_map: crate::ToneMap::Reinhard,
        };
        let mut host = RetainedHost::new(
            source.clone(),
            crate::ViewerOptions {
                size: [64, 64],
                frame_profile: profile,
                shadows: None,
                ..crate::tests::options()
            },
        )
        .unwrap();
        let mut effects = source
            .effects()
            .unwrap()
            .clone()
            .with_opaque_path(SceneOpaquePath::GBUFFER_RENDER)
            .unwrap();
        if rebuild {
            effects = effects.with_ssr(SceneSsr::default()).unwrap();
        }
        host.apply(RetainedUpdate::Settings(Box::new(effects)))
            .unwrap();
        if rebuild {
            let (send, receive) = std::sync::mpsc::sync_channel(1);
            send.send(Ok(worldrunner_render_scene::draw::PreparedScene::new(
                host.scene(),
            )
            .unwrap()))
                .unwrap();
            host.readiness.queue.front_mut().unwrap().scene = Some(receive);
        }
        Self {
            resident: Resident::new(context, &source, None, profile).unwrap(),
            presentation: crate::presentation::fixtures::presentation_with_profile(
                context,
                crate::presentation::COLOR,
                size,
                profile,
                source.effects(),
            )
            .unwrap(),
            host,
            streaming: Streaming::new(None, profile),
            target: OffscreenTarget::new(context.device(), size).unwrap(),
        }
    }

    fn assert_refused(&mut self, context: &GpuContext) {
        let generation = self.presentation.scheduler.generation();
        let before = super::tests::draw(
            context,
            &mut self.resident,
            &mut self.presentation,
            &self.host,
            &mut self.target,
        );
        for _ in 0..2 {
            let error = self
                .streaming
                .publish(
                    context,
                    &mut self.resident,
                    &mut self.presentation,
                    &mut self.host,
                )
                .unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains(&OpaqueAdmissionError::UnsupportedOpposingReceiver.to_string())
            );
            assert_eq!(self.presentation.scheduler.generation(), generation);
            assert_eq!(self.host.readiness.published, 0);
            assert_eq!(self.host.ready_generation(), 0);
            assert_eq!(self.host.accepted_generation(), 1);
            assert_eq!(self.host.preparations_in_flight(), 1);
            assert!(self.streaming.objects.handles.is_none());
            assert_eq!(
                self.presentation.scheduler.configuration().opaque_path,
                SceneOpaquePath::Forward
            );
            assert_eq!(
                super::tests::draw(
                    context,
                    &mut self.resident,
                    &mut self.presentation,
                    &self.host,
                    &mut self.target
                ),
                before
            );
        }
    }
}

#[test]
fn refused_in_place_route_change_keeps_published_scene_and_frame() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let context = GpuContext::shared_for_test_with_features(wgpu::Features::empty()).unwrap();
    Fixture::new(&context, false).assert_refused(&context);
}

#[test]
fn refused_prepared_replacement_keeps_published_scene_and_frame() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let context = GpuContext::shared_for_test_with_features(wgpu::Features::empty()).unwrap();
    Fixture::new(&context, true).assert_refused(&context);
}
