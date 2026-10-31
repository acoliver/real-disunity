use super::*;

fn settings() -> SceneSpecularPreparation {
    SceneSpecularPreparation {
        convention: SceneSpecularConvention::default(),
        cube_size: 4,
        mip_count: 3,
        samples: 8,
        source_lod_tail: 1,
    }
}

#[test]
fn source_cube_request_validates_settings_and_preserves_prefiltered_payload() {
    let cube = SceneSpecularCube::rgba16_float(1, 1, vec![0; 48].into()).unwrap();
    assert_eq!(cube.preparation(), None);
    let source = cube.clone().with_preparation(settings()).unwrap();
    assert_eq!(source.bytes(), cube.bytes());
    assert_eq!(source.preparation(), Some(settings()));
    for invalid in [
        SceneSpecularPreparation {
            samples: 0,
            ..settings()
        },
        SceneSpecularPreparation {
            cube_size: 3,
            ..settings()
        },
        SceneSpecularPreparation {
            mip_count: 4,
            ..settings()
        },
        SceneSpecularPreparation {
            source_lod_tail: 0,
            ..settings()
        },
    ] {
        assert!(matches!(
            cube.clone().with_preparation(invalid),
            Err(SceneValidationError::UnsupportedInput {
                part: "specular cube preparation settings"
            })
        ));
    }
}
