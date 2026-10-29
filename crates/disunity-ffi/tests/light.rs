//! The 3D light record: creation, round trip of every field, normalization and rejection.
use worldrunner_disunity_ffi::{abi::*, light::*, sprites::*};

fn light_world() -> (u64, u64, u64) {
    let (mut world, mut object, mut light) = (0, 0, 0);
    unsafe {
        assert_eq!(disunity_world_create(&mut world).code, 0);
        assert_eq!(disunity_gameobject_create(world, &mut object).code, 0);
        assert_eq!(disunity_light_create(world, object, &mut light).code, 0);
    }
    (world, object, light)
}

fn read(world: u64, light: u64) -> DisunityLight {
    let mut value = DisunityLight::default();
    assert_eq!(
        unsafe { disunity_light_read(world, light, &mut value) }.code,
        0
    );
    value
}

fn write(world: u64, light: u64, value: DisunityLight) -> DisunityResult {
    unsafe { disunity_light_write(world, light, &value) }
}

/// Every setting differs from its default.
fn distinct() -> DisunityLight {
    DisunityLight {
        cookie_texture: 4242,
        enabled: 0,
        kind: 1,
        r: 1.,
        g: 0.5,
        b: 0.25,
        a: 0.75,
        intensity: 2.5,
        range: 12.,
        spot_angle: 40.,
        inner_spot_angle: 20.,
        shadows: 2,
        shadow_strength: 0.75,
        shadow_bias: 0.0625,
        shadow_normal_bias: 0.5,
        shadow_near_plane: 0.25,
        culling_mask: 0xffff_fffb,
        rendering_layer_mask: 3,
        bounce_intensity: 1.5,
        color_temperature: 5000.,
        use_color_temperature: 1,
        cookie_size: 4.,
        lightmap_bake_type: 2,
    }
}

fn fields(value: DisunityLight) -> (Vec<u32>, Vec<u32>, u64) {
    let floats = [
        value.r,
        value.g,
        value.b,
        value.a,
        value.intensity,
        value.range,
        value.spot_angle,
        value.inner_spot_angle,
        value.shadow_strength,
        value.shadow_bias,
        value.shadow_normal_bias,
        value.shadow_near_plane,
        value.bounce_intensity,
        value.color_temperature,
        value.cookie_size,
    ];
    let integers = [
        value.enabled,
        value.kind,
        value.shadows,
        value.culling_mask,
        value.rendering_layer_mask,
        value.use_color_temperature,
        value.lightmap_bake_type,
    ];
    (
        floats.iter().map(|v| v.to_bits()).collect(),
        integers.to_vec(),
        value.cookie_texture,
    )
}

#[test]
fn a_new_light_reads_back_the_engines_defaults() {
    let (world, _, light) = light_world();
    let value = read(world, light);
    assert_eq!(fields(value), fields(DisunityLight::default()));
    assert_eq!(
        (value.enabled, value.kind, value.lightmap_bake_type),
        (1, 2, 4)
    );
    assert_eq!((value.intensity, value.range), (1., 10.));
    unsafe { assert_eq!(disunity_world_destroy(world).code, 0) };
}

#[test]
fn every_field_round_trips() {
    let (world, _, light) = light_world();
    assert_eq!(write(world, light, distinct()).code, 0);
    assert_eq!(fields(read(world, light)), fields(distinct()));
    unsafe { assert_eq!(disunity_world_destroy(world).code, 0) };
}

#[test]
fn a_write_brings_angles_and_shadow_strength_into_range() {
    let (world, _, light) = light_world();
    let value = DisunityLight {
        spot_angle: 500.,
        inner_spot_angle: 90.,
        shadow_strength: 2.,
        ..distinct()
    };
    assert_eq!(write(world, light, value).code, 0);
    let stored = read(world, light);
    assert_eq!(
        (
            stored.spot_angle,
            stored.inner_spot_angle,
            stored.shadow_strength
        ),
        (179., 90., 1.)
    );
    let narrowed = DisunityLight {
        spot_angle: 10.,
        ..stored
    };
    assert_eq!(write(world, light, narrowed).code, 0);
    assert_eq!(read(world, light).inner_spot_angle, 10.);
    unsafe { assert_eq!(disunity_world_destroy(world).code, 0) };
}

#[test]
fn invalid_records_are_rejected_and_leave_the_light_unchanged() {
    let (world, _, light) = light_world();
    assert_eq!(write(world, light, distinct()).code, 0);
    let rejected = [
        DisunityLight {
            intensity: -1.,
            ..distinct()
        },
        DisunityLight {
            range: f32::NAN,
            ..distinct()
        },
        DisunityLight {
            bounce_intensity: f32::INFINITY,
            ..distinct()
        },
        DisunityLight {
            cookie_size: -0.5,
            ..distinct()
        },
        DisunityLight {
            color_temperature: 0.,
            ..distinct()
        },
        DisunityLight {
            spot_angle: f32::NAN,
            ..distinct()
        },
        DisunityLight {
            shadow_strength: f32::NAN,
            ..distinct()
        },
        DisunityLight {
            r: f32::NAN,
            ..distinct()
        },
        DisunityLight {
            shadow_bias: f32::INFINITY,
            ..distinct()
        },
    ];
    for value in rejected {
        let result = write(world, light, value);
        assert_eq!(
            result.detail,
            DisunityErrorReason::InvalidComponent as u64,
            "{value:?}"
        );
        assert_eq!(fields(read(world, light)), fields(distinct()));
    }
    unsafe { assert_eq!(disunity_world_destroy(world).code, 0) };
}

#[test]
fn unknown_enumerations_and_booleans_are_rejected() {
    let (world, _, light) = light_world();
    let rejected = [
        DisunityLight {
            kind: 8,
            ..distinct()
        },
        DisunityLight {
            shadows: 3,
            ..distinct()
        },
        DisunityLight {
            lightmap_bake_type: 3,
            ..distinct()
        },
        DisunityLight {
            enabled: 2,
            ..distinct()
        },
        DisunityLight {
            use_color_temperature: 2,
            ..distinct()
        },
    ];
    for value in rejected {
        assert_ne!(write(world, light, value).code, 0, "{value:?}");
        assert_eq!(fields(read(world, light)), fields(DisunityLight::default()));
    }
    unsafe { assert_eq!(disunity_world_destroy(world).code, 0) };
}

#[test]
fn null_pointers_and_other_components_are_refused() {
    let (world, object, light) = light_world();
    unsafe {
        assert_eq!(
            disunity_light_write(world, light, std::ptr::null()).code,
            result_code::INVALID_ARGUMENT
        );
        assert_eq!(
            disunity_light_read(world, light, std::ptr::null_mut()).code,
            result_code::INVALID_ARGUMENT
        );
        assert_eq!(
            disunity_light_create(world, object, std::ptr::null_mut()).code,
            result_code::INVALID_ARGUMENT
        );
        let mut sprite = 0;
        assert_eq!(disunity_sprite_create(world, object, &mut sprite).code, 0);
        assert_ne!(write(world, sprite, distinct()).code, 0);
        let mut second = 0;
        assert_ne!(disunity_light_create(world, object, &mut second).code, 0);
        assert_eq!(disunity_world_destroy(world).code, 0);
    }
}
