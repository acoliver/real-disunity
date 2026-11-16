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
