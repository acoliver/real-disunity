#[test]
fn recipe_returns_linear_hdr_before_tone_mapping_and_plain_bytes_are_unchanged() {
    let mut h = Harness::with_profile(
        FrameProfile::Hdr {
            samples: 1,
            tone_map: crate::ToneMap::Reinhard,
        },
        crate::presentation::COLOR,
    );
    let effects = bright_effects(h.host.scene.effects().unwrap());
    h.host
        .apply(RetainedUpdate::Settings(Box::new(effects)))
        .unwrap();
    h.wait_prepared();
    let recipe = crate::RetainedCaptureRecipe::new(
        1,
        1,
        crate::CaptureJitter::None,
        false,
        vec![crate::CaptureOutput::Srgb8, crate::CaptureOutput::LinearHdr],
        1024 * 1024,
    )
    .unwrap();
    let request = h.host.request_capture_recipe(1, recipe).unwrap();
    h.frame();
    let products = recipe_result(&mut h, request);
    let linear = products.linear_hdr.unwrap();
    assert_eq!(linear.viewport, products.srgb8.as_ref().unwrap().viewport);
    assert!(linear.pixels.iter().any(|pixel| pixel[0] > 1.0));
    assert_eq!(linear, direct_linear(&h, linear.viewport));
    h.host.request_capture(1).unwrap();
    h.frame();
    assert_eq!(products.srgb8.unwrap(), h.result().3);
}

fn bright_effects(initial: &SceneEffects) -> SceneEffects {
    SceneEffects::new(SceneEffectsParts {
        water: None,
        sky: *initial.sky(),
        sun: SceneSun {
            direction: SceneDirection::new([0, 0, 1_000_000]).unwrap(),
            irradiance_micro: 10_000_000,
        },
        ambient: *initial.ambient(),
        shadows: initial.shadows().clone(),
        ssao: None,
        camera: *initial.camera(),
        grading: *initial.grading(),
        post: vec![],
        anti_alias: *initial.anti_alias(),
    })
    .unwrap()
}

fn recipe_result(h: &mut Harness, request: u64) -> crate::RetainedCaptureProducts {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        h.poll();
        if let Some(RetainedEvent::Capture {
            id,
            generation,
            outcome:
                RetainedCaptureOutcome::RecipeCompleted {
                    frame, products, ..
                },
        }) = h.host.captures.take_event()
        {
            assert_eq!((id, generation, frame), (request, 1, 0));
            return products;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::yield_now();
    }
}

fn direct_linear(
    h: &Harness,
    viewport: Viewport,
) -> worldrunner_render_scene::offscreen::LinearRgbaImage {
    let mut readback = worldrunner_render_post::Readback::new(
        h.context.device(),
        worldrunner_render_post::FrameConfig::new([64, 64], wgpu::TextureFormat::Rgba16Float, 1),
        1024 * 1024,
    )
    .unwrap();
    let bytes = readback
        .read(
            h.context.device(),
            h.context.submission(),
            h.presentation.scheduler.targets().texture(),
        )
        .unwrap();
    worldrunner_render_scene::offscreen::LinearRgbaImage::from_native(
        viewport,
        wgpu::TextureFormat::Rgba16Float,
        &bytes,
    )
    .unwrap()
}

#[test]
fn continuous_ready_requests_cannot_starve_a_successor_capture() {
    let mut h = Harness::new();
    let cohort: Vec<_> = (0..4).map(|_| h.host.request_capture(0).unwrap()).collect();
    h.replacement([255, 0, 0, 255]);
    let successor = h.host.request_capture(1).unwrap();
    let mut delivered = std::collections::BTreeSet::new();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        h.poll();
        let ready = h.host.ready_generation();
        let request = h.host.request_capture(ready);
        if ready == 0 {
            assert!(matches!(
                request,
                Err(crate::CaptureRequestError::BackPressure { .. })
            ));
        }
        h.frame().wait(h.context.device()).unwrap();
        h.poll();
        let mut complete = false;
        while let Some(RetainedEvent::Capture {
            id,
            generation,
            outcome,
        }) = h.host.captures.take_event()
        {
            let RetainedCaptureOutcome::Completed { image, .. } = outcome else {
                panic!("accepted capture was cancelled");
            };
            if cohort.contains(&id) {
                assert_eq!(generation, 0);
                assert_eq!(center(&image), [255, 255, 255, 255]);
                delivered.insert(id);
            }
            if id == successor {
                assert_eq!(generation, 1);
                assert_eq!(center(&image), [255, 0, 0, 255]);
                complete = true;
            }
        }
        if complete {
            assert_eq!(delivered.len(), cohort.len());
            assert_eq!(h.host.ready_generation(), 1);
            break;
        }
        assert!(std::time::Instant::now() < deadline, "publication starved");
        std::thread::yield_now();
    }
}

