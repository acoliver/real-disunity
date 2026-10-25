use super::reflections::*;
use glam::Vec3;

#[test]
fn replacement_keeps_diffuse_and_counts_specular_once() {
    let diffuse = Vec3::new(0.25, 0.125, 0.0625);
    let environment = Vec3::splat(0.125);
    let reflected = Vec3::splat(1.0);
    let factor = split_sum_factor(Vec3::splat(0.25), [0.5, 0.125]);
    assert_eq!(factor, Vec3::splat(0.25));
    let delta = composition_delta(
        Composition::ReplaceEnvironment,
        reflected,
        environment,
        factor,
        1.0,
    );
    assert_eq!(diffuse + environment + delta, diffuse + reflected * factor);
    let black_hit = composition_delta(
        Composition::ReplaceEnvironment,
        Vec3::ZERO,
        environment,
        factor,
        1.0,
    );
    assert_eq!(diffuse + environment + black_hit, diffuse);
    let partial = composition_delta(
        Composition::ReplaceEnvironment,
        reflected * 0.5,
        environment,
        factor,
        0.5,
    );
    assert_eq!(
        diffuse + environment + partial,
        diffuse + environment.lerp(reflected * factor, 0.5)
    );
}

#[test]
fn zero_confidence_keeps_the_forward_result_and_additive_is_explicit() {
    let environment = Vec3::splat(0.5);
    for mode in [Composition::Additive, Composition::ReplaceEnvironment] {
        assert_eq!(
            composition_delta(mode, Vec3::ONE, environment, Vec3::ONE, 0.0),
            Vec3::ZERO
        );
    }
    assert_eq!(
        composition_delta(
            Composition::Additive,
            Vec3::splat(0.5),
            environment,
            Vec3::splat(0.25),
            0.5
        ),
        Vec3::splat(0.125)
    );
}

#[test]
fn roughness_interpolates_adjacent_radiance_mips() {
    assert_eq!(radiance_lod(0.375, 5), (1, 2, 0.5));
    assert_eq!(radiance_lod(0.0, 5), (0, 1, 0.0));
    assert_eq!(radiance_lod(1.0, 5), (4, 4, 0.0));
    assert_eq!(radiance_lod(0.75, 1), (0, 0, 0.0));
}

#[test]
fn water_selection_is_per_sample_and_never_uses_a_foreground_opaque_receiver() {
    for samples in [1, 4] {
        for sample in 0..samples {
            let water = sample == 0;
            assert_eq!(
                select_receiver(water, 3.0, 5.0),
                if water {
                    Receiver::Water
                } else {
                    Receiver::None
                }
            );
            assert_eq!(
                select_receiver(water, 5.0, 5.0),
                if water {
                    Receiver::Water
                } else {
                    Receiver::Opaque
                }
            );
        }
    }
}
