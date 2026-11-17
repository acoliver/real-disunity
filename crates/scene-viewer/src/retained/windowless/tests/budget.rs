use super::{Driver, host};

#[test]
fn windowless_derived_budget_is_observable_and_tracks_resize() {
    let mut driver = Driver::default();
    let mut host = host(crate::renderer::retained_tests::source());
    host.options.presentation_budget = crate::PresentationBudgetPolicy::Derived {
        headroom_bytes: 4096,
        granularity_bytes: 256,
        cap_bytes: None,
    };
    host.options.window_size_unit = crate::WindowSizeUnit::Physical;
    let mut run = host.windowless(&mut driver).unwrap();
    assert!(run.frame().unwrap());
    let first = run.host().statistics().unwrap().presentation_budget_bytes;
    assert!(first > 256 * 1024 * 1024);
    assert_eq!(first % 256, 0);
    run.resize([128, 96]).unwrap();
    assert!(run.frame().unwrap());
    let next = run.host().statistics().unwrap().presentation_budget_bytes;
    assert!(next > first);
    assert_eq!(next % 256, 0);
    run.shutdown().unwrap();
}

#[test]
fn windowless_statistics_report_resident_bytes_by_class() {
    use crate::ResourceClass;
    let mut driver = Driver::default();
    let host = host(crate::renderer::retained_tests::source());
    let mut run = host.windowless(&mut driver).unwrap();
    assert!(run.frame().unwrap());
    let resident = run.host().statistics().unwrap().resident_bytes;
    // The scene's placements are drawn from instance buffers, which the census counts.
    assert!(resident.get(ResourceClass::Instance) > 0, "{resident}");
    assert_eq!(
        resident.total(),
        resident.classes().map(|(_, bytes)| bytes).sum::<u64>()
    );
    run.shutdown().unwrap();
}

#[test]
fn windowless_steady_frames_reuse_upload_staging() {
    let mut driver = Driver::default();
    let host = host(crate::renderer::retained_tests::source());
    let mut run = host.windowless(&mut driver).unwrap();
    let mut staging = Vec::new();
    // A frame is skipped while every flight slot is busy; polling lets earlier frames retire.
    let deadline = std::time::Instant::now() + std::time::Duration::from_mins(1);
    while staging.len() < 24 {
        assert!(std::time::Instant::now() < deadline, "{staging:?}");
        if run.frame().unwrap() {
            staging.push(run.host().statistics().unwrap().upload_staging);
        } else {
            run.poll().unwrap();
        }
    }
    // Every frame uploads at least its frame uniforms through the staging.
    assert!(
        staging
            .windows(2)
            .all(|pair| pair[1].uploads > pair[0].uploads),
        "{staging:?}"
    );
    // Admission bounds the frames in flight, so the pool settles and stops creating chunks.
    assert!(
        staging[8..]
            .iter()
            .all(|stats| stats.chunks_created == staging[8].chunks_created),
        "{staging:?}"
    );
    let resident = run.host().statistics().unwrap().resident_bytes;
    assert_eq!(
        resident.get(crate::ResourceClass::Staging),
        staging[23].bytes,
        "{resident}"
    );
    run.shutdown().unwrap();
}
