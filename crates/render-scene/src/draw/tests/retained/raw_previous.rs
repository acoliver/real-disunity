//! Public raw colour draws advance motion history without publishing view selections.
use super::*;
use crate::test_support::upload_scene;
use worldrunner_render_gpu::submission::{
    PreparedFrame, SubmissionAuthority, SubmitError, TransactionError,
};
use worldrunner_render_post::color_texture;

const HDR: TextureFormat = TextureFormat::Rgba16Float;

struct Fixture {
    context: GpuContext,
    authority: SubmissionAuthority,
    gpu: SceneGpu,
    handle: crate::draw::InstanceHandle,
    pipelines: ScenePipelines,
    targets: [TextureView; 3],
}

impl Fixture {
    fn new() -> Self {
        Self::of(&scene(SceneAlphaMode::Opaque, false, 1))
    }

    fn of(source: &RenderScene) -> Self {
        let context = context();
        let authority = SubmissionAuthority::new(context.queue().clone());
        let pipelines = ScenePipelines::with_taa_velocity(context.device(), HDR, 1).unwrap();
        let gpu = upload_scene(context.device(), &authority, source, &pipelines).unwrap();
        let handle = gpu.instance_handles().next().unwrap();
        gpu.set_frame(context.queue(), temporal_frame());
        let targets = attachments(&context);
        Self {
            context,
            authority,
            gpu,
            handle,
            pipelines,
            targets,
        }
    }

    fn prepare(&mut self) -> PreparedFrame {
        let prepared = self
            .authority
            .prepare(self.context.device(), |encoder, _| {
                self.gpu
                    .flush_instances_encoded(self.context.device(), encoder)?;
                record_raw(encoder, &self.gpu, &self.pipelines, &self.targets)?;
                Ok::<_, TransactionError>(())
            })
            .unwrap();
        self.assert_unpublished();
        prepared
    }

    /// Uploads the instances in one transaction and submits it without drawing, since the
    /// velocity pass draws no skinned geometry.
    fn submit_flush_only(&mut self) {
        self.authority
            .prepare(self.context.device(), |encoder, _| {
                self.gpu
                    .flush_instances_encoded(self.context.device(), encoder)?;
                Ok::<_, TransactionError>(())
            })
            .unwrap()
            .submit()
            .unwrap();
        self.assert_unpublished();
    }

    fn raw_only(&self) -> PreparedFrame {
        self.authority
            .prepare(self.context.device(), |encoder, _| {
                record_raw(encoder, &self.gpu, &self.pipelines, &self.targets)
            })
            .unwrap()
    }

    fn ready(&self) -> bool {
        self.gpu
            .instances
            .batches
            .values()
            .all(|batch| batch.ready_for(None))
    }

    fn pending_history(&self) -> bool {
        self.gpu
            .instances
            .batches
            .values()
            .any(|batch| batch.history.has_pending())
    }

    fn submit(&mut self) {
        self.prepare().submit().unwrap();
        self.assert_unpublished();
    }

    fn move_to(&mut self, x: i64) -> Vec<u8> {
        self.gpu
            .set_instance_transform(self.handle, moved(x))
            .unwrap();
        crate::geometry::pack_object(self.gpu.instance(self.handle).unwrap().0)
    }

    fn buffers(&self) -> (Vec<u8>, Vec<u8>) {
        let batch = self.gpu.instances.batches.values().next().unwrap();
        let read = |buffer: Option<&wgpu::Buffer>| {
            super::scheduler::buffer_bytes(
                &self.context,
                buffer.unwrap(),
                crate::geometry::INSTANCE_BYTES as u64,
            )
        };
        (
            read(batch.buffer.as_deref()),
            read(batch.previous.as_deref()),
        )
    }

    fn skin_bytes(&self) -> Vec<u8> {
        let batch = self.gpu.instances.batches.values().next().unwrap();
        let stream = batch.skin.as_ref().unwrap();
        super::scheduler::buffer_bytes(
            &self.context,
            stream.buffer.as_deref().unwrap(),
            crate::skinning::SKIN_INSTANCE_BYTES as u64,
        )
    }

    fn assert_unpublished(&self) {
        assert_eq!(self.gpu.view_stats().considered, 0);
        assert_eq!(self.gpu.shadow_view_stats().considered, 0);
        for view in &self.gpu.views {
            assert!(view.published.borrow().ranges.is_empty());
        }
    }
}

fn temporal_frame() -> FrameUniform {
    let mut frame = frame_from(pose());
    frame.temporal = Some(crate::frame::temporal::TemporalView {
        unjittered_view_projection: frame.view_projection,
        previous_view_projection: frame.view_projection,
        jitter_pixels: [0.0; 2],
        previous_jitter_pixels: [0.0; 2],
        history_valid: true,
        previous_animation_time_ms: frame.animation_time_milliseconds,
        previous_camera_position: frame.camera_position,
    });
    frame
}

