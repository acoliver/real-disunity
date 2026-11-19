use super::retained_tests::source;
use super::startup::{reduce_backend_census, resolve_retained};
use crate::{
    AdapterClass, AdapterIdentity, AdapterMemory, DisplayInfo, InitialContent, PresentationCadence,
    RetainedHost, ViewerError, ViewerOptions,
};
use std::sync::{Arc, Mutex};
use wgpu::{Backend, Backends};

#[derive(Debug, PartialEq)]
struct Observed {
    adapter: AdapterIdentity,
    surface_extent: [u32; 2],
    display: Option<DisplayInfo>,
    census: Option<Vec<Backend>>,
}

fn identity(backend: Backend) -> AdapterIdentity {
    AdapterIdentity {
        name: "selected".into(),
        class: AdapterClass::Integrated,
        backend,
        memory: None,
    }
}

fn supply(
    adapter: &AdapterIdentity,
    surface_extent: [u32; 2],
    display: Option<DisplayInfo>,
    census: Option<&[Backend]>,
) -> Vec<Observed> {
    let options = ViewerOptions {
        cadence: PresentationCadence::Uncapped,
        ..crate::tests::options()
    };
    let log = Arc::new(Mutex::new(Vec::new()));
    let record = Arc::clone(&log);
    let mut retained = RetainedHost::with_initial_content(options, move |context| {
        record.lock().unwrap().push(Observed {
            adapter: context.adapter.clone(),
            surface_extent: context.surface_extent,
            display: context.display,
            census: context.presentable_backends.map(<[_]>::to_vec),
        });
        Ok(InitialContent {
            scene: source(),
            settings: None,
        })
    })
    .unwrap();
    resolve_retained(&mut retained, adapter, surface_extent, display, census).unwrap();
    let mut guard = log.lock().unwrap();
    std::mem::take(&mut *guard)
}

#[test]
fn retained_supplier_receives_ordered_presentability_for_the_selected_adapter() {
    let adapter = identity(Backend::Metal);
    let census = [Backend::Vulkan, Backend::Metal];
    assert_eq!(
        supply(&adapter, [64, 32], None, Some(&census)),
        vec![Observed {
            adapter,
            surface_extent: [64, 32],
            display: None,
            census: Some(vec![Backend::Vulkan, Backend::Metal]),
        }]
    );
}

#[test]
fn retained_supplier_receives_physical_extent_and_full_identity_with_monitor() {
    let adapter = AdapterIdentity {
        name: "synthetic discrete adapter".into(),
        class: AdapterClass::Discrete,
        backend: Backend::Vulkan,
        memory: Some(AdapterMemory::declared(8 * 1024 * 1024 * 1024)),
    };
    let display = DisplayInfo {
        physical_size: [3840, 2160],
        scale_factor: 1.5,
    };
    let surface_extent = [1200, 900];
    let census = [Backend::Metal, Backend::Vulkan];
    assert_eq!(
        supply(&adapter, surface_extent, Some(display), Some(&census)),
        vec![Observed {
            adapter,
            surface_extent,
            display: Some(display),
            census: Some(vec![Backend::Metal, Backend::Vulkan]),
        }]
    );
}

#[test]
fn unknown_census_stays_none_whichever_backend_was_selected() {
    for selected in [Backend::Vulkan, Backend::Metal, Backend::Gl] {
        let adapter = identity(selected);
        assert_eq!(
            supply(&adapter, [64, 32], None, None),
            vec![Observed {
                adapter,
                surface_extent: [64, 32],
                display: None,
                census: None,
            }]
        );
    }
}

#[test]
fn backend_census_keeps_ordered_distinct_supported_allowed_observations() {
    let reduced = reduce_backend_census(
        Backends::VULKAN | Backends::METAL,
        Backend::Metal,
        [
            (Backend::Vulkan, false),
            (Backend::Metal, true),
            (Backend::Vulkan, true),
            (Backend::Metal, true),
            (Backend::Gl, true),
        ],
    )
    .unwrap();
    assert_eq!(reduced, vec![Backend::Metal, Backend::Vulkan]);
}

#[test]
fn census_follows_first_supported_observation_not_the_selected_backend() {
    let reduced = reduce_backend_census(
        Backends::VULKAN | Backends::METAL,
        Backend::Vulkan,
        [(Backend::Metal, true), (Backend::Vulkan, true)],
    )
    .unwrap();
    assert_eq!(reduced, vec![Backend::Metal, Backend::Vulkan]);
}

#[test]
fn backend_seen_only_as_unsupported_is_omitted() {
    let reduced = reduce_backend_census(
        Backends::VULKAN | Backends::METAL,
        Backend::Vulkan,
        [(Backend::Metal, false), (Backend::Vulkan, true)],
    )
    .unwrap();
    assert_eq!(reduced, vec![Backend::Vulkan]);
}

#[test]
fn enabled_backend_that_was_never_observed_is_omitted() {
    let reduced = reduce_backend_census(
        Backends::VULKAN | Backends::METAL | Backends::GL,
        Backend::Vulkan,
        [(Backend::Vulkan, true)],
    )
    .unwrap();
    assert_eq!(reduced, vec![Backend::Vulkan]);
}

#[test]
fn duplicate_unsupported_then_supported_observation_counts_once() {
    let reduced = reduce_backend_census(
        Backends::VULKAN | Backends::METAL,
        Backend::Metal,
        [
            (Backend::Metal, false),
            (Backend::Vulkan, true),
            (Backend::Metal, true),
            (Backend::Metal, true),
        ],
    )
    .unwrap();
    assert_eq!(reduced, vec![Backend::Vulkan, Backend::Metal]);
}

#[test]
fn empty_or_unsupported_observations_report_selected_backend_missing() {
    let enabled = Backends::VULKAN | Backends::METAL;
    for observations in [vec![], vec![(Backend::Metal, false)]] {
        assert!(matches!(
            reduce_backend_census(enabled, Backend::Metal, observations),
            Err(ViewerError::SelectedBackendMissing {
                backend: Backend::Metal
            })
        ));
    }
}

#[test]
fn selected_backend_missing_from_nonempty_supported_observations_is_an_error() {
    let enabled = Backends::VULKAN | Backends::METAL;
    let observations = [
        vec![(Backend::Vulkan, true)],
        vec![(Backend::Vulkan, true), (Backend::Metal, false)],
    ];
    for observations in observations {
        assert!(matches!(
            reduce_backend_census(enabled, Backend::Metal, observations),
            Err(ViewerError::SelectedBackendMissing {
                backend: Backend::Metal
            })
        ));
    }
}

#[test]
fn selected_backend_outside_the_enabled_mask_is_disallowed_even_when_others_present() {
    let result = reduce_backend_census(
        Backends::VULKAN,
        Backend::Metal,
        [(Backend::Vulkan, true), (Backend::Metal, true)],
    );
    assert!(matches!(
        result,
        Err(ViewerError::SelectedBackendDisallowed {
            backend: Backend::Metal,
            enabled,
        }) if enabled == Backends::VULKAN
    ));
}

#[test]
fn selected_backend_disallowed_is_distinct_from_missing_observation() {
    assert!(matches!(
        reduce_backend_census(Backends::VULKAN, Backend::Metal, []),
        Err(ViewerError::SelectedBackendDisallowed {
            backend: Backend::Metal,
            ..
        })
    ));
}
