use super::support::world;

#[test]
fn samples_flat_terrain_and_clamps_to_edges() {
    let terrain = world().terrain;
    assert_eq!(terrain.sample_height_meters(0.0, 0.0), Some(0.0));
    assert_eq!(terrain.sample_height_meters(-0.5, -0.5), Some(0.0));
    assert_eq!(terrain.sample_height_meters(10.0, 10.0), Some(0.0));
}

#[test]
fn bilinearly_samples_sloped_terrain_in_metres() {
    let mut terrain = world().terrain;
    terrain.heights_millimeters = vec![0, 2_000, 4_000, 6_000];
    assert_eq!(terrain.sample_height_meters(-0.5, -0.5), Some(0.0));
    assert_eq!(terrain.sample_height_meters(0.5, 0.5), Some(6.0));
    assert_eq!(terrain.sample_height_meters(0.0, 0.0), Some(3.0));
    assert_eq!(terrain.sample_height_meters(0.25, -0.25), Some(2.5));
    assert_eq!(terrain.sample_height_meters(-10.0, 10.0), Some(4.0));
}

#[test]
fn rejects_nonfinite_sample_coordinates() {
    let terrain = world().terrain;
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(terrain.sample_height_meters(invalid, 0.0), None);
        assert_eq!(terrain.sample_height_meters(0.0, invalid), None);
    }
}

#[test]
fn malformed_heightfields_do_not_panic_when_sampled() {
    let terrain = world().terrain;
    for missing in 0..4 {
        let mut malformed = terrain.clone();
        malformed.heights_millimeters.truncate(missing);
        assert_eq!(malformed.sample_height_meters(0.0, 0.0), None);
    }
    let mut malformed = terrain.clone();
    malformed.width_samples = u32::MAX;
    malformed.depth_samples = u32::MAX;
    assert_eq!(malformed.sample_height_meters(0.0, 0.0), None);
    malformed = terrain;
    malformed.sample_spacing_millimeters = 0;
    assert_eq!(malformed.sample_height_meters(0.0, 0.0), None);
}

#[test]
fn interpolation_stays_within_corner_bounds_for_many_signed_heights() {
    for base in [-1_000_000, -7_000, 0, 42_000, 1_000_000] {
        let mut terrain = world().terrain;
        terrain.heights_millimeters = vec![base, base + 1_000, base + 2_000, base + 3_000];
        for east in -10..=10 {
            for north in -10..=10 {
                let height = terrain
                    .sample_height_meters(f64::from(east) / 20.0, f64::from(north) / 20.0)
                    .unwrap();
                assert!(height >= f64::from(base) / 1_000.0 - 1e-10);
                assert!(height <= f64::from(base + 3_000) / 1_000.0 + 1e-10);
            }
        }
    }
}
