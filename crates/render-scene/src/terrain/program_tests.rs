//! Claims about the terrain programs that need no GPU.

/// Slice 1 of the program-runtime migration moves machinery only. The terrain WGSL is the
/// reference of the byte-equality captures, so a change to it is a change to what the captures
/// mean. A later modernization proposal edits the live file and the snapshot together and records
/// the decision; until then any drift fails here before it can hide inside a capture.
#[test]
fn the_live_terrain_wgsl_equals_the_frozen_snapshot() {
    let pairs = [
        (
            "shader/terrain.wgsl",
            include_str!("../shader/terrain.wgsl"),
            include_str!("fixtures/stock_terrain.wgsl"),
        ),
        (
            "terrain/pages/footprint.wgsl",
            include_str!("pages/footprint.wgsl"),
            include_str!("fixtures/footprint.wgsl"),
        ),
        (
            "terrain/pages/sampling.wgsl",
            include_str!("pages/sampling.wgsl"),
            include_str!("fixtures/sampling.wgsl"),
        ),
        (
            "terrain/pages/layered_gpu.wgsl",
            include_str!("pages/layered_gpu.wgsl"),
            include_str!("fixtures/layered_gpu.wgsl"),
        ),
    ];
    for (name, live, frozen) in pairs {
        assert!(
            live == frozen,
            "{name} differs from its frozen snapshot in terrain/fixtures"
        );
    }
}