#[test]
fn submitted_capture_completes_with_zero_extent_and_no_more_frames() {
    let mut h = Harness::new();
    let id = h.host.request_capture(0).unwrap();
    h.frame();
    h.host.input(crate::RetainedInput::Resized([0, 0]));
    assert!(h.host.view().unwrap().is_none());
    assert!(h.streaming.captures.is_pending());
    let (completed, generation, frame, image) = h.result();
    assert_eq!((completed, generation, frame), (id, 0, 0));
    assert_eq!(center(&image), [255, 255, 255, 255]);
    assert_eq!(h.streaming.captures.frame, 1);
    assert!(!h.streaming.captures.is_pending());
}

use super::super::{GpuContext, Presentation, Resident, readiness::Streaming};
use super::*;
use crate::{FrameProfile, RetainedCaptureOutcome, RetainedEvent, RetainedUpdate};
use worldrunner_render_gpu::flight::FrameAdmission;
use worldrunner_render_plan::*;
use worldrunner_render_scene::{Viewport, offscreen::RgbaImage};

struct Harness {
    context: GpuContext,
    resident: Resident,
    presentation: Presentation,
    streaming: Streaming,
    host: RetainedHost,
    flight: FrameAdmission,
    output: wgpu::Texture,
}
impl Harness {
    fn new() -> Self {
        Self::with_profile(FrameProfile::Legacy, crate::presentation::COLOR)
    }
    fn with_profile(profile: FrameProfile, format: wgpu::TextureFormat) -> Self {
        let context =
            pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty())).unwrap();
        let source = super::super::retained_tests::source();
        let size = Viewport {
            width: 64,
            height: 64,
        };
        let resident = Resident::new(&context, &source, None, profile).unwrap();
        let presentation =
            Presentation::with_profile(&context, format, size, size, profile, source.effects())
                .unwrap();
        let output = worldrunner_render_post::color_texture(
            context.device(),
            "capture test output",
            [64, 64],
            format,
        );
        let host = RetainedHost::new(
            source,
            crate::ViewerOptions {
                size: [64, 64],
                frame_profile: profile,
                ..crate::tests::options()
            },
        )
        .unwrap();
        Self {
            context,
            resident,
            presentation,
            streaming: Streaming::new(None, profile),
            host,
            flight: FrameAdmission::new(3).unwrap(),
            output,
        }
    }
    fn publish(&mut self) {
        self.streaming
            .publish(
                &self.context,
                &mut self.resident,
                &mut self.presentation,
                &mut self.host,
            )
            .unwrap();
    }
    fn frame(&mut self) -> CompletionTicket {
        self.publish();
        let admission = self.flight.try_acquire().unwrap();
        let mut capture = self.prepare_capture();
        let view = self.host.view().unwrap().unwrap();
        self.resident.select_frame_slot(admission.slot());
        self.presentation
            .scheduler
            .select_frame_slot(admission.slot());
        let prepared = self
            .context
            .submission()
            .prepare(self.context.device(), |encoder, _| {
                self.resident.render(
                    &self.context,
                    &view,
                    &self.presentation.targets,
                    encoder,
                    self.presentation.scheduler.encoding(),
                )?;
                self.presentation.present(
                    encoder,
                    &self
                        .output
                        .create_view(&wgpu::TextureViewDescriptor::default()),
                )?;
                if let Some((_, image)) = &mut capture {
                    image.replay(
                        self.context.device(),
                        encoder,
                        &self.resident,
                        &self.presentation.scheduler,
                    );
                    image.encode(
                        encoder,
                        &self.output,
                        self.presentation.scheduler.targets().texture(),
                    )?;
                }
                Ok::<_, ViewerError>(())
            })
            .unwrap();
        let ticket = self.flight.submit_prepared(admission, prepared).unwrap();
        self.streaming
            .captures
            .submitted(
                &mut self.host,
                capture,
                ticket.clone(),
                std::time::Duration::ZERO,
            )
            .unwrap();
        ticket
    }
    fn prepare_capture(&mut self) -> Option<(u64, super::Products)> {
        self.streaming
            .captures
            .prepare(
                self.context.device(),
                &mut self.host,
                &self.output,
                &self.presentation.scheduler,
                false,
            )
            .unwrap()
    }
    fn poll(&mut self) {
        self.context.device().poll(wgpu::PollType::Poll).unwrap();
        self.streaming.captures.collect(&mut self.host).unwrap();
    }
    fn result(&mut self) -> (u64, u64, u64, RgbaImage) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            self.poll();
            if let Some(RetainedEvent::Capture {
                id,
                generation,
                outcome: RetainedCaptureOutcome::Completed { frame, image, .. },
            }) = self.host.captures.take_event()
            {
                return (id, generation, frame, image);
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
    }
    fn replacement(&mut self, pixel: [u8; 4]) {
        let source = self.host.scene.clone();
        let texture = SceneTexture::Texels {
            format: TexelFormat::Rgba8Unorm,
            color_space: TextureColorSpace::Srgb,
            width: 1,
            height: 1,
            mips: 1,
            bytes: pixel.into(),
        };
        self.host
            .apply(RetainedUpdate::Resources {
                resources: SceneResources::new(
                    vec![texture],
                    source.resources().materials().to_vec(),
                )
                .unwrap(),
                meshes: source.geometry().meshes().to_vec(),
            })
            .unwrap();
    }
    fn wait_prepared(&mut self) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while self.host.ready_generation() != self.host.accepted_generation() {
            self.poll();
            self.publish();
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
    }
}

