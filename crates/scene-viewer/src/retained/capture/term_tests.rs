use super::*;

#[test]
fn term_outputs_are_native_budgeted_and_reject_accumulation() {
    for output in [
        CaptureOutput::Albedo,
        CaptureOutput::SunVisibility,
        CaptureOutput::DirectDiffuse,
        CaptureOutput::AmbientDiffuse,
        CaptureOutput::EnvironmentSpecular,
        CaptureOutput::SunSpecular,
    ] {
        let recipe =
            RetainedCaptureRecipe::new(1, 1, CaptureJitter::None, false, vec![output], 1024 * 1024)
                .unwrap();
        assert!(recipe.storage_bytes([32, 32]).unwrap() > 32 * 32 * 12);
        assert!(
            recipe
                .with_native_rcas(crate::NativeRcasSettings {
                    stops: 0.5,
                    domain: crate::RcasDomain::EncodedOutput,
                })
                .is_err()
        );
        assert!(RetainedCaptureRecipe::new(
            2, 1, CaptureJitter::None, false, vec![output], 1024 * 1024,
        ).is_err());
        assert!(
            RetainedCaptureRecipe::new(
                1,
                1,
                CaptureJitter::None,
                false,
                vec![output, output],
                1024 * 1024,
            )
            .is_err()
        );
    }
}

#[test]
fn replay_targets_are_charged_only_to_requested_term_storage() {
    use worldrunner_render_scene::compositor::RendererConfiguration;
    assert_eq!(
        RendererConfiguration::capture_term_allocations([32, 32])
            .validate(&wgpu::Limits::default())
            .unwrap(),
        32 * 32 * 12
    );
    assert!(RetainedCaptureProducts::default().terms.is_none());
    let make = |output| {
        RetainedCaptureRecipe::new(1, 1, CaptureJitter::None, false, vec![output], 1024 * 1024)
            .unwrap()
    };
    assert_eq!(
        make(CaptureOutput::Albedo).storage_bytes([32, 32]).unwrap(),
        make(CaptureOutput::Normals)
            .storage_bytes([32, 32])
            .unwrap()
            + 32 * 32 * 12
    );
}

#[test]
fn radiance_recipes_reject_every_accumulation_mode_and_charge_each_target() {
    let outputs = [
        CaptureOutput::DirectDiffuse,
        CaptureOutput::AmbientDiffuse,
        CaptureOutput::EnvironmentSpecular,
        CaptureOutput::SunSpecular,
    ];
    for output in outputs {
        for (spatial, temporal, jitter, clock) in [
            (2, 1, CaptureJitter::None, false),
            (1, 2, CaptureJitter::None, false),
            (1, 1, CaptureJitter::Halton23, false),
            (1, 1, CaptureJitter::None, true),
        ] {
            assert_eq!(
                RetainedCaptureRecipe::new(
                    spatial,
                    temporal,
                    jitter,
                    clock,
                    vec![output],
                    1024 * 1024
                ),
                Err(CaptureRequestError::UnsupportedRecipe)
            );
        }
    }
    let recipe = RetainedCaptureRecipe::new(
        1,
        1,
        CaptureJitter::None,
        false,
        outputs.to_vec(),
        1024 * 1024,
    )
    .unwrap();
    let single = RetainedCaptureRecipe::new(
        1,
        1,
        CaptureJitter::None,
        false,
        vec![CaptureOutput::Albedo],
        1024 * 1024,
    )
    .unwrap();
    assert_eq!(
        recipe.storage_bytes([33, 2]).unwrap(),
        4 * single.storage_bytes([33, 2]).unwrap()
    );
}