fn attachments(context: &GpuContext) -> [TextureView; 3] {
    [
        color_texture(
            context.device(),
            "raw history colour",
            [TARGET, TARGET],
            HDR,
        ),
        color_texture(
            context.device(),
            "raw history velocity",
            [TARGET, TARGET],
            TextureFormat::Rg16Float,
        ),
        depth_texture(
            context.device(),
            Viewport {
                width: TARGET,
                height: TARGET,
            },
        ),
    ]
    .map(|texture| texture.create_view(&TextureViewDescriptor::default()))
}

fn record_raw(
    encoder: &mut CommandEncoder,
    gpu: &SceneGpu,
    pipelines: &ScenePipelines,
    targets: &[TextureView; 3],
) -> Result<(), TransactionError> {
    gpu.draw(raw_pass(encoder, targets)?, pipelines).unwrap();
    Ok(())
}

fn raw_pass<'encoder>(
    encoder: &'encoder mut CommandEncoder,
    targets: &[TextureView; 3],
) -> Result<worldrunner_render_gpu::submission::RecordingRenderPass<'encoder>, TransactionError> {
    let colors = targets[..2]
        .iter()
        .map(|view| {
            Some(RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::TRANSPARENT),
                    store: StoreOp::Store,
                },
            })
        })
        .collect::<Vec<_>>();
    worldrunner_render_gpu::submission::RecordingRenderPass::begin(
        encoder,
        &RenderPassDescriptor {
            label: Some("raw last-visible instance history"),
            color_attachments: &colors,
            depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                view: &targets[2],
                depth_ops: Some(Operations {
                    load: LoadOp::Clear(1.0),
                    store: StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        },
    )
}

#[test]
fn submitted_raw_colour_advances_last_visible_transforms_and_settles() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut fixture = Fixture::new();
    fixture.submit();
    let (a, previous) = fixture.buffers();
    assert_eq!(previous, a);
    let b = fixture.move_to(250);
    assert_ne!(b, a);
    fixture.submit();
    assert_eq!(fixture.buffers(), (b.clone(), a));
    let c = fixture.move_to(500);
    assert_ne!(c, b);
    fixture.submit();
    assert_eq!(fixture.buffers(), (c.clone(), b));
    fixture.submit();
    assert_eq!(fixture.buffers(), (c.clone(), c.clone()));
    fixture.submit();
    assert_eq!(fixture.buffers(), (c.clone(), c));
}

#[test]
fn repeated_encoded_flush_restores_submitted_transform_before_raw_colour() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut fixture = Fixture::new();
    let a = fixture.move_to(0);
    fixture.submit();
    assert_eq!(fixture.buffers(), (a.clone(), a.clone()));
    let mut intermediate_content = 0;
    let Fixture {
        context,
        authority,
        gpu,
        handle,
        pipelines,
        targets,
    } = &mut fixture;
    authority
        .prepare(context.device(), |encoder, _| {
            for x in [250, 0] {
                gpu.set_instance_transform(*handle, moved(x)).unwrap();
                gpu.flush_instances_encoded(context.device(), encoder)?;
                if x == 250 {
                    intermediate_content = gpu.instances.content_generation();
                }
            }
            record_raw(encoder, gpu, pipelines, targets)
        })
        .unwrap()
        .submit()
        .unwrap();
    fixture.assert_unpublished();
    assert_eq!(
        fixture.buffers(),
        (a.clone(), a.clone()),
        "the second flush must restore A over the staged B bytes"
    );
    assert!(
        fixture.gpu.instances.content_generation() > intermediate_content,
        "the restoring upload must invalidate mirror and gather content"
    );
    let c = fixture.move_to(500);
    assert_ne!(c, a);
    fixture.submit();
    assert_eq!(
        fixture.buffers(),
        (c, a),
        "the next raw colour draw must use submitted A as previous"
    );
}

#[test]
fn repeated_encoded_flush_restores_submitted_skin_descriptor() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut fixture = Fixture::of(&crate::draw::tests::skinning::posed(0));
    let original = [0_u32, 0, 2, 0];
    let batch = fixture.gpu.instances.batches.values_mut().next().unwrap();
    batch.set_skin(0, original);
    fixture.submit_flush_only();
    let expected = original
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect::<Vec<_>>();
    assert_eq!(fixture.skin_bytes(), expected);
    let Fixture {
        context,
        authority,
        gpu,
        ..
    } = &mut fixture;
    authority
        .prepare(context.device(), |encoder, _| {
            for descriptor in [[4, 4, 2, 0], original] {
                gpu.instances
                    .batches
                    .values_mut()
                    .next()
                    .unwrap()
                    .set_skin(0, descriptor);
                gpu.flush_instances_encoded(context.device(), encoder)?;
            }
            Ok::<_, TransactionError>(())
        })
        .unwrap()
        .submit()
        .unwrap();
    assert_eq!(
        fixture.skin_bytes(),
        expected,
        "the second flush must restore the submitted skin descriptor"
    );
}

