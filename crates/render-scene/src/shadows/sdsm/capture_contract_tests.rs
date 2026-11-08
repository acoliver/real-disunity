use super::*;
use worldrunner_render_gpu::submission::capture::{WriteError, WriteKey};

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
    let writes = capture_writes(
        vec![scene],
        vec![scene, depth, normals, linear, material],
        vec![depth, normals, linear, material],
        depth,
        reduction,
    )
    .unwrap();
    let prefix: Vec<_> = writes.prefix_keys().collect();
    let suffix: Vec<_> = writes.suffix_keys().collect();
    assert!(prefix.contains(&depth));
    assert!(suffix.contains(&depth));
    for key in [normals, linear, material, reduction] {
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
