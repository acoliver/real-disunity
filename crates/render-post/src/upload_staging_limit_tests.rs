//! Upload staging on devices whose buffer limit is below or between staging chunk sizes, and
//! staging through the publications a capture hands its callbacks. Each body takes a device, so
//! it runs on wgpu's no-op device (which validates creation against the requested limits and
//! executes copies at submission) and on the test GPU.
use crate::PrepareContext;
use crate::upload_staging_tests::{destination, pattern, read};
use worldrunner_render_gpu::submission::{
    Publications, SubmissionAuthority,
    capture::{CaptureWrites, WriteKey},
};

/// Below the smallest staging chunk, and a limit between two powers of two.
const LIMITS: [u64; 2] = [128 * 1024, 196_612];

fn limits(max_buffer_size: u64) -> wgpu::Limits {
    wgpu::Limits {
        max_buffer_size,
        ..Default::default()
    }
}

fn noop(max_buffer_size: u64) -> (wgpu::Device, wgpu::Queue) {
    wgpu::Device::noop(&wgpu::DeviceDescriptor {
        required_limits: limits(max_buffer_size),
        ..Default::default()
    })
}

fn gpu(max_buffer_size: u64) -> (wgpu::Device, wgpu::Queue) {
    let gpu = worldrunner_render_gpu::test_support::TestGpu::shared(
        &worldrunner_render_gpu::device::DeviceRequirements {
            required_limits: limits(max_buffer_size),
            ..Default::default()
        },
    )
    .unwrap();
    (gpu.device, gpu.queue)
}

fn len(bytes: u64) -> usize {
    usize::try_from(bytes).unwrap()
}

/// A 4-byte `PrepareContext` upload on a device whose buffers may not reach the smallest
/// staging chunk.
fn small_upload(device: &wgpu::Device, queue: &wgpu::Queue, limit: u64) {
    assert_eq!(device.limits().max_buffer_size, limit);
    let authority = SubmissionAuthority::new(queue.clone());
    let target = destination(device, 16);
    authority
        .prepare(device, |encoder, _| {
            let mut context = PrepareContext::new(device, encoder)?;
            context.upload_buffer(&target, 8, &[1, 2, 3, 4]);
            context.finish();
            Ok::<_, crate::FrameError>(())
        })
        .unwrap()
        .submit()
        .unwrap();
    assert_eq!(
        read(device, queue, &target),
        [0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4, 0, 0, 0, 0]
    );
    let statistics = authority.upload_staging();
    assert_eq!(statistics.chunks_created, 1);
    assert!(
        statistics.bytes_created <= limit,
        "a {limit}-byte device got a {}-byte chunk",
        statistics.bytes_created
    );
}

/// Two frames in flight, each uploading doubling sizes from 4 bytes up to the largest buffer the
/// device allows, each into its own destination, so the pool grows by several chunks. Creating
/// any chunk past the limit is a validation error; every upload must land whole.
fn growing_uploads(device: &wgpu::Device, queue: &wgpu::Queue, limit: u64) {
    let authority = SubmissionAuthority::new(queue.clone());
    let largest = limit / 4 * 4;
    let mut sizes: Vec<u64> = std::iter::successors(Some(4_u64), |size| Some(size * 2))
        .take_while(|&size| size < largest)
        .collect();
    sizes.push(largest);
    let frames: Vec<Vec<wgpu::Buffer>> = (0..2_u32)
        .map(|frame| {
            let targets: Vec<wgpu::Buffer> = sizes
                .iter()
                .map(|&size| destination(device, size))
                .collect();
            authority
                .prepare(device, |encoder, _| {
                    let mut context = PrepareContext::new(device, encoder)?;
                    for (index, (target, &size)) in targets.iter().zip(&sizes).enumerate() {
                        let index = u32::try_from(index).unwrap();
                        context.upload_buffer(target, 0, &pattern(frame, index, len(size)));
                    }
                    context.finish();
                    Ok::<_, crate::FrameError>(())
                })
                .unwrap()
                .submit()
                .unwrap();
            targets
        })
        .collect();
    for (frame, targets) in (0..2_u32).zip(&frames) {
        for (index, (target, &size)) in targets.iter().zip(&sizes).enumerate() {
            let index = u32::try_from(index).unwrap();
            assert_eq!(
                read(device, queue, target),
                pattern(frame, index, len(size)),
                "frame {frame}, upload of {size} bytes on a {limit}-byte device"
            );
        }
    }
    let statistics = authority.upload_staging();
    assert_eq!(statistics.uploads, 2 * sizes.len() as u64);
    assert!(statistics.chunks_created >= 4, "{statistics:?}");
    assert!(
        statistics.bytes_created <= statistics.chunks_created * limit,
        "{statistics:?}"
    );
}

