//! The four per-frame uploads of the HDR post path (the exposure step, the TAA block, the
//! temporal block and the temporal camera) recorded through a real [`SubmissionAuthority`]
//! transaction on wgpu's no-op device, which executes copies on the CPU at submission.
//!
//! Two kinds of test share the rig. The value tests (`distinct_frames_...`) hold today and must
//! keep holding: every frame's destinations carry exactly that frame's bytes. The accounting
//! tests (`..._staged_...`, `a_cancelled_frame_...`) pin that each upload goes through the
//! authority's reusable upload staging, so they fail while the uploads still create a fresh
//! `COPY_SRC` buffer each.
use crate::hdr_post::HdrPost;
use crate::upload_staging_tests::{destination, noop, pattern, read};
use crate::{
    AdaptiveExposure, ExposureMetering, FrameError, HdrPostSettings, PostError, TAA_UNIFORM_BYTES,
    TEMPORAL_BLOCK_BYTES, TEMPORAL_CAMERA_BYTES, TaaSettings, TemporalBlock, TemporalCamera,
    ToneMap, ToneMapOutput,
};
use worldrunner_render_gpu::submission::{
    PrepareError, PreparedFrame, StagingStatistics, SubmissionAuthority, TransactionError,
};

/// The frame that uploads no TAA block, which must land as zeros.
const NO_TAA_FRAME: u8 = 2;
const FRAMES: u8 = 4;

/// Everything one frame uploads.
struct Values {
    delta: f32,
    reset: bool,
    initial: f32,
    taa: Option<[u8; TAA_UNIFORM_BYTES]>,
    block: TemporalBlock,
    camera: TemporalCamera,
}

/// Values that differ between frames in every upload. Frame [`NO_TAA_FRAME`] has no TAA block.
fn values(frame: u8) -> Values {
    let step = f32::from(frame);
    Values {
        delta: 0.0625 * (step + 1.0),
        reset: frame.is_multiple_of(2),
        initial: 1.0 + step,
        taa: (frame != NO_TAA_FRAME).then(|| {
            pattern(u32::from(frame), 1, TAA_UNIFORM_BYTES)
                .try_into()
                .unwrap()
        }),
        block: TemporalBlock {
            render: [10, 10],
            output: [16, 16],
            jitter: [0.25 - 0.125 * step, 0.5],
            phase_count: 8,
            settings: TaaSettings::new(0.5 + 0.1 * step),
            history_valid: !frame.is_multiple_of(2),
        },
        camera: TemporalCamera {
            reprojection: matrix(frame),
            depth_terms: [0.5 + f64::from(frame), 2.0 + f64::from(frame)],
            valid: true,
        },
    }
}

fn matrix(frame: u8) -> [[f64; 4]; 4] {
    let mut matrix = [[0.0; 4]; 4];
    for (index, term) in matrix.iter_mut().flatten().enumerate() {
        *term = f64::from(frame) * 0.5 + f64::from(u8::try_from(index).unwrap());
    }
    matrix
}

/// The destination bytes `values` must leave: exposure step, TAA block, temporal block, camera.
fn expected(values: &Values) -> [Vec<u8>; 4] {
    let step = [
        values.delta.to_bits(),
        u32::from(values.reset),
        values.initial.to_bits(),
        0,
    ];
    [
        step.into_iter().flat_map(u32::to_le_bytes).collect(),
        values
            .taa
            .map_or_else(|| vec![0; TAA_UNIFORM_BYTES], |block| block.to_vec()),
        values.block.to_bytes().unwrap().to_vec(),
        values.camera.to_bytes().to_vec(),
    ]
}

/// Bytes the four uploads of one frame stage in total.
fn frame_bytes() -> u64 {
    [
        16,
        TAA_UNIFORM_BYTES,
        TEMPORAL_BLOCK_BYTES,
        TEMPORAL_CAMERA_BYTES,
    ]
    .iter()
    .map(|&len| u64::try_from(len).unwrap())
    .sum()
}

fn adaptive_settings() -> HdrPostSettings {
    HdrPostSettings {
        adaptive: Some(AdaptiveExposure {
            metering: ExposureMetering::TrimmedMean,
            log_range: [-12.0, 12.0],
            percentiles: [0.1, 0.9],
            key: 0.18,
            exposure_range: [0.01, 64.0],
            speeds: [3.0, 1.0],
            fixed_delta: None,
            fixed_exposure: None,
        }),
        ..Default::default()
    }
}

struct Rig {
    device: wgpu::Device,
    queue: wgpu::Queue,
    authority: SubmissionAuthority,
    post: HdrPost,
    taa: wgpu::Buffer,
    temporal: wgpu::Buffer,
    camera: wgpu::Buffer,
}

impl Rig {
    fn new() -> Self {
        let (device, queue, authority) = noop();
        let post = HdrPost::new(&device, &adaptive_settings()).unwrap();
        let readable = |len: usize| destination(&device, u64::try_from(len).unwrap());
        Self {
            taa: readable(TAA_UNIFORM_BYTES),
            temporal: readable(TEMPORAL_BLOCK_BYTES),
            camera: readable(TEMPORAL_CAMERA_BYTES),
            post,
            device,
            queue,
            authority,
        }
    }