fn center(image: &RgbaImage) -> &[u8] {
    let offset = ((image.viewport.height / 2 * image.viewport.width + image.viewport.width / 2) * 4)
        as usize;
    &image.pixels[offset..offset + 4]
}

#[test]
fn requested_generation_waits_for_publication_and_returns_its_completed_pixels() {
    let mut h = Harness::new();
    let initial = h.host.request_capture(0).unwrap();
    h.frame();
    let (id, generation, frame, image) = h.result();
    assert_eq!((id, generation, frame), (initial, 0, 0));
    assert_eq!(center(&image), [255, 255, 255, 255]);
    let (release, gate) = std::sync::mpsc::sync_channel(1);
    h.host.readiness.preparation_hook = Some(Box::new(move || gate.recv().unwrap()));
    h.replacement([255, 0, 0, 255]);
    let red = h.host.request_capture(1).unwrap();
    for _ in 0..3 {
        h.frame().wait(h.context.device()).unwrap();
        h.poll();
        assert_eq!(h.host.ready_generation(), 0);
        assert!(h.host.captures.take_event().is_none());
    }
    release.send(()).unwrap();
    h.wait_prepared();
    h.replacement([0, 255, 0, 255]);
    h.frame();
    let (id, generation, frame, image) = h.result();
    assert_eq!((id, generation, frame), (red, 1, 4));
    assert_eq!(center(&image), [255, 0, 0, 255]);
    h.wait_prepared();
    let green = h.host.request_capture(2).unwrap();
    h.frame();
    let (id, generation, frame, image) = h.result();
    assert_eq!((id, generation, frame), (green, 2, 5));
    assert_eq!(center(&image), [0, 255, 0, 255]);
    assert!(!h.streaming.captures.spare.is_empty());
}

#[test]
fn production_collection_retains_replaced_resident_and_submitted_capture() {
    let mut h = Harness::new();
    h.host.request_capture(0).unwrap();
    h.replacement([255, 0, 0, 255]);
    // Finish CPU preparation without publishing or polling GPU completion.
    let receive = h
        .host
        .readiness
        .queue
        .front()
        .unwrap()
        .scene
        .as_ref()
        .unwrap();
    let prepared = receive
        .recv_timeout(std::time::Duration::from_secs(10))
        .unwrap()
        .unwrap();
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    send.send(Ok(prepared)).unwrap();
    h.host.readiness.queue.front_mut().unwrap().scene = Some(receive);
    let ticket = h.frame();
    let serial = h.context.submission().submitted_serial();
    assert!(!ticket.is_complete());
    assert!(h.context.submission().completed_serial() < serial);
    h.streaming.captures.collect(&mut h.host).unwrap();
    assert_eq!(h.streaming.captures.pending.len(), 1);
    h.publish();
    assert_eq!(h.host.ready_generation(), 1);
    assert_eq!(h.streaming.retired.len(), 1);
    assert_eq!(h.streaming.retired[0].0, serial);
    assert_eq!(h.streaming.captures.pending.len(), 1);
    assert!(h.streaming.captures.spare.is_empty());
    // Upload may dispatch completion callbacks; collection above ran before upload.
    ticket.wait(h.context.device()).unwrap();
    h.poll();
    h.publish();
    assert!(h.streaming.retired.is_empty());
    assert!(h.streaming.captures.pending.is_empty());
    assert_eq!(h.streaming.captures.spare.len(), 1);
}

