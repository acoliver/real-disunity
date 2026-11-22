//! Requirements of two users of one device, combined.
use super::*;

fn needs(features: Features, limits: Limits) -> DeviceRequirements {
    DeviceRequirements {
        required_features: features,
        required_limits: limits,
        ..DeviceRequirements::default()
    }
}

#[test]
fn a_union_asks_for_every_feature_either_side_asks_for() {
    let mut player = needs(Features::FLOAT32_FILTERABLE, Limits::default());
    player.optional_features = Features::TEXTURE_COMPRESSION_BC;
    let mut viewer = needs(Features::empty(), Limits::default());
    viewer.optional_features = Features::DEPTH32FLOAT_STENCIL8;
    let both = player.union(&viewer);
    assert_eq!(both.required_features, Features::FLOAT32_FILTERABLE);
    assert_eq!(
        both.optional_features,
        Features::TEXTURE_COMPRESSION_BC | Features::DEPTH32FLOAT_STENCIL8
    );
}

#[test]
fn a_union_takes_the_more_demanding_value_of_each_limit() {
    let player = needs(
        Features::empty(),
        Limits {
            max_buffer_size: 1 << 31,
            min_uniform_buffer_offset_alignment: 256,
            ..Limits::default()
        },
    );
    let viewer = needs(
        Features::empty(),
        Limits {
            max_texture_array_layers: 2048,
            min_uniform_buffer_offset_alignment: 64,
            ..Limits::default()
        },
    );
    let both = player.union(&viewer).required_limits;
    assert_eq!(both.max_buffer_size, 1 << 31);
    assert_eq!(both.max_texture_array_layers, 2048);
    // An alignment is more demanding when it is smaller.
    assert_eq!(both.min_uniform_buffer_offset_alignment, 64);
}

#[test]
fn a_union_keeps_a_preferred_profile_that_still_meets_both_sides_requirements() {
    let mut player = needs(Features::empty(), Limits::default());
    player.optional_limits = Some(Limits {
        max_storage_textures_per_shader_stage: 8,
        ..Limits::default()
    });
    let viewer = needs(
        Features::empty(),
        Limits {
            max_texture_array_layers: 2048,
            ..Limits::default()
        },
    );
    let both = player.union(&viewer);
    let preferred = both.optional_limits.clone().expect("the player's profile");
    assert_eq!(preferred.max_storage_textures_per_shader_stage, 8);
    assert_eq!(preferred.max_texture_array_layers, 2048);
    let generous = Limits {
        max_storage_textures_per_shader_stage: 8,
        max_texture_array_layers: 2048,
        ..Limits::default()
    };
    let negotiated = negotiate(Features::empty(), &generous, &both).unwrap();
    assert!(negotiated.unavailable_optional_limits.is_empty());
    assert_eq!(negotiated.limits, preferred);
}

#[test]
fn a_device_asks_for_the_adapters_storage_images_from_four_up_to_sixteen() {
    for (offered, asked) in [
        (0, 4),
        (3, 4),
        (4, 4),
        (5, 5),
        (16, 16),
        (17, 16),
        (1 << 20, 16),
    ] {
        let available = Limits {
            max_storage_textures_per_shader_stage: offered,
            ..Limits::default()
        };
        assert_eq!(
            storage_images_per_stage(&available),
            asked,
            "an adapter offering {offered}"
        );
    }
}

#[test]
fn a_union_of_requirements_without_profiles_has_none() {
    let both = needs(Features::empty(), Limits::default())
        .union(&needs(Features::empty(), Limits::default()));
    assert!(both.optional_limits.is_none());
}