    /// Records the four uploads of one frame in the order the compositor does.
    fn record(&self, values: &Values) -> Result<PreparedFrame, PrepareError<PostError>> {
        self.authority.prepare(&self.device, |encoder, _| {
            self.post
                .prepare(encoder, 0, values.delta, values.reset, values.initial)?;
            crate::hdr_taa::upload(&self.device, encoder, &self.taa, values.taa.as_ref())?;
            crate::hdr_taa::upload_temporal(&self.device, encoder, &self.temporal, &values.block)?;
            crate::hdr_taa::upload_camera(&self.device, encoder, &self.camera, values.camera)?;
            Ok::<_, PostError>(())
        })
    }

    /// Submits `prepared` and waits until the device has finished it, so the staging it used can
    /// map again.
    fn submit_and_wait(&self, prepared: PreparedFrame) {
        let submission = prepared.submit().unwrap();
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(std::time::Duration::from_secs(30)),
            })
            .unwrap();
    }

    fn observed(&self) -> [Vec<u8>; 4] {
        [
            self.post.exposure_step_buffer(0),
            &self.taa,
            &self.temporal,
            &self.camera,
        ]
        .map(|buffer| read(&self.device, &self.queue, buffer))
    }
}

#[test]
fn distinct_frames_deliver_their_exact_values_to_every_destination() {
    let rig = Rig::new();
    let mut previous = None;
    for frame in 0..FRAMES {
        let frame_values = values(frame);
        rig.submit_and_wait(rig.record(&frame_values).unwrap());
        let observed = rig.observed();
        assert_eq!(observed, expected(&frame_values), "frame {frame}");
        if let Some(previous) = previous {
            for (now, before) in observed.iter().zip(&previous) {
                assert_ne!(now, before, "frame {frame} must differ from the one before");
            }
        }
        previous = Some(observed);
    }
}

#[test]
fn a_frame_without_a_taa_block_uploads_zeros_over_the_previous_block() {
    let rig = Rig::new();
    rig.submit_and_wait(rig.record(&values(1)).unwrap());
    assert!(rig.observed()[1].iter().any(|&byte| byte != 0));
    let none = values(NO_TAA_FRAME);
    assert!(none.taa.is_none());
    rig.submit_and_wait(rig.record(&none).unwrap());
    assert_eq!(rig.observed()[1], vec![0; TAA_UNIFORM_BYTES]);
}

#[test]
fn every_frame_stages_its_four_post_uploads_and_reuses_the_mapped_chunk() {
    let rig = Rig::new();
    for frame in 0..FRAMES {
        rig.submit_and_wait(rig.record(&values(frame)).unwrap());
        let frames = u64::from(frame) + 1;
        let statistics = rig.authority.upload_staging();
        // The frame without a TAA block still stages its 96 zero bytes.
        assert_eq!(
            (statistics.uploads, statistics.uploaded_bytes),
            (4 * frames, frame_bytes() * frames),
            "staged uploads after frame {frame}: {statistics:?}"
        );
    }
    // Each frame is finished before the next records, so one chunk serves all of them.
    let statistics = rig.authority.upload_staging();
    assert_eq!((statistics.chunks_created, statistics.chunks), (1, 1));
}

#[test]
fn a_cancelled_frame_returns_its_staging_and_leaves_no_values_behind() {
    let rig = Rig::new();
    let landed = values(0);
    rig.submit_and_wait(rig.record(&landed).unwrap());
    // A cancelled frame, then a dropped one: neither submits, so the destinations keep the
    // landed frame's bytes, and both give their staging back at once.
    for (cancelled, frame) in [(true, 1), (false, 2)] {
        let prepared = rig.record(&values(frame)).unwrap();
        if cancelled {
            prepared.cancel();
        } else {
            drop(prepared);
        }
        assert_eq!(rig.observed(), expected(&landed), "after frame {frame}");
    }
    let next = values(3);
    rig.submit_and_wait(rig.record(&next).unwrap());
    assert_eq!(rig.observed(), expected(&next));
    let statistics = rig.authority.upload_staging();
    assert_eq!(
        (statistics.uploads, statistics.uploaded_bytes),
        (4 * 4, frame_bytes() * 4),
        "every recorded frame stages its uploads, submitted or not: {statistics:?}"
    );
    assert_eq!((statistics.chunks_created, statistics.chunks), (1, 1));
}

#[test]
fn an_invalid_exposure_delta_fails_the_frame_and_stages_nothing() {
    let rig = Rig::new();
    for delta in [f32::NAN, f32::INFINITY, -1.0, 10.5] {
        let result = rig.authority.prepare(&rig.device, |encoder, _| {
            rig.post.prepare(encoder, 0, delta, false, 1.0)
        });
        assert!(
            matches!(
                result,
                Err(PrepareError::Preparation(PostError::FrameDelta))
            ),
            "delta {delta}"
        );
        assert_eq!(rig.authority.upload_staging(), StagingStatistics::default());
    }
}

