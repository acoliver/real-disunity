use super::{
    residency::*,
    residency_tests::{context, settings},
    selection::PageKey,
};
use worldrunner_render_gpu::{configuration::AllocationBudget, submission::SubmissionAuthority};

#[test]
fn terrain_pages_cancelled_unencoded_handles_cannot_hold_or_clear_reused_slots() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let device = context.device();
    let authority = SubmissionAuthority::new(context.queue().clone());
    let cache =
        ResidentPages::new(device, &authority, settings(), 2, AllocationBudget(100_000)).unwrap();
    let frame = authority.begin(device).unwrap();
    let obsolete = cache.reserve(&frame, PageKey::ROOT).unwrap();
    frame.finish().cancel();
    let frame = authority.begin(device).unwrap();
    let replacement = cache.reserve(&frame, PageKey::ROOT).unwrap();
    drop(obsolete);
    replacement.stage(&frame).unwrap();
    frame.finish().submit().unwrap();
    assert!(cache.lookup(PageKey::ROOT, 0).is_some());
}

#[test]
fn terrain_pages_scratch_admission_includes_unretired_submissions() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let device = context.device();
    let authority = SubmissionAuthority::new(context.queue().clone());
    let cache =
        ResidentPages::new(device, &authority, settings(), 3, AllocationBudget(100_000)).unwrap();
    let frame = authority.begin(device).unwrap();
    cache
        .reserve(&frame, PageKey::ROOT)
        .unwrap()
        .stage(&frame)
        .unwrap();
    let submission = frame.finish().submit().unwrap();
    cache.evict(PageKey::ROOT);
    let frame = authority.begin(device).unwrap();
    assert!(matches!(
        cache.reserve(&frame, PageKey::ROOT),
        Err(PageCacheError::ScratchFull)
    ));
    frame.finish().cancel();
    device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: None,
        })
        .unwrap();
    let frame = authority.begin(device).unwrap();
    assert!(cache.reserve(&frame, PageKey::ROOT).is_ok());
    frame.finish().cancel();
}

#[test]
fn terrain_pages_foreign_authority_cannot_reserve() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let device = context.device();
    let owner = SubmissionAuthority::new(context.queue().clone());
    let foreign = SubmissionAuthority::new(context.queue().clone());
    let cache =
        ResidentPages::new(device, &owner, settings(), 2, AllocationBudget(100_000)).unwrap();
    let frame = foreign.begin(device).unwrap();
    assert!(matches!(
        cache.reserve(&frame, PageKey::ROOT),
        Err(PageCacheError::Submission(_))
    ));
    frame.finish().cancel();
}

#[test]
fn terrain_page_arrays_count_in_the_census_until_the_cache_drops() {
    use worldrunner_render_gpu::census::{ResourceClass, texture_backing_bytes};
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let device = context.device();
    // A private authority, so no other test's pages enter its tally.
    let authority = SubmissionAuthority::new(context.queue().clone());
    let cache =
        ResidentPages::new(device, &authority, settings(), 2, AllocationBudget(100_000)).unwrap();
    let texture = cache.texture();
    let expected = texture_backing_bytes(&wgpu::TextureDescriptor {
        label: None,
        size: texture.size(),
        mip_level_count: texture.mip_level_count(),
        sample_count: texture.sample_count(),
        dimension: texture.dimension(),
        format: texture.format(),
        usage: texture.usage(),
        view_formats: &[],
    });
    assert!(expected > 0);
    assert_eq!(
        authority
            .census()
            .snapshot()
            .get(ResourceClass::TerrainPage),
        expected
    );
    drop((texture, cache));
    assert_eq!(
        authority
            .census()
            .snapshot()
            .get(ResourceClass::TerrainPage),
        0
    );
}
