//! `PrepareContext::upload_buffer` staging on wgpu's no-op device, which executes copies on the
//! CPU at submission. Shared with the GPU tests through [`frames`].
use crate::{BufferReadback, PrepareContext};
use worldrunner_render_gpu::submission::SubmissionAuthority;

pub(crate) fn destination(device: &wgpu::Device, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("upload staging destination"),
        size,
        usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

pub(crate) fn read(device: &wgpu::Device, queue: &wgpu::Queue, source: &wgpu::Buffer) -> Vec<u8> {
    let mut capture = BufferReadback::new(device, source.size(), source.size()).unwrap();
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    capture.encode(&mut encoder, source).unwrap();
    let submission = queue.submit([encoder.finish()]);
    capture.read_encoded(device, Some(submission)).unwrap()
}

/// The bytes frame `frame` uploads at `index`: distinct per frame and position.
pub(crate) fn pattern(frame: u32, index: u32, len: usize) -> Vec<u8> {
    (0..len)
        .map(|byte| {
            (frame.wrapping_mul(31) ^ index.wrapping_mul(7) ^ u32::try_from(byte).unwrap())
                .to_le_bytes()[0]
        })
        .collect()
}

/// Records `frames` frames that each upload `uploads` scattered 64-byte records into
/// `destination`, keeping up to `in_flight` frames submitted before waiting for the oldest.
/// Returns the staging chunks created after each frame.
pub(crate) fn frames(
    device: &wgpu::Device,
    authority: &SubmissionAuthority,
    destination: &wgpu::Buffer,
    (frames, uploads, in_flight): (u32, u32, usize),
) -> Vec<u64> {
    let mut pending = std::collections::VecDeque::new();
    let mut created = Vec::new();
    for frame in 0..frames {
        let prepared = authority
            .prepare(device, |encoder, _| {
                let mut context = PrepareContext::new(device, encoder)?;
                for index in 0..uploads {
                    let offset = u64::from((index * 7 + frame) % uploads) * 64;
                    context.upload_buffer(destination, offset, &pattern(frame, index, 64));
                }
                context.finish();
                Ok::<_, crate::FrameError>(())
            })
            .unwrap();
        pending.push_back(prepared.submit().unwrap());
        if pending.len() >= in_flight {
            device
                .poll(wgpu::PollType::Wait {
                    submission_index: pending.pop_front(),
                    timeout: Some(std::time::Duration::from_secs(30)),
                })
                .unwrap();
        }
        created.push(authority.upload_staging().chunks_created);
    }
    created
}

/// What [`frames`] leaves in `destination` after its last frame.
pub(crate) fn expected(frames: u32, uploads: u32) -> Vec<u8> {
    let mut bytes = vec![0; usize::try_from(uploads).unwrap() * 64];
    for frame in 0..frames {
        for index in 0..uploads {
            let offset = usize::try_from((index * 7 + frame) % uploads).unwrap() * 64;
            bytes[offset..offset + 64].copy_from_slice(&pattern(frame, index, 64));
        }
    }
    bytes
}

pub(crate) fn noop() -> (wgpu::Device, wgpu::Queue, SubmissionAuthority) {
    let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let authority = SubmissionAuthority::new(queue.clone());
    (device, queue, authority)
}

#[test]
fn steady_frames_reuse_staging_once_the_device_has_finished_with_it() {
    let (device, queue, authority) = noop();
    let target = destination(&device, 40 * 64);
    let created = frames(&device, &authority, &target, (24, 40, 3));
    assert_eq!(read(&device, &queue, &target), expected(24, 40));
    // At most two frames stay in flight while the next records, so no more than three chunks,
    // and none created once the first frames have settled.
    assert!(created[23] <= 3, "{created:?}");
    assert!(
        created[4..].iter().all(|&count| count == created[23]),
        "{created:?}"
    );
    let statistics = authority.upload_staging();
    assert_eq!(statistics.uploads, 24 * 40);
    assert_eq!(statistics.uploaded_bytes, 24 * 40 * 64);
    assert_eq!(statistics.chunks, created[23]);
}

#[test]
fn a_cancelled_frame_never_copies_its_staged_bytes_and_frees_its_staging() {
    let (device, queue, authority) = noop();
    let target = destination(&device, 64);
    frames(&device, &authority, &target, (1, 1, 1));
    let before = read(&device, &queue, &target);
    for cancel in [true, false] {
        let prepared = authority
            .prepare(&device, |encoder, _| {
                let mut context = PrepareContext::new(&device, encoder)?;
                context.upload_buffer(&target, 0, &[0xAB; 64]);
                context.finish();
                Ok::<_, crate::FrameError>(())
            })
            .unwrap();
        if cancel {
            prepared.cancel();
        } else {
            drop(prepared);
        }
        assert_eq!(read(&device, &queue, &target), before);
    }
    let failed = authority.prepare(&device, |encoder, _| {
        let mut context = PrepareContext::new(&device, encoder)?;
        context.upload_buffer(&target, 0, &[0xCD; 64]);
        context.finish();
        Err(crate::FrameError::StockOrder {
            pass: crate::PassEntry::new(crate::PassSlot::ReflectionTrace),
            needs: crate::PassEntry::new(crate::PassSlot::Opaque),
        })
    });
    assert!(failed.is_err());
    assert_eq!(read(&device, &queue, &target), before);
    // The cancelled staging was reused, and the next submitted frame still lands its bytes.
    frames(&device, &authority, &target, (1, 1, 1));
    assert_eq!(authority.upload_staging().chunks_created, 1);
    assert_eq!(read(&device, &queue, &target), pattern(0, 0, 64));
}

#[test]
fn an_upload_larger_than_a_chunk_lands_whole_and_is_reused() {
    let (device, queue, authority) = noop();
    let len = 600 * 1024;
    let target = destination(&device, len);
    for frame in 0..3 {
        let bytes = pattern(frame, 9, usize::try_from(len).unwrap());
        authority
            .prepare(&device, |encoder, _| {
                let mut context = PrepareContext::new(&device, encoder)?;
                context.upload_buffer(&target, 0, &bytes);
                context.finish();
                Ok::<_, crate::FrameError>(())
            })
            .unwrap()
            .submit()
            .unwrap();
        assert_eq!(read(&device, &queue, &target), bytes);
    }
    let statistics = authority.upload_staging();
    assert_eq!(statistics.chunks_created, 1);
    assert_eq!(statistics.bytes_created, 1024 * 1024);
}

#[test]
fn an_empty_upload_stages_nothing() {
    let (device, _queue, authority) = noop();
    let target = destination(&device, 16);
    authority
        .prepare(&device, |encoder, _| {
            let mut context = PrepareContext::new(&device, encoder)?;
            context.upload_buffer(&target, 16, &[]);
            context.finish();
            Ok::<_, crate::FrameError>(())
        })
        .unwrap()
        .submit()
        .unwrap();
    assert_eq!(authority.upload_staging().uploads, 0);
}

#[test]
#[should_panic(expected = "does not fit")]
fn an_upload_past_the_destination_fails_fast() {
    let (device, _queue, authority) = noop();
    let target = destination(&device, 16);
    let _ = authority.prepare(&device, |encoder, _| {
        let mut context = PrepareContext::new(&device, encoder)?;
        context.upload_buffer(&target, 8, &[0; 16]);
        Ok::<_, crate::FrameError>(())
    });
}