#[test]
fn hdr_upscaled_bgra_capture_reads_the_final_display_image() {
    let mut h = Harness::with_profile(
        FrameProfile::HdrUpscaled {
            samples: 1,
            tone_map: crate::ToneMap::Reinhard,
            scale: worldrunner_render_plan::post::RenderScale::new(500_000).unwrap(),
            upscale: worldrunner_render_plan::post::UpscaleSettings {
                mode: worldrunner_render_plan::post::UpscaleMode::Easu,
                sharpness: None,
            },
        },
        wgpu::TextureFormat::Bgra8UnormSrgb,
    );
    h.replacement([255, 0, 0, 255]);
    h.wait_prepared();
    h.host.request_capture(1).unwrap();
    h.frame();
    let (_, generation, frame, image) = h.result();
    assert_eq!((generation, frame), (1, 0));
    assert_eq!(
        image.viewport,
        Viewport {
            width: 64,
            height: 64
        }
    );
    assert_eq!(h.presentation.targets.color.texture().width(), 32);
    let pixel = center(&image);
    assert!(pixel[0] > 100 && pixel[0] < 255);
    assert_eq!(&pixel[1..], [0, 0, 255]);
}

#[test]
fn three_gpu_slots_are_reused_without_skipping_waiting_generation() {
    let mut h = Harness::new();
    for _ in 0..4 {
        h.host.request_capture(0).unwrap();
    }
    h.replacement([255, 0, 0, 255]);
    for _ in 0..3 {
        h.frame().wait(h.context.device()).unwrap();
    }
    assert_eq!(h.streaming.captures.pending.len(), 3);
    h.frame().wait(h.context.device()).unwrap();
    assert_eq!(h.host.ready_generation(), 0);
    assert_eq!(h.host.captures.waiting.len(), 1);
    h.poll();
    assert_eq!(h.streaming.captures.spare.len(), 3);
    h.frame().wait(h.context.device()).unwrap();
    h.poll();
    assert!(h.host.captures.waiting.is_empty());
    assert_eq!(h.streaming.captures.spare.len(), 3);
    h.wait_prepared();
    assert_eq!(h.host.ready_generation(), 1);
}

#[test]
fn session_exit_cancels_a_real_submitted_capture_before_releasing_its_buffer() {
    use crate::{RetainedDriver, RetainedTick, app::Content, retained::Session};
    struct Driver(Vec<RetainedEvent>);
    impl RetainedDriver for Driver {
        fn update(&mut self, _: &mut RetainedHost, _: &RetainedTick) -> Result<(), String> {
            Ok(())
        }
        fn event(
            &mut self,
            _host: &mut crate::RetainedEventHost<'_>,
            event: &RetainedEvent,
        ) -> Result<(), String> {
            self.0.push(event.clone());
            Ok(())
        }
    }
    let mut h = Harness::new();
    let id = h.host.request_capture(0).unwrap();
    let ticket = h.frame();
    assert!(!ticket.is_complete());
    assert_eq!(h.streaming.captures.pending.len(), 1);
    let mut driver = Driver(Vec::new());
    let mut session = Session::new(h.host, &mut driver);
    session.shutdown().unwrap();
    drop(session);
    assert_eq!(
        driver.0,
        vec![
            RetainedEvent::Capture {
                id,
                generation: 0,
                outcome: RetainedCaptureOutcome::Cancelled,
            },
            RetainedEvent::Shutdown
        ]
    );
    assert_eq!(h.streaming.captures.pending.len(), 1);
    ticket.wait(h.context.device()).unwrap();
    assert!(ticket.is_complete());
    drop(h.streaming);
}
