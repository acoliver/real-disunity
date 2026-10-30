use super::*;

#[test]
fn alpha_coverage_is_opt_in_masked_and_keyed() {
    let stock = material(
        SceneAlphaMode::Mask {
            cutoff_micro: 500_000,
        },
        false,
    );
    assert!(!stock.alpha_to_coverage());
    let enabled = stock.with_alpha_to_coverage(true).unwrap();
    assert!(enabled.alpha_to_coverage());
    assert_ne!(
        PipelineKey::from_material(&stock),
        PipelineKey::from_material(&enabled)
    );
    assert_eq!(
        MaterialUniform::from_material(&stock).to_bytes(),
        MaterialUniform::from_material(&stock.with_alpha_to_coverage(false).unwrap()).to_bytes()
    );
    assert!(
        material(SceneAlphaMode::Opaque, false)
            .with_alpha_to_coverage(true)
            .is_err()
    );
}

#[test]
fn alpha_coverage_requires_multisampled_masked_pass() {
    let enabled = material(
        SceneAlphaMode::Mask {
            cutoff_micro: 500_000,
        },
        false,
    )
    .with_alpha_to_coverage(true)
    .unwrap();
    let key = PipelineKey::from_material(&enabled);
    assert!(!key.alpha_to_coverage(1));
    assert!(key.alpha_to_coverage(4));
    for key in PipelineKey::ALL {
        assert_eq!(key.alpha_to_coverage(4), key.masked && key.index() >= 8);
        assert!(!key.alpha_to_coverage(1));
    }
}
