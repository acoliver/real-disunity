//! The shadow query: caster bounds and the shadow maps of each kind of light, through the ABI.
use glam::{Mat4, Quat, Vec3, Vec4};
use worldrunner_disunity_ffi::shadows::*;

const SPOT: u32 = 0;
const DIRECTIONAL: u32 = 1;
const POINT: u32 = 2;

fn cube(centre: Vec3, half: f32, mode: u32) -> DisunityShadowCaster {
    DisunityShadowCaster {
        min_x: centre.x - half,
        min_y: centre.y - half,
        min_z: centre.z - half,
        max_x: centre.x + half,
        max_y: centre.y + half,
        max_z: centre.z + half,
        casting_mode: mode,
        layer: 0,
    }
}

/// A camera at the origin looking down +z, as the engine's matrices have it.
fn camera_matrix() -> Mat4 {
    let (near, far) = (0.3_f32, 100.0_f32);
    let scale = 1.0 / 30.0_f32.to_radians().tan();
    let projection = Mat4::from_cols(
        Vec4::new(scale, 0.0, 0.0, 0.0),
        Vec4::new(0.0, scale, 0.0, 0.0),
        Vec4::new(0.0, 0.0, -(far + near) / (far - near), -1.0),
        Vec4::new(0.0, 0.0, -2.0 * far * near / (far - near), 0.0),
    );
    projection * Mat4::from_scale(Vec3::new(1.0, 1.0, -1.0))
}

fn request(query: u32, light_kind: u32, casters: &[DisunityShadowCaster]) -> DisunityShadowRequest {
    DisunityShadowRequest {
        query,
        light_kind,
        light_range: 20.0,
        light_spot_angle: 60.0,
        shadow_distance: 80.0,
        culling_mask: u32::MAX,
        caster_count: casters.len() as u32,
        split_index: 0,
        split_count: 2,
        ratio_x: 0.5,
        resolution: 1024,
        near_plane_offset: 3.0,
        ..DisunityShadowRequest::default()
    }
}

struct Answer {
    code: i32,
    result: DisunityShadowResult,
    output: Vec<f32>,
}

fn ask(request: &DisunityShadowRequest, light: Mat4, casters: &[DisunityShadowCaster]) -> Answer {
    let mut matrices = light.to_cols_array().to_vec();
    matrices.extend(camera_matrix().to_cols_array());
    let mut result = DisunityShadowResult::default();
    let mut output = vec![0.0_f32; SHADOW_OUTPUT_FLOATS];
    let code = unsafe {
        disunity_shadow_query(
            request,
            matrices.as_ptr(),
            if casters.is_empty() {
                std::ptr::null()
            } else {
                casters.as_ptr()
            },
            &mut result,
            output.as_mut_ptr(),
        )
    }
    .code;
    Answer {
        code,
        result,
        output,
    }
}

fn sun() -> Mat4 {
    let rotation = Quat::from_rotation_y(-30.0_f32.to_radians())
        * Quat::from_rotation_x(50.0_f32.to_radians());
    Mat4::from_rotation_translation(rotation, Vec3::new(0.0, 50.0, 0.0))
}

fn matrix(floats: &[f32]) -> Mat4 {
    Mat4::from_cols_slice(floats)
}

#[test]
fn a_bounds_query_answers_with_the_box_around_the_casters_that_cast() {
    let casters = [
        cube(Vec3::new(0.0, 0.0, 10.0), 1.0, 1),
        cube(Vec3::new(5.0, 0.0, 0.0), 1.0, 0),
    ];
    let answer = ask(
        &request(SHADOW_QUERY_BOUNDS, DIRECTIONAL, &casters),
        sun(),
        &casters,
    );
    assert_eq!(answer.code, 0);
    assert_eq!(answer.result.found, 1);
    assert_eq!(
        [
            answer.result.min_x,
            answer.result.min_y,
            answer.result.min_z
        ],
        [-1.0, -1.0, 9.0]
    );
    assert_eq!(
        [
            answer.result.max_x,
            answer.result.max_y,
            answer.result.max_z
        ],
        [1.0, 1.0, 11.0]
    );
}

#[test]
fn a_query_with_no_casters_that_cast_finds_none() {
    let casters = [cube(Vec3::ZERO, 1.0, 0)];
    let answer = ask(
        &request(SHADOW_QUERY_BOUNDS, DIRECTIONAL, &casters),
        sun(),
        &casters,
    );
    assert_eq!((answer.code, answer.result.found), (0, 0));
    let answer = ask(&request(SHADOW_QUERY_BOUNDS, DIRECTIONAL, &[]), sun(), &[]);
    assert_eq!((answer.code, answer.result.found), (0, 0));
}

