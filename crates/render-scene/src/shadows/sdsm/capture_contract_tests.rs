use super::*;
use worldrunner_render_gpu::submission::capture::{WriteError, WriteKey};

#[test]
fn visible_instance_history_is_a_suffix_only_cpu_write() {
    let resource = WriteKey::Resident(1);
    let history = WriteKey::Resident(2);
    let depth = WriteKey::Resource {
        id: 3,
        generation: 0,
    };
    let reduction = WriteKey::Readback(4);
    let writes = capture_writes(
        vec![resource],
        vec![resource, history, depth],
        vec![],
        depth,
        reduction,
    )
    .unwrap();
    assert!(!writes.prefix_keys().any(|key| key == history));
    assert!(writes.suffix_keys().any(|key| key == history));
    CaptureWrites::new(writes.prefix_keys(), writes.suffix_keys(), [resource]).unwrap();
}

#[test]
fn aliased_metadata_depth_remains_owned_by_the_capture_suffix() {
    let scene = WriteKey::Resident(1);
    let depth = WriteKey::Resource {
        id: 2,
        generation: 0,
    };
    let normals = WriteKey::Resource {
        id: 3,
        generation: 0,
    };
    let linear = WriteKey::Resource {
        id: 4,
        generation: 0,
    };
    let material = WriteKey::Resource {
        id: 5,
        generation: 0,
    };
    let reduction = WriteKey::Readback(6);
    let receiver_a = WriteKey::Resource {
        id: 7,
        generation: 0,
    };
    let receiver_b = WriteKey::Resource {
        id: 8,
        generation: 0,
    };
    let writes = capture_writes(
        vec![scene],
        vec![
            scene, depth, normals, linear, material, receiver_a, receiver_b,
        ],
        vec![depth, normals, linear, material, receiver_a, receiver_b],
        depth,
        reduction,
    )
    .unwrap();
    let prefix: Vec<_> = writes.prefix_keys().collect();
    let suffix: Vec<_> = writes.suffix_keys().collect();
    assert!(prefix.contains(&depth));
    assert!(suffix.contains(&depth));
    for key in [normals, linear, material, receiver_a, receiver_b, reduction] {
        assert!(prefix.contains(&key));
        assert!(!suffix.contains(&key));
    }
    assert_eq!(
        CaptureWrites::new(prefix.clone(), suffix.clone(), [scene]).unwrap_err(),
        WriteError::Overlap(depth),
    );
    CaptureWrites::new(prefix, suffix, [scene, depth]).unwrap();
}

#[test]
fn independent_metadata_depth_is_prefix_only() {
    let scene = WriteKey::Resident(1);
    let depth = WriteKey::Resource {
        id: 2,
        generation: 0,
    };
    let metadata = WriteKey::Resource {
        id: 3,
        generation: 0,
    };
    let reduction = WriteKey::Readback(4);
    let writes = capture_writes(
        vec![scene],
        vec![scene, depth, metadata],
        vec![metadata],
        depth,
        reduction,
    )
    .unwrap();
    assert!(writes.prefix_keys().any(|key| key == metadata));
    assert!(!writes.prefix_keys().any(|key| key == depth));
    assert!(writes.suffix_keys().any(|key| key == depth));
    assert!(!writes.suffix_keys().any(|key| key == metadata));
}

#[test]
fn contour_capture_keeps_physical_target_and_publication_in_suffix_only() {
    let scene = WriteKey::Resident(1);
    let published = WriteKey::Resident(2);
    let target = WriteKey::Resource {
        id: 2,
        generation: 0,
    };
    let depth = WriteKey::Resource {
        id: 3,
        generation: 0,
    };
    let reduction = WriteKey::Readback(4);
    for active in [false, true] {
        let mut suffix = vec![scene, published, depth];
        suffix.extend(active.then_some(target));
        let writes = capture_writes(vec![scene], suffix, vec![depth], depth, reduction).unwrap();
        assert!(!writes.prefix_keys().any(|key| key == target));
        assert!(!writes.prefix_keys().any(|key| key == published));
        assert!(writes.suffix_keys().any(|key| key == published));
        assert_eq!(writes.suffix_keys().any(|key| key == target), active);
    }
}

#[test]
fn game_target_writes_and_publications_belong_to_the_capture_suffix_only() {
    let scene = WriteKey::Resident(1);
    let texture = WriteKey::Resource {
        id: 2,
        generation: 0,
    };
    let kept = WriteKey::Resident(3);
    let depth = WriteKey::Resource {
        id: 4,
        generation: 0,
    };
    let reduction = WriteKey::Readback(5);
    let writes = capture_writes(
        vec![scene],
        vec![scene, depth, texture, kept],
        vec![depth],
        depth,
        reduction,
    )
    .unwrap();
    for key in [texture, kept] {
        assert!(writes.suffix_keys().any(|suffix| suffix == key));
        assert!(!writes.prefix_keys().any(|prefix| prefix == key));
    }
    CaptureWrites::new(writes.prefix_keys(), writes.suffix_keys(), [scene, depth]).unwrap();
}
