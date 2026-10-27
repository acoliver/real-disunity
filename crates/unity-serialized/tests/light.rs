#[path = "support/decode.rs"]
mod decode;
mod support;

use decode::decode_one;
use worldrunner_unity_serialized::{
    ClassId, SerializedError,
    light::{Light, LightBakingOutput, ShadowSettings, Spot},
    object::PPtr,
    write::ObjectWriter,
};

const fn ptr(path_id: i64) -> PPtr {
    PPtr {
        file_id: 0,
        path_id,
    }
}

/// Flags that turn on, in order: the culling matrix override, the color temperature, the
/// bounding sphere override, view frustum caster culling and forced visibility.
type Flags = [bool; 5];

const ALL: Flags = [true; 5];

/// A spot light whose optional values and flags are set by `flags`.
fn light_bytes(big_endian: bool, flags: Flags) -> Vec<u8> {
    let [matrix, temperature, sphere, frustum, visible] = flags.map(u8::from);
    let mut w = ObjectWriter::new(big_endian);
    w.pptr(ptr(4))
        .u8(1)
        .align()
        .i32(0)
        .floats(&[1.0, 0.9, 0.8, 1.0]);
    w.floats(&[2.0, 10.0, 30.0, 21.0]).floats(&[10.0, 12.0]);
    w.i32(2).i32(-1).i32(512).floats(&[0.75, 0.05, 0.4, 0.2]);
    w.floats(&[3.0; 16]).u8(matrix).align();
    w.pptr(ptr(7)).u8(1).align();
    w.i32(-1).i32(2).i32(4).i32(1).u8(0).align();
    w.pptr(ptr(0))
        .i32(1)
        .u32(0xff)
        .u32(1)
        .i32(4)
        .i32(2)
        .floats(&[1.0, 2.0])
        .f32(1.5);
    w.f32(6_500.0)
        .u8(temperature)
        .align()
        .floats(&[0.0, 1.0, 0.0, 8.0])
        .bytes(&[sphere, frustum, visible])
        .align();
    w.i32(1).f32(1.0).u8(1).align();
    w.finish()
}

fn expected_light(flags: Flags) -> Light {
    let [matrix, temperature, sphere, frustum, visible] = flags;
    Light {
        game_object: ptr(4),
        enabled: true,
        kind: 0,
        color: [1.0, 0.9, 0.8, 1.0],
        intensity: 2.0,
        range: 10.0,
        spot: Spot {
            angle: 30.0,
            inner_angle: 21.0,
            reflector: true,
        },
        cookie_size: [10.0, 12.0],
        shadows: ShadowSettings {
            kind: 2,
            resolution: -1,
            custom_resolution: 512,
            strength: 0.75,
            bias: 0.05,
            normal_bias: 0.4,
            near_plane: 0.2,
            culling_matrix_override: matrix.then_some([3.0; 16]),
            use_view_frustum_for_caster_culling: frustum,
        },
        cookie: ptr(7),
        draw_halo: true,
        baking: LightBakingOutput {
            probe_occlusion_light_index: -1,
            occlusion_mask_channel: 2,
            lightmap_bake_type: 4,
            mixed_lighting_mode: 1,
            is_baked: false,
        },
        flare: ptr(0),
        render_mode: 1,
        culling_mask: 0xff,
        rendering_layer_mask: 1,
        lightmapping: 4,
        shadow_caster_mode: 2,
        area_size: [1.0, 2.0],
        bounce_intensity: 1.5,
        color_temperature: temperature.then_some(6_500.0),
        bounding_sphere_override: sphere.then_some([0.0, 1.0, 0.0, 8.0]),
        force_visible: visible,
        light_unit: 1,
        lux_at_distance: 1.0,
    }
}

#[test]
fn decodes_lights_in_both_byte_orders() {
    for big_endian in [false, true] {
        let decoded: Light =
            decode_one(ClassId::LIGHT, light_bytes(big_endian, ALL), big_endian).unwrap();
        assert_eq!(decoded, expected_light(ALL));
    }
}

#[test]
fn each_flag_sets_only_its_own_value() {
    for set in 0..5 {
        let flags: Flags = std::array::from_fn(|i| i == set);
        let decoded: Light = decode_one(ClassId::LIGHT, light_bytes(false, flags), false).unwrap();
        assert_eq!(decoded, expected_light(flags), "flag {set}");
    }
}

#[test]
fn every_truncated_light_fails() {
    let bytes = light_bytes(false, ALL);
    for end in 0..bytes.len() {
        let error = decode_one::<Light>(ClassId::LIGHT, bytes[..end].to_vec(), false).unwrap_err();
        assert!(
            matches!(error, SerializedError::Truncated { .. }),
            "prefix {end}: {error}"
        );
    }
}

#[test]
fn rejects_flag_bytes_that_are_not_booleans() {
    let mut bytes = light_bytes(false, ALL);
    // The culling matrix override flag follows 60 bytes of light fields, seven 4-byte
    // shadow fields and the 64-byte matrix.
    let flag = 12 + 4 + 4 + 16 + 16 + 8 + 28 + 64;
    assert_eq!(bytes[flag], 1);
    bytes[flag] = 2;
    let error = decode_one::<Light>(ClassId::LIGHT, bytes, false).unwrap_err();
    assert_eq!(
        error,
        SerializedError::InvalidBool {
            offset: flag,
            value: 2
        }
    );
}
