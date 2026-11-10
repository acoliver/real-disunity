//! The census of a player's compute kernels on a device: every kernel variant of the SPIR-V
//! platform is built as a compute program and the ones that are not are counted by cause.

use super::super::census::compute_census;
use super::gpu::gpu;
use super::kernels::{copy_object, text_object, write_compute};
use super::storage::painter;

#[test]
fn every_variant_is_built_on_the_device_and_the_ones_it_cannot_hold_are_counted_by_cause() {
    let directory = tempfile::tempdir().unwrap();
    // The copy kernel, a kernel with four storage images, one with five (the test device allows
    // four in a stage) and a shader with no SPIR-V platform, which has no variants to count.
    write_compute(
        directory.path(),
        "resources.assets",
        vec![
            (1, copy_object()),
            (2, painter(4)),
            (3, painter(5)),
            (4, text_object()),
        ],
    );
    let runtime = gpu().runtime;

    let census = compute_census(directory.path(), &runtime).unwrap();

    assert_eq!(census.shaders, 4);
    assert_eq!((census.variants, census.programs), (3, 2));
    assert_eq!(
        census.failed.into_iter().collect::<Vec<_>>(),
        [("StorageLimit", 1)]
    );
}
