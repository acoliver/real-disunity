//! Claims about the water body path that need no byte comparison.

/// The water files the body path composes, with the snapshot of each. `water_source` joins them
/// to the lighting, post and shadow crates' text and to `shader/lighting.wgsl`,
/// `shader/camera_rays.wgsl` and one of `shader/shadows.wgsl` and `shader/unshadowed.wgsl`,
/// which `object/fixtures` freezes and `object::program_tests` pins.
///
/// A capture that compares the scene's water with a fixed path built from the snapshots means
/// what it says only while the live files equal them. A change to a live file fails here before
/// it can hide inside such a capture.
macro_rules! pairs {
    ($($name:literal),* $(,)?) => {
        [$((
            $name,
            include_str!(concat!($name, ".wgsl")),
            include_str!(concat!("fixtures/", $name, ".wgsl")),
        )),*]
    };
}

#[test]
fn the_live_water_wgsl_equals_the_frozen_snapshot() {
    let pairs = pairs![
        "ambient_visibility",
        "optics",
        "water",
        "screen",
        "terms",
        "shadow_lookup",
        "shadow_response",
        "shadow_visibility",
    ];
    let drifted: Vec<_> = pairs
        .iter()
        .filter(|(_, live, frozen)| live != frozen)
        .map(|(name, ..)| *name)
        .collect();
    assert!(
        drifted.is_empty(),
        "the live water WGSL differs from its frozen snapshot in water/fixtures for {drifted:?}. \
         When the change is intended, copy the live file over the snapshot \
         (`cp water/<name>.wgsl water/fixtures/<name>.wgsl` from crates/render-scene/src)."
    );
}