#[test]
fn an_invalid_temporal_block_fails_the_frame_and_stages_nothing() {
    let rig = Rig::new();
    let valid = values(0).block;
    let invalid = [
        TemporalBlock {
            phase_count: 0,
            ..valid
        },
        TemporalBlock {
            jitter: [f32::NAN, 0.0],
            ..valid
        },
        TemporalBlock {
            jitter: [2.0, 0.0],
            ..valid
        },
        TemporalBlock {
            render: [0, 10],
            ..valid
        },
    ];
    for block in invalid {
        let result = rig.authority.prepare(&rig.device, |encoder, _| {
            crate::hdr_taa::upload_temporal(&rig.device, encoder, &rig.temporal, &block)
        });
        assert!(
            matches!(
                result,
                Err(PrepareError::Preparation(PostError::UnsupportedProfile(_)))
            ),
            "{block:?}"
        );
        assert_eq!(rig.authority.upload_staging(), StagingStatistics::default());
    }
}

/// The factory a host holds, with effects so that every upload has a slot to land in.
fn factory(device: &wgpu::Device) -> ToneMapOutput {
    ToneMapOutput::new(wgpu::TextureFormat::Rgba8UnormSrgb, 1.0, ToneMap::Reinhard)
        .unwrap()
        .with_effects(device, adaptive_settings())
        .unwrap()
}

/// The four uploads of one frame through the factory's public methods, recorded into `encoder`.
fn uploads(
    factory: &ToneMapOutput,
    encoder: &mut wgpu::CommandEncoder,
) -> [(&'static str, Result<(), PostError>); 4] {
    let frame = values(0);
    [
        (
            "exposure step",
            factory.prepare_effects(encoder, 0, frame.delta, frame.reset),
        ),
        (
            "TAA block",
            factory.prepare_taa(encoder, 0, frame.taa.as_ref()),
        ),
        (
            "temporal block",
            factory.prepare_temporal(encoder, 0, &frame.block),
        ),
        (
            "temporal camera",
            factory.prepare_temporal_camera(encoder, 0, frame.camera),
        ),
    ]
}

fn transaction_error(error: TransactionError) -> Result<(), PostError> {
    Err(PostError::Frame(FrameError::Transaction(error)))
}

#[test]
fn every_upload_outside_a_prepared_frame_reports_the_missing_transaction() {
    let (device, _queue, authority) = crate::upload_staging_tests::noop();
    let factory = factory(&device);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    for (upload, result) in uploads(&factory, &mut encoder) {
        assert_eq!(
            result,
            transaction_error(TransactionError::Missing),
            "{upload}"
        );
    }
    assert_eq!(authority.upload_staging(), StagingStatistics::default());
}

#[test]
fn every_upload_recorded_into_a_foreign_encoder_reports_it_and_stages_nothing() {
    let (device, _queue, authority) = crate::upload_staging_tests::noop();
    let factory = factory(&device);
    let mut results = Vec::new();
    let prepared = authority
        .prepare(&device, |_, _| {
            let mut foreign =
                device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            results.extend(uploads(&factory, &mut foreign));
            Ok::<_, PostError>(())
        })
        .unwrap();
    assert_eq!(results.len(), 4);
    for (upload, result) in results {
        assert_eq!(
            result,
            transaction_error(TransactionError::ForeignEncoder),
            "{upload}"
        );
    }
    prepared.cancel();
    assert_eq!(authority.upload_staging(), StagingStatistics::default());
}

#[test]
fn a_failing_temporal_block_after_a_staged_camera_cancels_the_frame_cleanly() {
    let rig = Rig::new();
    let landed = values(0);
    rig.submit_and_wait(rig.record(&landed).unwrap());
    let doomed = values(1);
    let invalid = TemporalBlock {
        phase_count: 0,
        ..doomed.block
    };
    // The order of the compositor: the camera is staged, then the block fails its validation.
    let result = rig.authority.prepare(&rig.device, |encoder, _| {
        crate::hdr_taa::upload_camera(&rig.device, encoder, &rig.camera, doomed.camera)?;
        crate::hdr_taa::upload_temporal(&rig.device, encoder, &rig.temporal, &invalid)
    });
    assert!(
        matches!(
            result,
            Err(PrepareError::Preparation(PostError::UnsupportedProfile(_)))
        ),
        "the invalid block must fail the frame"
    );
    let statistics = rig.authority.upload_staging();
    assert_eq!(
        (statistics.uploads, statistics.uploaded_bytes),
        (
            4 + 1,
            frame_bytes() + u64::try_from(TEMPORAL_CAMERA_BYTES).unwrap()
        ),
        "the failed frame staged only its camera: {statistics:?}"
    );
    assert_eq!(
        rig.observed(),
        expected(&landed),
        "nothing of it was copied"
    );
    // Its staging went back at once, the authority is free, and the next frame's destinations
    // carry its own values, none of the cancelled ones.
    let next = values(3);
    rig.submit_and_wait(rig.record(&next).unwrap());
    assert_eq!(rig.observed(), expected(&next));
    let statistics = rig.authority.upload_staging();
    assert_eq!((statistics.chunks_created, statistics.chunks), (1, 1));
}
