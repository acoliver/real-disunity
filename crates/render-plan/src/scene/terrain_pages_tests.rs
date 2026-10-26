use super::terrain_pages::*;

pub(crate) fn parts() -> TerrainPageSettingsParts {
    TerrainPageSettingsParts {
        interior: [31, 17],
        border: 4,
        filter_radius: 1,
        maximum_depth: 8,
        split_threshold_millipixels: 2_000,
        merge_threshold_millipixels: 1_000,
        maximum_selected_pages: 64,
        resident_bytes: 1_000_000,
        upload_bytes_per_frame: 100_000,
    }
}

#[test]
fn terrain_pages_validate_dimensions_and_filter_support() {
    let settings = TerrainPageSettings::new(parts()).unwrap();
    assert_eq!(settings.extent(), [39, 25]);
    assert_eq!(
        settings.mip_extents(),
        vec![[39, 25], [19, 12], [9, 6], [4, 3], [2, 1], [1, 1]]
    );
    assert_eq!(settings.page_bytes(), 10_176);
    for interior in [[0, 1], [1, 0], [u32::MAX, 1]] {
        assert!(
            TerrainPageSettings::new(TerrainPageSettingsParts {
                interior,
                ..parts()
            })
            .is_err()
        );
    }
    assert!(
        TerrainPageSettings::new(TerrainPageSettingsParts {
            border: 0,
            ..parts()
        })
        .is_err()
    );
    assert!(
        TerrainPageSettings::new(TerrainPageSettingsParts {
            filter_radius: 5,
            ..parts()
        })
        .is_err()
    );
}

#[test]
fn terrain_pages_reject_unbounded_selection_and_inverted_hysteresis() {
    for changed in [
        TerrainPageSettingsParts {
            maximum_depth: 17,
            ..parts()
        },
        TerrainPageSettingsParts {
            maximum_selected_pages: 0,
            ..parts()
        },
        TerrainPageSettingsParts {
            merge_threshold_millipixels: 2_000,
            ..parts()
        },
        TerrainPageSettingsParts {
            merge_threshold_millipixels: 0,
            ..parts()
        },
        TerrainPageSettingsParts {
            upload_bytes_per_frame: 1,
            ..parts()
        },
        TerrainPageSettingsParts {
            resident_bytes: 1,
            ..parts()
        },
    ] {
        assert!(TerrainPageSettings::new(changed).is_err());
    }
}

#[test]
fn terrain_pages_expose_only_filter_safe_mips() {
    let settings = TerrainPageSettings::new(parts()).unwrap();
    assert_eq!(settings.maximum_sampling_mip(), 0);
    let aligned = TerrainPageSettings::new(TerrainPageSettingsParts {
        interior: [8, 12],
        border: 4,
        ..parts()
    })
    .unwrap();
    assert_eq!(aligned.maximum_sampling_mip(), 2);
    let narrow = TerrainPageSettings::new(TerrainPageSettingsParts {
        border: 1,
        ..parts()
    })
    .unwrap();
    assert_eq!(narrow.maximum_sampling_mip(), 0);
}

#[test]
fn terrain_page_policy_requires_bounded_slots_and_allocation() {
    use super::{TerrainPagePolicy, TerrainPageSettingsError};
    let settings = TerrainPageSettings::new(parts()).unwrap();
    assert_eq!(
        TerrainPagePolicy::new(settings, 84, 10_000_000),
        Err(TerrainPageSettingsError::SelectionBounds)
    );
    assert_eq!(
        TerrainPagePolicy::new(settings, 85, 1),
        Err(TerrainPageSettingsError::Budget)
    );
    let policy = TerrainPagePolicy::new(settings, 85, 10_000_000).unwrap();
    assert_eq!(policy.settings(), settings);
    assert_eq!(policy.slots(), 85);
    assert_eq!(policy.budget_bytes(), 10_000_000);
    let root = TerrainPageSettings::new(TerrainPageSettingsParts {
        maximum_depth: 0,
        ..parts()
    })
    .unwrap();
    assert!(TerrainPagePolicy::new(root, 1, 10_000_000).is_ok());
}
