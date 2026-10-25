use super::residency_tests::{context, settings};
use super::{composition::*, mip_gpu::PageWriter, residency::*, selection::PageKey};
use worldrunner_render_gpu::{configuration::AllocationBudget, submission::SubmissionAuthority};

#[test]
fn terrain_pages_failed_submission_and_pending_edits_publish_nothing() {
    for fail_submission in [false, true] {
        let context = context();
        let device = context.device();
        let authority = SubmissionAuthority::new(context.queue().clone());
        let settings = settings();
        let cache =
            ResidentPages::new(device, &authority, settings, 2, AllocationBudget(100_000)).unwrap();
        let writer = PageWriter::new(device, settings);
        let base = MaterialImage::new(
            settings.extent(),
            vec![
                MaterialTexel {
                    color: [127; 4],
                    normal_roughness: [128, 128, 255, 100],
                };
                35
            ],
        )
        .unwrap();
        let mut frame = authority.begin(device).unwrap();
        let pending = cache.reserve(&frame, PageKey::ROOT).unwrap();
        writer
            .encode(device, &mut frame, &cache, pending, &base)
            .unwrap();
        if fail_submission {
            authority.report_device_loss("injected submission rejection".into());
            assert!(frame.finish().submit().is_err());
        } else {
            cache.invalidate([0.0, 0.0], [1.0, 1.0]);
            frame.finish().submit().unwrap();
        }
        assert!(cache.lookup(PageKey::ROOT, 0).is_none());
    }
}

#[test]
fn terrain_pages_invalidated_reservation_is_rejected_before_encoding() {
    let context = context();
    let device = context.device();
    let authority = SubmissionAuthority::new(context.queue().clone());
    let settings = settings();
    let cache =
        ResidentPages::new(device, &authority, settings, 2, AllocationBudget(100_000)).unwrap();
    let writer = PageWriter::new(device, settings);
    let base = MaterialImage::new(
        settings.extent(),
        vec![
            MaterialTexel {
                color: [127; 4],
                normal_roughness: [128, 128, 255, 100],
            };
            35
        ],
    )
    .unwrap();
    let mut frame = authority.begin(device).unwrap();
    let pending = cache.reserve(&frame, PageKey::ROOT).unwrap();
    cache.evict(PageKey::ROOT);
    assert!(matches!(
        writer.encode(device, &mut frame, &cache, pending, &base),
        Err(super::mip_gpu::PageEncodeError::Cache(
            PageCacheError::Address
        ))
    ));
    frame.finish().submit().unwrap();
    assert!(cache.lookup(PageKey::ROOT, 0).is_none());
}
