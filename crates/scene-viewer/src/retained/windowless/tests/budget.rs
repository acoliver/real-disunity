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
    let mut run = host.windowless(&mut driver, None).unwrap();
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
    let mut run = host.windowless(&mut driver, None).unwrap();
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
    let mut run = host.windowless(&mut driver, None).unwrap();
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
    // Admission bounds the frames in flight, but a submitted chunk is free again only once its
    // mapping callback has run, which depends on when the device is polled, so whether and
    // when the in-flight frames create more chunks is timing. Once the device is idle every
    // chunk's mapping has completed, and a frame that starts on an idle device creates none.
    let in_flight = staging[23].chunks_created;
    let mut settled = Vec::new();
    while settled.len() < 8 {
        assert!(std::time::Instant::now() < deadline, "{settled:?}");
        run.renderer.drain().unwrap();
        if run.frame().unwrap() {
            settled.push(run.host().statistics().unwrap().upload_staging);
        } else {
            run.poll().unwrap();
        }
    }
    assert!(
        settled
            .iter()
            .all(|stats| stats.chunks_created == in_flight),
        "in flight {staging:?}, settled {settled:?}"
    );
    let resident = run.host().statistics().unwrap().resident_bytes;
    assert_eq!(
        resident.get(crate::ResourceClass::Staging),
        settled[7].bytes,
        "{resident}"
    );
    run.shutdown().unwrap();
}