#[test]
fn cancelling_repeated_flushes_keeps_the_unsubmitted_current_upload_pending() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut fixture = Fixture::new();
    fixture.submit();
    let before = fixture.buffers();
    let b = fixture.move_to(250);
    let Fixture {
        context,
        authority,
        gpu,
        pipelines,
        targets,
        ..
    } = &mut fixture;
    authority
        .prepare(context.device(), |encoder, _| {
            for _ in 0..2 {
                gpu.flush_instances_encoded(context.device(), encoder)?;
            }
            record_raw(encoder, gpu, pipelines, targets)
        })
        .unwrap()
        .cancel();
    assert_eq!(fixture.buffers(), before);
    assert!(!fixture.ready());
    fixture.submit();
    assert_eq!(fixture.buffers(), (b, before.0));
}

#[test]
fn cancelled_raw_colour_does_not_advance_the_last_visible_transform() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut fixture = Fixture::new();
    fixture.submit();
    let before = fixture.buffers();
    fixture.move_to(250);
    let cancelled = fixture.prepare();
    cancelled.cancel();
    fixture.assert_unpublished();
    assert_eq!(fixture.buffers(), before);
    let c = fixture.move_to(500);
    assert_ne!(c, before.0);
    fixture.submit();
    assert_eq!(fixture.buffers(), (c.clone(), before.0));
    fixture.submit();
    assert_eq!(fixture.buffers(), (c.clone(), c));
}

#[test]
fn fresh_velocity_upload_supports_raw_draw_without_edits_or_an_extra_flush() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut fixture = Fixture::new();
    let before = fixture.buffers();
    assert_eq!(before.0, before.1);
    let prepared = fixture
        .authority
        .prepare(fixture.context.device(), |encoder, _| {
            record_raw(encoder, &fixture.gpu, &fixture.pipelines, &fixture.targets)?;
            Ok::<_, TransactionError>(())
        })
        .unwrap();
    fixture.assert_unpublished();
    prepared.submit().unwrap();
    fixture.assert_unpublished();
    assert_eq!(fixture.buffers(), before);
    let moved = fixture.move_to(250);
    assert_ne!(moved, before.0);
    fixture.submit();
    assert_eq!(fixture.buffers(), (moved, before.0));
}

#[test]
fn upload_only_then_later_raw_colour_uses_the_new_colour_owner() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut fixture = Fixture::new();
    fixture.raw_only().submit().unwrap();
    let a = fixture.buffers().0;
    let b = fixture.move_to(250);
    fixture
        .gpu
        .flush_instances(fixture.context.device(), &fixture.authority)
        .unwrap();
    assert!(fixture.ready());
    assert!(fixture.pending_history());
    assert_eq!(fixture.buffers(), (b.clone(), a));
    fixture.raw_only().submit().unwrap();
    fixture.assert_unpublished();
    let c = fixture.move_to(500);
    fixture.submit();
    assert_eq!(fixture.buffers(), (c, b));
}

#[test]
fn raw_colour_without_upload_keeps_history_on_cancel_or_device_loss() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let fixture = Fixture::new();
    assert!(fixture.ready());
    fixture.raw_only().cancel();
    assert!(fixture.pending_history());
    assert!(fixture.ready());
    let refused = fixture.raw_only();
    fixture
        .authority
        .report_device_loss("raw colour regression".into());
    assert!(matches!(refused.submit(), Err(SubmitError::DeviceLost(_))));
    fixture.assert_unpublished();
    assert!(fixture.pending_history());
}

#[test]
fn replacing_a_raw_colour_encoder_cannot_publish_its_candidates() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let fixture = Fixture::new();
    let mut recording = fixture.authority.begin(fixture.context.device()).unwrap();
    record_raw(
        &mut recording,
        &fixture.gpu,
        &fixture.pipelines,
        &fixture.targets,
    )
    .unwrap();
    let replacement = fixture
        .context
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    let original = std::mem::replace(&mut *recording, replacement);
    drop(original);
    assert!(matches!(
        recording.finish().submit(),
        Err(SubmitError::Transaction(TransactionError::ForeignEncoder))
    ));
    assert!(fixture.pending_history());
    fixture.assert_unpublished();
    fixture.raw_only().submit().unwrap();
    assert!(!fixture.pending_history());
}

