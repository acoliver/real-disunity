use super::{SceneDirection, SceneFoliageWind, SceneWind, SceneWindBand, SceneWindPhase};

#[test]
fn foliage_parameters_fail_at_construction_and_stock_stays_rigid() {
    let axis = SceneDirection::new([0, 0, 1_000_000]).unwrap();
    for frequency in [0, 1, 64_000_001, u32::MAX] {
        assert!(SceneWindBand::new(frequency, 0, 0).is_err());
    }
    assert!(SceneWindBand::new(1_000_000, 1_000_001, 0).is_err());
    assert!(SceneWindBand::new(1_000_000, 0, 64_000_001).is_err());
    let band = SceneWindBand::new(1_000_000, 100_000, 150_000).unwrap();
    for (extents, curve) in [
        ([0, 1000], 1_000_000),
        ([1000, 0], 1_000_000),
        ([1000; 2], 0),
        ([1000; 2], 999_999),
        ([1000; 2], 8_000_001),
    ] {
        assert!(
            SceneFoliageWind::new(
                axis,
                extents,
                curve,
                [band; 2],
                SceneWindPhase::WorldPosition
            )
            .is_err()
        );
    }
    let curved = SceneFoliageWind::new(
        axis,
        [2000, 1000],
        1_000_000,
        [band; 2],
        SceneWindPhase::InstancePosition,
    )
    .unwrap();
    assert!(curved.with_strength(4_000_001).is_err());
    let stock = SceneWind::new(axis, 500, 1000, 123_456).unwrap();
    assert_eq!(stock.foliage(), None);
    assert_eq!(stock.with_foliage(curved).foliage(), Some(curved));
    assert_eq!(stock.with_foliage(curved).period_milliseconds(), 1000);
}
