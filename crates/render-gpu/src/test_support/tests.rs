// CPU identity tokens exercise the same profile lookup and insertion used for wgpu handles.
use super::{DeviceProfile, ProfileCache};
use crate::device::{DeviceRequirements, negotiate};
use std::sync::Arc;
use wgpu::{Features, Limits};

fn profile(requirements: &DeviceRequirements) -> DeviceProfile {
    let negotiated = negotiate(Features::all(), &Limits::default(), requirements).unwrap();
    DeviceProfile::from(&negotiated)
}

fn device(cache: &mut ProfileCache<Arc<()>>, profile: DeviceProfile) -> Arc<()> {
    cache
        .get_or_try_insert_with(profile, || Ok::<_, ()>(Arc::new(())))
        .unwrap()
}

#[test]
fn same_profile_returns_the_same_device_identity() {
    let mut cache = ProfileCache::default();
    let key = profile(&DeviceRequirements::default());
    let first = device(&mut cache, key.clone());
    let second = device(&mut cache, key);
    assert!(Arc::ptr_eq(&first, &second));
}

#[test]
fn different_full_limits_profiles_return_different_device_identities() {
    let mut cache = ProfileCache::default();
    let first = device(&mut cache, profile(&DeviceRequirements::default()));
    let reduced = DeviceRequirements {
        required_limits: Limits {
            max_buffer_size: 8,
            ..Default::default()
        },
        ..Default::default()
    };
    let second = device(&mut cache, profile(&reduced));
    assert!(!Arc::ptr_eq(&first, &second));
    let aligned = DeviceRequirements {
        required_limits: Limits {
            min_uniform_buffer_offset_alignment: 512,
            ..Default::default()
        },
        ..Default::default()
    };
    let third = device(&mut cache, profile(&aligned));
    assert!(!Arc::ptr_eq(&first, &third));
    assert!(!Arc::ptr_eq(&second, &third));
}

#[test]
fn features_are_never_unioned_across_profiles() {
    let mut cache = ProfileCache::default();
    let empty = profile(&DeviceRequirements::default());
    let timestamp = profile(&DeviceRequirements {
        required_features: Features::TIMESTAMP_QUERY,
        ..Default::default()
    });
    let first = device(&mut cache, empty.clone());
    let second = device(&mut cache, timestamp.clone());
    assert!(!Arc::ptr_eq(&first, &second));
    assert!(Arc::ptr_eq(&first, &device(&mut cache, empty.clone())));
    assert_eq!(empty.features, Features::empty());
    assert_eq!(timestamp.features, Features::TIMESTAMP_QUERY);
}

#[test]
fn cache_keys_use_negotiated_features_and_optional_limits() {
    let mut cache = ProfileCache::default();
    let requirements = DeviceRequirements {
        optional_features: Features::TIMESTAMP_QUERY,
        optional_limits: Some(Limits {
            max_buffer_size: 8,
            ..Default::default()
        }),
        ..Default::default()
    };
    let negotiated = negotiate(Features::empty(), &Limits::default(), &requirements).unwrap();
    assert!(!negotiated.unavailable_optional_limits.is_empty());
    let actual = DeviceProfile::from(&negotiated);
    let default = profile(&DeviceRequirements::default());
    let first = device(&mut cache, actual);
    assert!(Arc::ptr_eq(&first, &device(&mut cache, default)));
}

#[test]
fn failed_creation_does_not_publish_a_cache_entry() {
    let mut cache = ProfileCache::<Arc<()>>::default();
    let key = profile(&DeviceRequirements::default());
    assert_eq!(
        cache.get_or_try_insert_with(key.clone(), || Err("failed")),
        Err("failed")
    );
    let first = device(&mut cache, key.clone());
    assert!(Arc::ptr_eq(&first, &device(&mut cache, key)));
}

