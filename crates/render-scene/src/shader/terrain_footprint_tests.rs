use super::gpu_math_tests::{configured_frame, evaluate_source};
use glam::{Mat4, Vec2, Vec3, Vec4};

fn plane_position(matrix: Mat4, pixel: Vec2, normal: Vec3, depth: f32) -> Vec3 {
    let ndc = Vec2::new(pixel.x / 64.0 - 1.0, 1.0 - pixel.y / 32.0);
    let inverse = matrix.inverse();
    let near = inverse.project_point3(ndc.extend(0.0));
    let far = inverse.project_point3(ndc.extend(1.0));
    let ray = far - near;
    near + ray * (normal.dot(Vec3::new(0.0, 0.0, -depth) - near) / normal.dot(ray))
}

fn expected(matrix: Mat4, pixel: Vec2, normal: Vec3, depth: f32) -> [f32; 4] {
    let left = Vec2::new((pixel.x / 2.0).floor() * 2.0 + 0.5, pixel.y);
    let top = Vec2::new(pixel.x, (pixel.y / 2.0).floor() * 2.0 + 0.5);
    let dx = plane_position(matrix, left + Vec2::X, normal, depth)
        - plane_position(matrix, left, normal, depth);
    let dy = plane_position(matrix, top + Vec2::Y, normal, depth)
        - plane_position(matrix, top, normal, depth);
    [dx.x, dx.y, dy.x, dy.y]
}

#[test]
fn terrain_analytic_footprint_matches_pixel_pairs_on_flat_and_sloped_planes() {
    let context = pollster::block_on(crate::device::GpuContext::request_with_features(
        wgpu::Features::empty(),
    ))
    .unwrap();
    let mut frame = configured_frame(worldrunner_render_lighting::ExposureOwnership::SceneLinear(
        1.0,
    ));
    frame.raster_viewport = [128, 64];
    for matrix in [
        glam::camera::rh::proj::directx::perspective(
            std::f32::consts::FRAC_PI_2,
            2.0,
            0.1,
            1_000.0,
        ),
        glam::camera::rh::proj::directx::orthographic(-160.0, 160.0, -80.0, 80.0, 0.1, 1_000.0),
    ] {
        frame.view_projection = matrix;
        for normal in [
            Vec3::Z,
            Vec3::new(0.3, -0.5, 1.0).normalize(),
            Vec3::Z * 1e-7,
            Vec3::new(0.3, -0.5, 1.0) * 100.0,
        ] {
            for depth in [25.0, 250.0] {
                for pixel in [Vec2::new(40.5, 26.5), Vec2::new(41.5, 27.5)] {
                    check(&context, &frame, pixel, normal, depth);
                }
            }
        }
    }
}

fn check(
    context: &crate::device::GpuContext,
    frame: &crate::frame::FrameUniform,
    pixel: Vec2,
    normal: Vec3,
    depth: f32,
) {
    let position = plane_position(frame.view_projection, pixel, normal, depth);
    let clip = frame.view_projection * position.extend(1.0);
    let fragment = Vec4::new(pixel.x, pixel.y, clip.z / clip.w, 1.0 / clip.w);
    let entry = format!(
        "@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;\n\
         @compute @workgroup_size(1) fn evaluate_math() {{\n\
         let fragment = vec4<f32>({}, {}, {}, {});\n\
         let normal = vec3<f32>({}, {}, {});\n\
         result[0] = vec4<f32>(terrain_quad_footprint(fragment, vec2<f32>(1.0, 0.0), normal),\n\
         terrain_quad_footprint(fragment, vec2<f32>(0.0, 1.0), normal)); }}",
        fragment.x, fragment.y, fragment.z, fragment.w, normal.x, normal.y, normal.z,
    );
    let result = evaluate_source(context, super::TERRAIN_WGSL, &entry, Some(frame), None);
    for (actual, expected) in
        result
            .iter()
            .zip(expected(frame.view_projection, pixel, normal, depth))
    {
        assert!(
            (actual - expected).abs() < 2e-4,
            "{pixel:?} {normal:?} {depth}: {actual} != {expected}"
        );
    }
}

fn analytic_cpu(matrix: Mat4, fragment: Vec4, axis: Vec2, normal: Vec3) -> Vec2 {
    let size = Vec2::new(128.0, 64.0);
    let lane = (fragment.truncate().truncate().floor().as_uvec2() & glam::UVec2::ONE)
        .as_vec2()
        .dot(axis);
    let direction = 1.0 - 2.0 * lane;
    let delta = axis * Vec2::new(2.0 / size.x, -2.0 / size.y) * direction;
    let ndc = Vec2::new(
        fragment.x / size.x * 2.0 - 1.0,
        1.0 - fragment.y / size.y * 2.0,
    ) + delta;
    let rows = matrix.transpose();
    let x = rows.x_axis.truncate() - ndc.x * rows.w_axis.truncate();
    let y = rows.y_axis.truncate() - ndc.y * rows.w_axis.truncate();
    let tangent_x = y.cross(normal);
    let tangent_y = normal.cross(x);
    let determinant = x.dot(tangent_x);
    let floor = x.cross(y).length() * normal.length() * 1e-5;
    let safe = determinant.signum() * determinant.abs().max(floor);
    let displacement = (delta.x * tangent_x + delta.y * tangent_y) / (fragment.w * safe);
    (displacement.truncate() * direction).clamp(Vec2::splat(-1e4), Vec2::splat(1e4))
}

#[test]
fn terrain_cpu_footprint_matches_ray_plane_intersections_for_both_projections() {
    let perspective = glam::camera::rh::proj::directx::perspective(
        std::f32::consts::FRAC_PI_2,
        2.0,
        0.1,
        1_000.0,
    );
    let orthographic =
        glam::camera::rh::proj::directx::orthographic(-160.0, 160.0, -80.0, 80.0, 0.1, 1_000.0);
    for projection in [perspective, orthographic] {
        for normal in [Vec3::Z, Vec3::new(0.3, -0.5, 1.0)] {
            for pixel in [Vec2::new(40.5, 26.5), Vec2::new(41.5, 27.5)] {
                for depth in [25.0, 250.0] {
                    let point = plane_position(projection, pixel, normal, depth);
                    let clip = projection * point.extend(1.0);
                    let fragment = Vec4::new(pixel.x, pixel.y, clip.z / clip.w, clip.w.recip());
                    let dx = analytic_cpu(projection, fragment, Vec2::X, normal);
                    let dy = analytic_cpu(projection, fragment, Vec2::Y, normal);
                    for (actual, expected) in [dx.x, dx.y, dy.x, dy.y]
                        .into_iter()
                        .zip(expected(projection, pixel, normal, depth))
                    {
                        assert!((actual - expected).abs() < 2e-4);
                    }
                }
            }
        }
    }
}
