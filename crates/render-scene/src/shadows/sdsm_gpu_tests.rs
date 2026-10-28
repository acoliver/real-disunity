#[path = "attachment_tests.rs"]
mod attachment_tests;
use super::*;
use crate::{device::GpuContext, frame::metadata::FrameProduct};
use std::{cell::Cell, rc::Rc};
use worldrunner_render_gpu::submission::capture::{
    CaptureError, CaptureOutcome, CaptureWrites, WriteKey,
};
use worldrunner_render_post::AttachmentView;

fn context() -> GpuContext {
    pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty())).unwrap()
}
pub(crate) fn source(
    context: &GpuContext,
    values: [f32; 4],
    covered: bool,
) -> (AttachmentView, AttachmentView) {
    let texture = |format| {
        context.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("SDSM test metadata"),
            size: wgpu::Extent3d {
                width: 2,
                height: 2,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        })
    };
    let depth = texture(wgpu::TextureFormat::R32Float);
    let normals = texture(wgpu::TextureFormat::Rgba16Float);
    let write = |texture: &wgpu::Texture, bytes: &[u8], pitch| {
        context.queue().write_texture(
            texture.as_image_copy(),
            bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(pitch),
                rows_per_image: Some(2),
            },
            texture.size(),
        );
    };
    write(
        &depth,
        &values
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect::<Vec<_>>(),
        8,
    );
    let mut normal_bytes = vec![0_u8; 32];
    if covered {
        for pixel in normal_bytes.chunks_exact_mut(8) {
            pixel[6..8].copy_from_slice(&0x3c00_u16.to_le_bytes());
        }
    }
    write(&normals, &normal_bytes, 16);
    (
        FrameProduct::linear_depth(0).attachment(&depth).unwrap(),
        FrameProduct::normals().attachment(&normals).unwrap(),
    )
}
fn range() -> RangeFit {
    RangeFit {
        near_step: 0.5,
        far_step: 1.0,
        minimum_span: 1.0,
        minimum_span_policy: worldrunner_render_shadow::MinimumSpanPolicy::ExtendNear,
        active_maximum: None,
        split_lambda: 0.5,
        split_policy: worldrunner_render_shadow::SplitPolicy::Practical,
    }
}
fn state() -> AdoptionState {
    AdoptionState::new(SdsmSettings {
        policy: AdoptionPolicy::SameFrameCapture,
        range: range(),
    })
    .unwrap()
}
fn writes() -> CaptureWrites {
    CaptureWrites::new([WriteKey::Readback(1)], [WriteKey::Resident(2)], []).unwrap()
}
fn reducer(context: &GpuContext, source: &(AttachmentView, AttachmentView)) -> DepthReducer {
    DepthReducer::new(
        context.device(),
        (FrameProduct::linear_depth(0), &source.0),
        (FrameProduct::normals(), &source.1),
    )
    .unwrap()
}
fn input() -> CaptureInput<()> {
    CaptureInput {
        frame: (),
        source_frame: 8,
        clip_meters: [0.1, 100.0],
        range: range(),
    }
}

#[test]
fn gpu_same_frame_capture_waits_for_metres_before_suffix_and_publishes_on_submit() {
    let context = context();
    let source = source(&context, [2.7, 7.0, 19.3, 4.0], true);
    let reducer = reducer(&context, &source);
    let state = state();
    let prefix = reducer
        .begin_capture(
            &context,
            (writes(), WriteKey::Readback(1)),
            input(),
            state.ticket(8),
            |_, _, ()| Ok::<_, SdsmError>(()),
        )
        .unwrap();
    let published = Rc::new(Cell::new(None));
    let output = published.clone();
    let suffix = prefix
        .submit()
        .unwrap()
        .resume(context.device(), move |_, edits, (), fit| {
            assert_eq!(fit.source_frame, 8);
            assert_eq!(fit.adopted_frame, 8);
            assert_eq!(
                fit.depth,
                DepthRange::Receivers {
                    near: 2.5,
                    far: 20.0
                }
            );
            edits.stage_write(WriteKey::Resident(2), move || output.set(Some(fit)))?;
            Ok::<_, SdsmError>(())
        })
        .unwrap();
    assert_eq!(published.get(), None);
    suffix
        .submit()
        .unwrap()
        .finish(context.device(), |_, _| Ok::<_, SdsmError>(()))
        .unwrap();
    assert!(published.get().is_some());
}

#[test]
fn gpu_empty_invalid_cancelled_and_resized_prefixes_have_distinct_outcomes() {
    let context = context();
    for (values, covered, empty) in [([0.0; 4], false, true), ([f32::NAN; 4], true, false)] {
        let source = source(&context, values, covered);
        let reducer = reducer(&context, &source);
        let prefix = reducer
            .begin_capture(
                &context,
                (writes(), WriteKey::Readback(1)),
                input(),
                state().ticket(8),
                |_, _, ()| Ok::<_, SdsmError>(()),
            )
            .unwrap();
        let result = prefix
            .submit()
            .unwrap()
            .resume(context.device(), |_, _, (), fit| {
                assert_eq!(fit.depth, DepthRange::EmptyDepth);
                Ok::<_, SdsmError>(())
            });
        if empty {
            result
                .unwrap()
                .submit()
                .unwrap()
                .finish(context.device(), |_, _| Ok::<_, SdsmError>(()))
                .unwrap();
        } else {
            assert!(matches!(
                result,
                Err(CaptureError::Readback(SdsmError::InvalidDepth(_)))
            ));
            context
                .submission()
                .drain_captures(context.device())
                .unwrap();
        }
    }
}