#[test]
fn a_directional_query_answers_with_matrices_that_hold_the_caster_and_a_sphere() {
    let centre = Vec3::new(0.0, 0.0, 10.0);
    let casters = [cube(centre, 0.5, 1)];
    let answer = ask(
        &request(SHADOW_QUERY_DIRECTIONAL, DIRECTIONAL, &casters),
        sun(),
        &casters,
    );
    assert_eq!((answer.code, answer.result.found), (0, 1));
    let (view, projection) = (matrix(&answer.output[..16]), matrix(&answer.output[16..32]));
    let culling = matrix(&answer.output[32..48]);
    assert!(culling.abs_diff_eq(projection * view, 1e-4));
    let clip = culling.project_point3(centre);
    assert!(
        clip.x.abs() <= 1.0 && clip.y.abs() <= 1.0 && clip.z.abs() <= 1.0,
        "{clip:?}"
    );
    let sphere = Vec3::new(
        answer.result.sphere_x,
        answer.result.sphere_y,
        answer.result.sphere_z,
    );
    assert!(sphere.distance(centre) <= answer.result.sphere_radius);
    assert!(answer.result.plane_count > 0 && answer.result.plane_count as usize <= 10);
}

#[test]
fn spot_and_point_queries_answer_with_a_perspective_map() {
    let casters = [cube(Vec3::new(0.0, 0.0, 8.0), 0.5, 1)];
    let light = Mat4::from_translation(Vec3::new(0.0, 0.0, -2.0));
    let spot = ask(&request(SHADOW_QUERY_SPOT, SPOT, &casters), light, &casters);
    assert_eq!((spot.code, spot.result.found), (0, 1));
    let projection = matrix(&spot.output[16..32]);
    assert!(
        projection.w_axis.w == 0.0 && projection.z_axis.w == -1.0,
        "perspective"
    );
    let mut face = request(SHADOW_QUERY_POINT, POINT, &casters);
    face.face = 4;
    let point = ask(&face, light, &casters);
    assert_eq!((point.code, point.result.found), (0, 1));
}

#[test]
fn a_refused_query_reports_an_invalid_argument() {
    let casters = [cube(Vec3::new(0.0, 0.0, 10.0), 0.5, 1)];
    let mut bad = request(SHADOW_QUERY_DIRECTIONAL, DIRECTIONAL, &casters);
    bad.split_index = 2;
    assert_ne!(ask(&bad, sun(), &casters).code, 0);
    let mut unknown = request(9, DIRECTIONAL, &casters);
    unknown.query = 9;
    assert_ne!(ask(&unknown, sun(), &casters).code, 0);
    let mut area = request(SHADOW_QUERY_BOUNDS, 3, &casters);
    area.light_kind = 3;
    assert_ne!(ask(&area, sun(), &casters).code, 0);
    let mut sixth = request(SHADOW_QUERY_POINT, POINT, &casters);
    sixth.face = 6;
    assert_ne!(ask(&sixth, Mat4::IDENTITY, &casters).code, 0);
}

#[test]
fn null_pointers_are_refused() {
    let casters = [cube(Vec3::ZERO, 1.0, 1)];
    let request = request(SHADOW_QUERY_BOUNDS, DIRECTIONAL, &casters);
    let matrices = [0.0_f32; 32];
    let mut result = DisunityShadowResult::default();
    let mut output = vec![0.0_f32; SHADOW_OUTPUT_FLOATS];
    unsafe {
        let ok = |code: worldrunner_disunity_ffi::DisunityResult| code.code;
        assert_ne!(
            ok(disunity_shadow_query(
                std::ptr::null(),
                matrices.as_ptr(),
                casters.as_ptr(),
                &mut result,
                output.as_mut_ptr()
            )),
            0
        );
        assert_ne!(
            ok(disunity_shadow_query(
                &request,
                std::ptr::null(),
                casters.as_ptr(),
                &mut result,
                output.as_mut_ptr()
            )),
            0
        );
        assert_ne!(
            ok(disunity_shadow_query(
                &request,
                matrices.as_ptr(),
                std::ptr::null(),
                &mut result,
                output.as_mut_ptr()
            )),
            0
        );
        assert_ne!(
            ok(disunity_shadow_query(
                &request,
                matrices.as_ptr(),
                casters.as_ptr(),
                std::ptr::null_mut(),
                output.as_mut_ptr()
            )),
            0
        );
        assert_ne!(
            ok(disunity_shadow_query(
                &request,
                matrices.as_ptr(),
                casters.as_ptr(),
                &mut result,
                std::ptr::null_mut()
            )),
            0
        );
    }
}