#[test]
fn panicking_creation_preserves_cached_devices_and_leaves_the_miss_unpublished() {
    use std::sync::{Mutex, PoisonError};
    let cache = Mutex::new(ProfileCache::<Arc<()>>::default());
    let existing = profile(&DeviceRequirements::default());
    let first = device(&mut cache.lock().unwrap(), existing.clone());
    let missing = profile(&DeviceRequirements {
        required_features: Features::TIMESTAMP_QUERY,
        ..Default::default()
    });
    let panic = std::panic::catch_unwind(|| {
        let _ = cache
            .lock()
            .unwrap()
            .get_or_try_insert_with(missing.clone(), || -> Result<Arc<()>, ()> {
                panic!("device creation failed")
            });
    });
    assert!(panic.is_err());
    assert!(cache.is_poisoned());
    let mut cache = cache.lock().unwrap_or_else(PoisonError::into_inner);
    assert_eq!(cache.entries.len(), 1);
    assert!(Arc::ptr_eq(&first, &device(&mut cache, existing)));
    assert!(!cache.entries.contains_key(&missing));
    let second = device(&mut cache, missing.clone());
    assert!(!Arc::ptr_eq(&first, &second));
    assert!(Arc::ptr_eq(&second, &device(&mut cache, missing)));
    assert_eq!(cache.entries.len(), 2);
}

#[test]
fn parallel_cache_misses_publish_one_identity() {
    use std::sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    };
    let cache = Mutex::new(ProfileCache::<Arc<()>>::default());
    let creations = AtomicUsize::new(0);
    let devices = std::thread::scope(|scope| {
        let threads: Vec<_> = (0..4)
            .map(|_| {
                scope.spawn(|| {
                    cache
                        .lock()
                        .unwrap()
                        .get_or_try_insert_with(profile(&DeviceRequirements::default()), || {
                            creations.fetch_add(1, Ordering::Relaxed);
                            Ok::<_, ()>(Arc::new(()))
                        })
                        .unwrap()
                })
            })
            .collect();
        threads
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(creations.load(Ordering::Relaxed), 1);
    for other in &devices[1..] {
        assert!(Arc::ptr_eq(&devices[0], other));
    }
}

#[test]
fn optional_and_required_requests_share_only_their_effective_profile() {
    let mut cache = ProfileCache::default();
    let optional = profile(&DeviceRequirements {
        optional_features: Features::TIMESTAMP_QUERY,
        ..Default::default()
    });
    let required = profile(&DeviceRequirements {
        required_features: Features::TIMESTAMP_QUERY,
        ..Default::default()
    });
    let first = device(&mut cache, optional);
    assert!(Arc::ptr_eq(&first, &device(&mut cache, required)));
    let empty = device(&mut cache, profile(&DeviceRequirements::default()));
    assert!(!Arc::ptr_eq(&first, &empty));
    assert_eq!(cache.entries.len(), 2);
}

#[test]
fn featureful_first_request_cannot_upgrade_a_later_empty_request() {
    let mut cache = ProfileCache::default();
    let features = Features::TIMESTAMP_QUERY | Features::TEXTURE_COMPRESSION_BC;
    let full = profile(&DeviceRequirements {
        required_features: features,
        ..Default::default()
    });
    let first = device(&mut cache, full.clone());
    let timestamp = profile(&DeviceRequirements {
        required_features: Features::TIMESTAMP_QUERY,
        ..Default::default()
    });
    let second = device(&mut cache, timestamp.clone());
    let empty = profile(&DeviceRequirements::default());
    let third = device(&mut cache, empty.clone());
    assert!(!Arc::ptr_eq(&first, &second));
    assert!(!Arc::ptr_eq(&first, &third));
    assert!(!Arc::ptr_eq(&second, &third));
    assert_eq!(cache.entries.len(), 3);
    assert_eq!(full.features, features);
    assert_eq!(timestamp.features, Features::TIMESTAMP_QUERY);
    assert!(empty.features.is_empty());
}