#[test]
fn gpu_cancelled_and_resized_prefixes_cannot_refit() {
    let context = context();
    let source = source(&context, [2.0; 4], true);
    let reducer = reducer(&context, &source);
    let mut state = state();
    let prefix = reducer
        .begin_capture(
            &context,
            (writes(), WriteKey::Readback(1)),
            input(),
            state.ticket(8),
            |_, _, ()| Ok::<_, SdsmError>(()),
        )
        .unwrap();
    let observer = prefix.observer();
    drop(prefix);
    assert_eq!(observer.outcome(), Some(CaptureOutcome::Cancelled));
    let prefix = reducer
        .begin_capture(
            &context,
            (writes(), WriteKey::Readback(1)),
            input(),
            state.ticket(8),
            |_, _, ()| Ok::<_, SdsmError>(()),
        )
        .unwrap();
    let continuation = prefix.submit().unwrap();
    state.invalidate();
    let result = continuation.resume(context.device(), |_, _, (), _| {
        panic!("stale fit reached suffix")
    });
    assert!(matches!(
        result,
        Err(CaptureError::Readback(SdsmError::StaleResource))
    ));
    context
        .submission()
        .drain_captures(context.device())
        .unwrap();
}

#[test]
fn gpu_dropped_prepared_reduction_cannot_start_mapping() {
    let context = context();
    let source = source(&context, [3.0; 4], true);
    let reducer = reducer(&context, &source);
    let mut pending = None;
    let frame = context
        .submission()
        .prepare(context.device(), |encoder, _| {
            pending = Some(reducer.encode(
                context.device(),
                encoder,
                state().ticket(0),
                WriteKey::Readback(1),
            )?);
            Ok::<_, SdsmError>(())
        })
        .unwrap();
    drop(frame);
    assert!(matches!(
        pending.unwrap().begin_mapping(),
        Err(SdsmError::NotSubmitted)
    ));
}

#[test]
fn gpu_device_loss_stops_capture_before_fit() {
    let context = context();
    let source = source(&context, [3.0; 4], true);
    let reducer = reducer(&context, &source);
    let prefix = reducer
        .begin_capture(
            &context,
            (writes(), WriteKey::Readback(1)),
            input(),
            state().ticket(8),
            |_, _, ()| Ok::<_, SdsmError>(()),
        )
        .unwrap();
    let continuation = prefix.submit().unwrap();
    context
        .submission()
        .report_device_loss("injected device failure".into());
    let result = continuation.resume(context.device(), |_, _, (), _| -> Result<(), SdsmError> {
        panic!("lost device reached refit")
    });
    assert!(matches!(
        result,
        Err(CaptureError::Submission(
            worldrunner_render_gpu::submission::SubmitError::DeviceLost(_)
        ))
    ));
}

#[test]
fn gpu_interactive_readback_preserves_delayed_source_and_rebind_invalidates_it() {
    let context = context();
    let first = source(&context, [2.7, 19.3, 4.0, 7.0], true);
    let mut fitting = DepthFitting::new(
        context.device(),
        SdsmSettings {
            policy: AdoptionPolicy::Interactive {
                latency_frames: 2,
                contraction_frames: 2,
            },
            range: range(),
        },
        (&first.0, &first.1),
    )
    .unwrap();
    let mut pending = None;
    let prepared = context
        .submission()
        .prepare(context.device(), |encoder, _| {
            pending = Some(fitting.encode(context.device(), encoder, 10, WriteKey::Readback(1))?);
            Ok::<_, SdsmError>(())
        })
        .unwrap();
    prepared.submit().unwrap();
    let mut pending = pending.unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let depth = loop {
        if let Some(depth) = pending.poll(context.device()).unwrap() {
            break depth;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::yield_now();
    };
    fitting.complete(&pending.ticket, Ok(depth)).unwrap();
    assert!(fitting.adopt(11, [0.1, 100.0]).unwrap().is_none());
    assert_eq!(
        fitting
            .adopt(12, [0.1, 100.0])
            .unwrap()
            .unwrap()
            .source_frame,
        10
    );
    let old = fitting.ticket(13);
    let replacement = source(&context, [8.0; 4], true);
    assert!(
        fitting
            .rebind(context.device(), (&replacement.0, &replacement.1))
            .unwrap()
    );
    assert!(matches!(
        fitting.complete(&old, Ok(depth)),
        Err(SdsmError::StaleResource)
    ));
}

#[test]
fn failed_readback_invalidates_queued_ranges_but_empty_depth_is_successful() {
    let context = context();
    let source = source(&context, [3.0; 4], true);
    let mut fitting = DepthFitting::new(
        context.device(),
        SdsmSettings {
            policy: AdoptionPolicy::Interactive {
                latency_frames: 2,
                contraction_frames: 0,
            },
            range: range(),
        },
        (&source.0, &source.1),
    )
    .unwrap();
    let old = fitting.ticket(10);
    fitting
        .complete(
            &old,
            Ok(DepthRange::Receivers {
                near: 2.0,
                far: 9.0,
            }),
        )
        .unwrap();
    assert!(matches!(
        fitting.complete(&fitting.ticket(11), Err(SdsmError::Callback)),
        Err(SdsmError::Callback)
    ));
    assert_eq!(fitting.adopt(12, [0.1, 20.0]).unwrap(), None);
    assert!(matches!(
        fitting.complete(&old, Ok(DepthRange::EmptyDepth)),
        Err(SdsmError::StaleResource)
    ));
    fitting
        .complete(&fitting.ticket(12), Ok(DepthRange::EmptyDepth))
        .unwrap();
    assert_eq!(
        fitting.adopt(14, [0.1, 20.0]).unwrap().unwrap().depth,
        DepthRange::EmptyDepth
    );
}
