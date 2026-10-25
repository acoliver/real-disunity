use super::{
    residency_tests::{context, settings},
    selection::PageKey,
};
use worldrunner_render_gpu::{configuration::AllocationBudget, submission::SubmissionAuthority};

#[test]
fn terrain_pages_camera_selection_changes_lod_and_capture_requires_exact_pages() {
    let settings = settings();
    let near = super::view::select(
        settings,
        None,
        glam::Mat4::IDENTITY,
        [128, 128],
        [0.0, 0.0],
        [8.0, 4.0],
    );
    let far = super::view::select(
        settings,
        Some(&near),
        glam::Mat4::from_scale(glam::Vec3::splat(0.0001)),
        [128, 128],
        [0.0, 0.0],
        [8.0, 4.0],
    );
    assert!(near.leaves().len() > far.leaves().len());
    assert_eq!(far.leaves(), &[PageKey::ROOT]);
    let context = context();
    let authority = SubmissionAuthority::new(context.queue().clone());
    let cache = super::residency::ResidentPages::new(
        context.device(),
        &authority,
        settings,
        2,
        AllocationBudget(100_000),
    )
    .unwrap();
    assert!(!cache.ready(&far));
    let frame = authority.begin(context.device()).unwrap();
    cache
        .reserve(&frame, PageKey::ROOT)
        .unwrap()
        .stage(&frame)
        .unwrap();
    assert!(!cache.ready(&far));
    frame.finish().submit().unwrap();
    assert!(cache.ready(&far));
    assert!(!cache.ready(&near));
    cache.invalidate([0.0, 0.0], [1.0, 1.0]);
    assert!(!cache.ready(&far));
}

#[test]
fn terrain_pages_automatic_eviction_protects_root_and_selected_pages() {
    let context = context();
    let authority = SubmissionAuthority::new(context.queue().clone());
    let cache = super::residency::ResidentPages::new(
        context.device(),
        &authority,
        settings(),
        2,
        AllocationBudget(100_000),
    )
    .unwrap();
    let child = PageKey::new(1, [0, 0]).unwrap();
    for key in [PageKey::ROOT, child] {
        let frame = authority.begin(context.device()).unwrap();
        cache.reserve(&frame, key).unwrap().stage(&frame).unwrap();
        let submission = frame.finish().submit().unwrap();
        context
            .device()
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: None,
            })
            .unwrap();
    }
    let old = cache.lookup(child, 0).unwrap();
    assert_eq!(cache.evict_unselected(&[PageKey::ROOT]), Some(child));
    assert!(!cache.is_current(old));
    assert!(cache.lookup(PageKey::ROOT, 0).is_some());
    assert_eq!(cache.evict_unselected(&[PageKey::ROOT]), None);
}
