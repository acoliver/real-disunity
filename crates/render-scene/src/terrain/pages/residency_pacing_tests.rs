//! Page bakes are spread over the frames that share the upload scratch. wgpu's no-op device
//! completes every submission at once but reports it only at the next poll or submission, so
//! polling before or after a frame's reservations decides whether the previous frame is still
//! in flight.

use super::{
    residency::{PageCacheError, ResidentPages},
    residency_tests::settings,
    selection::PageKey,
};
use worldrunner_render_gpu::{configuration::AllocationBudget, submission::SubmissionAuthority};
use worldrunner_render_plan::TerrainPageSettings;

const FRAMES: usize = 24;

/// A cache with two whole-page uploads per frame.
fn two_page_cache(device: &wgpu::Device, authority: &SubmissionAuthority) -> ResidentPages {
    let mut parts = settings().parts();
    parts.upload_bytes_per_frame = settings().page_bytes() * 2;
    let settings = TerrainPageSettings::new(parts).unwrap();
    ResidentPages::new(device, authority, settings, 64, AllocationBudget(100_000)).unwrap()
}

/// Reserves and stages pages for `keys` in `frame` until the cache refuses one, and returns
/// how many it admitted.
fn admit(cache: &ResidentPages, frame: &wgpu::CommandEncoder, keys: &[PageKey]) -> usize {
    let mut admitted = 0;
    for &key in keys {
        match cache.reserve(frame, key) {
            Ok(pending) => pending.stage(frame).unwrap(),
            Err(PageCacheError::UploadLimit | PageCacheError::ScratchFull) => break,
            Err(error) => panic!("reserve {key:?}: {error}"),
        }
        admitted += 1;
    }
    admitted
}

fn keys(count: u32) -> Vec<PageKey> {
    (0..count)
        .map(|x| PageKey::new(3, [x, 0]).unwrap())
        .collect()
}

/// Bakes pages over [`FRAMES`] frames with two whole-page uploads per frame and returns how
/// many each frame admitted. `overlap` reports the previous frame's completion only after this
/// frame's reservations, as a host with two frames in flight sees it while the GPU is busy.
fn pages_per_frame(overlap: bool) -> Vec<usize> {
    let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let authority = SubmissionAuthority::new(queue);
    let cache = two_page_cache(&device, &authority);
    let mut keys = (0..8).flat_map(|y| (0..8).map(move |x| PageKey::new(3, [x, y]).unwrap()));
    let mut next = keys.next();
    let mut counts = Vec::new();
    for _ in 0..FRAMES {
        if !overlap {
            device.poll(wgpu::PollType::Poll).unwrap();
        }
        let frame = authority.begin(&device).unwrap();
        let mut admitted = 0;
        while let Some(key) = next {
            match cache.reserve(&frame, key) {
                Ok(pending) => pending.stage(&frame).unwrap(),
                Err(PageCacheError::UploadLimit | PageCacheError::ScratchFull) => break,
                Err(error) => panic!("reserve {key:?}: {error}"),
            }
            admitted += 1;
            next = keys.next();
        }
        if overlap {
            device.poll(wgpu::PollType::Poll).unwrap();
        }
        frame.finish().submit().unwrap();
        counts.push(admitted);
    }
    counts
}

#[test]
fn page_bakes_spread_over_frames_in_flight() {
    let counts = pages_per_frame(true);
    // Uploads admitted in a frame hold the scratch until that frame completes, two frames
    // later here. Two pages every other frame would double the slow frames' bake work.
    assert_eq!(&counts[..2], [2, 0], "pages per frame {counts:?}");
    assert!(
        counts[2..].iter().all(|&count| count == 1),
        "pages per frame {counts:?}"
    );
    assert_eq!(counts.iter().sum::<usize>(), FRAMES, "same page throughput");
}

#[test]
fn page_bakes_use_the_whole_allowance_when_the_gpu_keeps_up() {
    let counts = pages_per_frame(false);
    assert!(
        counts.iter().all(|&count| count == 2),
        "pages per frame {counts:?}"
    );
}

#[test]
fn an_abandoned_frame_leaves_the_next_frame_the_whole_allowance() {
    let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let authority = SubmissionAuthority::new(queue);
    let cache = two_page_cache(&device, &authority);
    let abandoned = authority.begin(&device).unwrap();
    assert_eq!(admit(&cache, &abandoned, &keys(2)), 2);
    drop(abandoned.finish());
    assert_eq!(cache.frames_in_flight(), 0);
    let frame = authority.begin(&device).unwrap();
    assert_eq!(admit(&cache, &frame, &keys(2)), 2);
    frame.finish().submit().unwrap();
    assert_eq!(cache.frames_in_flight(), 1);
}

#[test]
fn work_that_prepares_no_pages_does_not_take_a_frames_share() {
    // Scene upload submits its instance flush as a prepared transaction, and a host draws its
    // first frame without polling. That work bakes no pages, so it is not a frame of the cache.
    let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let authority = SubmissionAuthority::new(queue);
    let cache = two_page_cache(&device, &authority);
    authority.begin(&device).unwrap().finish().submit().unwrap();
    assert_eq!(cache.frames_in_flight(), 0);
    let frame = authority.begin(&device).unwrap();
    assert_eq!(admit(&cache, &frame, &keys(2)), 2);
    frame.finish().submit().unwrap();
}