#[test]
fn raw_readiness_rejects_dirty_cancelled_and_unsettled_previous_bytes() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut fixture = Fixture::new();
    fixture.raw_only().submit().unwrap();
    fixture.move_to(250);
    assert!(!fixture.ready());
    fixture.prepare().cancel();
    assert!(!fixture.ready());
    fixture
        .gpu
        .flush_instances(fixture.context.device(), &fixture.authority)
        .unwrap();
    assert!(fixture.ready());
    assert!(fixture.pending_history());
    fixture.raw_only().submit().unwrap();
    assert!(!fixture.ready());
    fixture.prepare().cancel();
    assert!(!fixture.ready());
    fixture.submit();
    assert!(fixture.ready());
    fixture.gpu.reset_velocity_history();
    assert!(!fixture.ready());
}

#[test]
fn raw_readiness_rejects_cancelled_growth_with_an_empty_current_receipt() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut fixture = Fixture::new();
    let object = fixture.gpu.instance(fixture.handle).unwrap().0;
    let added: Vec<_> = (0..16)
        .map(|_| fixture.gpu.add_instance(object, true).unwrap())
        .collect();
    fixture.prepare().cancel();
    for handle in added {
        fixture.gpu.remove_instance(handle).unwrap();
    }
    assert!(
        fixture
            .gpu
            .instances
            .batches
            .values()
            .all(|batch| batch.pending_ranges().is_empty())
    );
    assert!(!fixture.ready());
    fixture.submit();
    assert!(fixture.ready());
}

#[test]
fn raw_colour_requires_its_history_key_in_the_capture_write_contract() {
    use worldrunner_render_gpu::submission::capture::{CaptureError, CaptureWrites, WriteError};
    let _session = crate::test_support::SceneTestSession::acquire();
    let fixture = Fixture::new();
    let key = fixture.gpu.instance_history_key();
    let serial = fixture.authority.submitted_serial();
    let timeout = std::time::Duration::from_secs(30);
    let deadline = std::time::Instant::now() + timeout;
    fixture
        .context
        .device()
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(timeout),
        })
        .unwrap();
    fixture
        .authority
        .await_completion_record(fixture.context.device(), serial, deadline)
        .unwrap();
    let writes = CaptureWrites::new([], [key], []).unwrap();
    let refused =
        fixture
            .authority
            .begin_capture(fixture.context.device(), writes, (), |encoder, _, ()| {
                let pass = raw_pass(encoder, &fixture.targets)?;
                assert_eq!(
                    fixture.gpu.draw(pass, &fixture.pipelines),
                    Err(WriteError::Undeclared(key))
                );
                Ok::<_, TransactionError>(())
            });
    assert!(
        matches!(refused, Err(CaptureError::Write(WriteError::Undeclared(found))) if found == key)
    );
    assert_eq!(fixture.authority.submitted_serial(), serial);
    assert!(fixture.pending_history());
    fixture.assert_unpublished();
    fixture.raw_only().submit().unwrap();
    assert!(!fixture.pending_history());
}

#[test]
fn raw_draws_of_one_unchanged_transaction_scan_readiness_once() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut fixture = Fixture::new();
    fixture.submit();
    let Fixture {
        context,
        authority,
        gpu,
        handle,
        pipelines,
        targets,
    } = &mut fixture;
    let mut scans = Vec::new();
    authority
        .prepare(context.device(), |encoder, _| {
            gpu.set_instance_transform(*handle, moved(250)).unwrap();
            gpu.flush_instances_encoded(context.device(), encoder)?;
            for _ in 0..3 {
                record_raw(encoder, gpu, pipelines, targets)?;
                scans.push(gpu.instances.ready_scans());
            }
            Ok::<_, TransactionError>(())
        })
        .unwrap()
        .cancel();
    assert_eq!(scans[1], scans[0]);
    assert_eq!(scans[2], scans[0]);
}

#[test]
fn an_edit_after_a_passing_raw_draw_in_the_same_transaction_is_refused() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut fixture = Fixture::new();
    fixture.submit();
    let Fixture {
        context,
        authority,
        gpu,
        handle,
        pipelines,
        targets,
    } = &mut fixture;
    let mut refusal = None;
    authority
        .prepare(context.device(), |encoder, _| {
            gpu.set_instance_transform(*handle, moved(250)).unwrap();
            gpu.flush_instances_encoded(context.device(), encoder)?;
            record_raw(encoder, gpu, pipelines, targets)?;
            gpu.set_instance_transform(*handle, moved(500)).unwrap();
            refusal = Some(std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                || record_raw(encoder, gpu, pipelines, targets),
            )));
            Ok::<_, TransactionError>(())
        })
        .unwrap()
        .cancel();
    let payload = refusal
        .expect("the transaction ran")
        .expect_err("a draw after an unflushed edit must be refused");
    let message = payload
        .downcast_ref::<&str>()
        .map(ToString::to_string)
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_default();
    assert!(
        message.contains("flush instance updates before drawing"),
        "unexpected refusal: {message}"
    );
}
