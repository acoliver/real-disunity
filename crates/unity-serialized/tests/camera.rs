#[path = "support/decode.rs"]
mod decode;
mod support;

use decode::decode_one;
use worldrunner_unity_serialized::{
    ClassId, SerializedError,
    camera::{Camera, CameraOutput, PhysicalCamera, Projection},
    object::PPtr,
    write::ObjectWriter,
};

const fn ptr(path_id: i64) -> PPtr {
    PPtr {
        file_id: 0,
        path_id,
    }
}

/// The five flags after the target eye, in serialized order: HDR, MSAA, dynamic
/// resolution, forced render texture and occlusion culling.
const OUTPUT_FLAGS: [u8; 5] = [1, 0, 1, 0, 1];

fn camera_bytes(big_endian: bool, flags: [u8; 5]) -> Vec<u8> {
    let mut w = ObjectWriter::new(big_endian);
    w.pptr(ptr(4))
        .u8(1)
        .align()
        .u32(2)
        .floats(&[0.25, 0.5, 0.75, 1.0])
        .i32(1);
    // Physical camera: gate fit, ISO, four lens floats, blade count, then pairs and floats.
    w.i32(2).i32(200).floats(&[0.005, 16.0, 10.0, 50.0]).i32(5);
    w.floats(&[2.0, 11.0, 0.25, 0.5, 36.0, 24.0, 0.1, -0.1]);
    w.floats(&[0.1, 0.2, 0.7, 0.8])
        .floats(&[0.3, 1_000.0, 60.0])
        .u8(1)
        .align()
        .f32(5.0);
    w.f32(-1.0)
        .u32(0xffff_fffb)
        .i32(-1)
        .pptr(ptr(9))
        .i32(1)
        .i32(2);
    w.bytes(&flags).align().f32(0.022).f32(0.064);
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
            anamorphism: 0.5,
            sensor_size: [36.0, 24.0],
            lens_shift: [0.1, -0.1],
        },
        viewport: [0.1, 0.2, 0.7, 0.8],
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
            target_eye: 2,
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

#[test]
fn decodes_cameras_in_both_byte_orders() {
    for big_endian in [false, true] {
        let bytes = camera_bytes(big_endian, OUTPUT_FLAGS);
        let decoded: Camera = decode_one(ClassId::CAMERA, bytes, big_endian).unwrap();
        assert_eq!(decoded, expected_camera());
    }
}

#[test]
fn each_output_flag_decodes_to_its_own_field() {
    for set in 0..5 {
        let flags: [u8; 5] = std::array::from_fn(|i| u8::from(i == set));
        let camera: Camera =
            decode_one(ClassId::CAMERA, camera_bytes(false, flags), false).unwrap();
        let output = &camera.output;
        let decoded = [
            output.hdr,
            output.allow_msaa,
            output.allow_dynamic_resolution,
            camera.force_into_render_texture,
            camera.occlusion_culling,
        ];
        assert_eq!(decoded.map(u8::from), flags, "flag {set}");
    }
}

#[test]
fn every_truncated_camera_fails() {
    let bytes = camera_bytes(false, OUTPUT_FLAGS);
    for end in 0..bytes.len() {
        let error =
            decode_one::<Camera>(ClassId::CAMERA, bytes[..end].to_vec(), false).unwrap_err();
        assert!(
            matches!(error, SerializedError::Truncated { .. }),
            "prefix {end}: {error}"
        );
    }
}
