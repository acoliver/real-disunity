#[path = "support/decode.rs"]
mod decode;
mod support;

use decode::decode_one;
use worldrunner_unity_serialized::{
    ClassId, SerializedError,
    camera::{Camera, CameraOutput, PhysicalCamera, Projection},
    light::{Light, LightBakingOutput, ShadowSettings, Spot},
    object::{Layout, PPtr},
    write::ObjectWriter,
};

const fn ptr(path_id: i64) -> PPtr {
    PPtr {
        file_id: 0,
        path_id,
    }
}

fn camera_bytes(big_endian: bool) -> Vec<u8> {
    let mut w = ObjectWriter::new(big_endian);
    w.pptr(ptr(4))
        .u8(1)
        .align()
        .u32(2)
        .floats(&[0.25, 0.5, 0.75, 1.0])
        .i32(1);
    // Physical camera: gate fit, ISO, four lens floats, blade count, then pairs and floats.
    w.i32(2).i32(200).floats(&[0.005, 16.0, 10.0, 50.0]).i32(5);
    w.floats(&[2.0, 11.0, 0.25, 0.0, 36.0, 24.0, 0.0, 0.0]);
    w.floats(&[0.0, 0.0, 1.0, 1.0])
        .floats(&[0.3, 1_000.0, 60.0])
        .u8(1)
        .align()
        .f32(5.0);
    w.f32(-1.0)
        .u32(0xffff_fffb)
        .i32(-1)
        .pptr(ptr(9))
        .i32(1)
        .i32(0);
    w.bytes(&[1, 0, 1, 0, 1]).align().f32(0.022).f32(0.064);
    w.finish()
}

fn expected_camera() -> Camera {
    Camera {
        game_object: ptr(4),
        enabled: true,
        clear_flags: 2,
        background_color: [0.25, 0.5, 0.75, 1.0],
        projection_matrix_mode: 1,
        physical: PhysicalCamera {
            gate_fit_mode: 2,
            iso: 200,
            shutter_speed: 0.005,
            aperture: 16.0,
            focus_distance: 10.0,
            focal_length: 50.0,
            blade_count: 5,
            curvature: [2.0, 11.0],
            barrel_clipping: 0.25,
            anamorphism: 0.0,
            sensor_size: [36.0, 24.0],
            lens_shift: [0.0, 0.0],
        },
        viewport: [0.0, 0.0, 1.0, 1.0],
        projection: Projection {
            near: 0.3,
            far: 1_000.0,
            field_of_view: 60.0,
            orthographic: true,
            orthographic_size: 5.0,
        },
        depth: -1.0,
        culling_mask: 0xffff_fffb,
        output: CameraOutput {
            rendering_path: -1,
            target_texture: ptr(9),
            target_display: 1,
            target_eye: 0,
            hdr: true,
            allow_msaa: false,
            allow_dynamic_resolution: true,
        },
        force_into_render_texture: false,
        occlusion_culling: true,
        stereo_convergence: 0.022,
        stereo_separation: 0.064,
    }
}

/// A spot light. `flags` turns on, in order: the culling matrix override, color temperature,
/// bounding sphere override and the view frustum caster culling.
fn light_bytes(big_endian: bool, flags: bool) -> Vec<u8> {
    let on = u8::from(flags);
    let mut w = ObjectWriter::new(big_endian);
    w.pptr(ptr(4))
        .u8(1)
        .align()
        .i32(0)
        .floats(&[1.0, 0.9, 0.8, 1.0]);
    w.floats(&[2.0, 10.0, 30.0, 21.0]).floats(&[10.0, 10.0]);
    w.i32(2).i32(-1).i32(512).floats(&[0.75, 0.05, 0.4, 0.2]);
    w.floats(&[3.0; 16]).u8(on).align();
    w.pptr(ptr(7)).u8(1).align();
    w.i32(-1).i32(-1).i32(4).i32(1).u8(0).align();
    w.pptr(ptr(0))
        .i32(1)
        .u32(0xff)
        .u32(1)
        .i32(4)
        .i32(2)
        .floats(&[1.0, 2.0])
        .f32(1.5);
    w.f32(6_500.0)
        .u8(on)
        .align()
        .floats(&[0.0, 1.0, 0.0, 8.0])
        .bytes(&[on, on, 1])
        .align();
    w.i32(1).f32(1.0).u8(1).align();
    w.finish()
}

fn expected_light(flags: bool) -> Light {
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
        cookie_size: [10.0, 10.0],
        shadows: ShadowSettings {
            kind: 2,
            resolution: -1,
            custom_resolution: 512,
            strength: 0.75,
            bias: 0.05,
            normal_bias: 0.4,
            near_plane: 0.2,
            culling_matrix_override: flags.then_some([3.0; 16]),
            use_view_frustum_for_caster_culling: flags,
        },
        cookie: ptr(7),
        draw_halo: true,
        baking: LightBakingOutput {
            probe_occlusion_light_index: -1,
            occlusion_mask_channel: -1,
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
        color_temperature: flags.then_some(6_500.0),
        bounding_sphere_override: flags.then_some([0.0, 1.0, 0.0, 8.0]),
        force_visible: true,
        light_unit: 1,
        lux_at_distance: 1.0,
    }
}

#[test]
fn decodes_cameras_in_both_byte_orders() {
    for big_endian in [false, true] {
        let decoded: Camera =
            decode_one(ClassId::CAMERA, camera_bytes(big_endian), big_endian).unwrap();
        assert_eq!(decoded, expected_camera());
    }
}

#[test]
fn decodes_lights_and_drops_values_their_flags_turn_off() {
    for big_endian in [false, true] {
        for flags in [false, true] {
            let bytes = light_bytes(big_endian, flags);
            let decoded: Light = decode_one(ClassId::LIGHT, bytes, big_endian).unwrap();
            assert_eq!(decoded, expected_light(flags), "flags {flags}");
        }
    }
}

fn every_prefix_fails<T: Layout + std::fmt::Debug>(class: ClassId, bytes: &[u8]) {
    for end in 0..bytes.len() {
        let error = decode_one::<T>(class, bytes[..end].to_vec(), false).unwrap_err();
        assert!(
            matches!(error, SerializedError::Truncated { .. }),
            "{class} prefix {end}: {error}"
        );
    }
}

#[test]
fn every_truncated_camera_or_light_fails() {
    every_prefix_fails::<Camera>(ClassId::CAMERA, &camera_bytes(false));
    every_prefix_fails::<Light>(ClassId::LIGHT, &light_bytes(false, true));
}

#[test]
fn rejects_flag_bytes_that_are_not_booleans() {
    let mut bytes = light_bytes(false, true);
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