/// Staging one upload larger than any buffer the device can create fails fast, naming the sizes.
fn oversized_upload(device: &wgpu::Device, queue: &wgpu::Queue, limit: u64) {
    let authority = SubmissionAuthority::new(queue.clone());
    let bytes = vec![0; len(limit / 4 * 4 + 4)];
    let _ = authority.prepare(device, |_, publications| {
        publications.stage_upload(device, &bytes);
        Ok::<_, ()>(())
    });
}

/// Stages an upload through the publications a capture callback was handed and copies it into
/// `target` at `offset`.
fn stage(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    publications: &Publications,
    (target, offset): (&wgpu::Buffer, u64),
    bytes: &[u8],
) {
    let (source, at) = publications.stage_upload(device, bytes);
    encoder.copy_buffer_to_buffer(&source, at, target, offset, bytes.len() as u64);
}

#[derive(Clone, Copy)]
enum Segment {
    Prefix,
    Suffix,
}

/// A capture whose prefix or suffix callback stages an upload through its own publications.
fn capture_upload(device: &wgpu::Device, queue: &wgpu::Queue, segment: Segment) {
    let authority = SubmissionAuthority::new(queue.clone());
    let target = destination(device, 16);
    let writes = CaptureWrites::new([WriteKey::Resident(1)], [WriteKey::Resident(2)], []).unwrap();
    let prefix = authority
        .begin_capture(device, writes, (), |encoder, publications, ()| {
            if matches!(segment, Segment::Prefix) {
                stage(device, encoder, publications, (&target, 0), &[1; 8]);
            }
            Ok::<_, ()>(())
        })
        .unwrap();
    let suffix = prefix
        .submit()
        .unwrap()
        .resume(
            device,
            |_, _| Ok(()),
            |encoder, publications, (), ()| {
                if matches!(segment, Segment::Suffix) {
                    stage(device, encoder, publications, (&target, 8), &[2; 8]);
                }
                Ok::<_, ()>(())
            },
        )
        .unwrap();
    suffix
        .submit()
        .unwrap()
        .finish(device, |_, _| Ok::<_, ()>(()))
        .unwrap();
    let expected = match segment {
        Segment::Prefix => [[1; 8], [0; 8]].concat(),
        Segment::Suffix => [[0; 8], [2; 8]].concat(),
    };
    assert_eq!(read(device, queue, &target), expected);
    assert_eq!(authority.upload_staging().uploads, 1);
}

#[test]
fn a_four_byte_upload_fits_devices_whose_buffers_are_smaller_than_a_chunk() {
    for limit in LIMITS {
        let (device, queue) = noop(limit);
        small_upload(&device, &queue, limit);
    }
}

#[test]
fn growing_uploads_never_create_staging_past_the_device_buffer_limit() {
    for limit in LIMITS {
        let (device, queue) = noop(limit);
        growing_uploads(&device, &queue, limit);
    }
}

#[test]
#[should_panic(expected = "exceeds the device's max_buffer_size of 196612 bytes")]
fn an_upload_larger_than_the_device_buffer_limit_fails_fast() {
    let (device, queue) = noop(LIMITS[1]);
    oversized_upload(&device, &queue, LIMITS[1]);
}

#[test]
fn a_capture_prefix_callback_stages_uploads_through_its_publications() {
    let (device, queue) = noop(wgpu::Limits::default().max_buffer_size);
    capture_upload(&device, &queue, Segment::Prefix);
}

#[test]
fn a_capture_suffix_callback_stages_uploads_through_its_publications() {
    let (device, queue) = noop(wgpu::Limits::default().max_buffer_size);
    capture_upload(&device, &queue, Segment::Suffix);
}

#[test]
fn gpu_a_four_byte_upload_fits_devices_whose_buffers_are_smaller_than_a_chunk() {
    for limit in LIMITS {
        let (device, queue) = gpu(limit);
        small_upload(&device, &queue, limit);
    }
}

#[test]
fn gpu_growing_uploads_never_create_staging_past_the_device_buffer_limit() {
    for limit in LIMITS {
        let (device, queue) = gpu(limit);
        growing_uploads(&device, &queue, limit);
    }
}

#[test]
#[should_panic(expected = "exceeds the device's max_buffer_size of 131072 bytes")]
fn gpu_an_upload_larger_than_the_device_buffer_limit_fails_fast() {
    let (device, queue) = gpu(LIMITS[0]);
    oversized_upload(&device, &queue, LIMITS[0]);
}

#[test]
fn gpu_capture_callbacks_stage_uploads_through_their_publications() {
    let (device, queue) = crate::gpu_tests::gpu();
    capture_upload(&device, &queue, Segment::Prefix);
    capture_upload(&device, &queue, Segment::Suffix);
}
