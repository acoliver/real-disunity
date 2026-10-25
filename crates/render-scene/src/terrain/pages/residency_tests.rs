use super::{residency::*, selection::PageKey};
use worldrunner_render_gpu::{configuration::*, submission::SubmissionAuthority};
use worldrunner_render_plan::{TerrainPageSettings, TerrainPageSettingsParts};

pub(super) fn settings() -> TerrainPageSettings {
    TerrainPageSettings::new(TerrainPageSettingsParts {
        interior: [3, 5],
        border: 1,
        filter_radius: 1,
        maximum_depth: 3,
        split_threshold_millipixels: 2000,
        merge_threshold_millipixels: 1000,
        maximum_selected_pages: 16,
        resident_bytes: 100_000,
        upload_bytes_per_frame: 336,
    })
    .unwrap()
}

pub(super) fn context() -> crate::device::GpuContext {
    pollster::block_on(crate::device::GpuContext::request_with_features(
        wgpu::Features::empty(),
    ))
    .unwrap()
}

#[test]
fn terrain_pages_publication_cancellation_invalidation_eviction_and_limits() {
    let context = context();
    let device = context.device();
    let authority = SubmissionAuthority::new(context.queue().clone());
    let cache =
        ResidentPages::new(device, &authority, settings(), 3, AllocationBudget(100_000)).unwrap();
    let root = PageKey::ROOT;
    let child = PageKey::new(2, [1, 2]).unwrap();
    let mut frame = authority.begin(device).unwrap();
    let pending = cache.reserve(&frame, root).unwrap();
    assert!(matches!(
        cache.reserve(&frame, child),
        Err(PageCacheError::UploadLimit)
    ));
    pending.stage(&frame).unwrap();
    assert!(cache.lookup(child, 2).is_none());
    frame.finish().cancel();
    assert!(cache.lookup(child, 2).is_none());
    frame = authority.begin(device).unwrap();
    cache.reserve(&frame, root).unwrap().stage(&frame).unwrap();
    let submitted = frame.finish().submit().unwrap();
    device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submitted),
            timeout: None,
        })
        .unwrap();
    let old = cache.lookup(child, 2).unwrap();
    assert_eq!(old.key(), root);
    assert!(cache.lookup(child, 3).is_none());
    assert!(cache.is_current(old));
    cache.invalidate([0.1, 0.6], [0.2, 0.7]);
    assert!(!cache.is_current(old));
    assert!(cache.lookup(child, 0).is_none());
    frame = authority.begin(device).unwrap();
    cache.reserve(&frame, child).unwrap().stage(&frame).unwrap();
    cache.invalidate([0.25, 0.5], [0.5, 0.75]);
    let submitted = frame.finish().submit().unwrap();
    device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submitted),
            timeout: None,
        })
        .unwrap();
    assert!(cache.lookup(child, 0).is_none());
    frame = authority.begin(device).unwrap();
    cache.reserve(&frame, child).unwrap().stage(&frame).unwrap();
    let submitted = frame.finish().submit().unwrap();
    device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submitted),
            timeout: None,
        })
        .unwrap();
    let resident = cache.lookup(child, 0).unwrap();
    cache.evict(child);
    assert!(!cache.is_current(resident));
    assert!(cache.lookup(child, 0).is_none());
}

#[test]
fn terrain_pages_replacement_peak_is_admitted_before_allocation() {
    let context = context();
    let device = context.device();
    let authority = SubmissionAuthority::new(context.queue().clone());
    let mut cache =
        ResidentPages::new(device, &authority, settings(), 2, AllocationBudget(2000)).unwrap();
    let before = cache.allocation_bytes(device).unwrap();
    assert_eq!(before, 1232);
    assert!(matches!(
        cache.replace(device, 3),
        Err(ConfigurationError::Budget {
            required: 2800,
            budget: 2000
        })
    ));
    assert_eq!(cache.allocation_bytes(device).unwrap(), before);
}
