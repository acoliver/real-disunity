use super::{ENVIRONMENT_WGSL, gpu_math_tests::evaluate_source};
use crate::{device::GpuContext, frame::FrameUniform};
use glam::Vec3;
use worldrunner_render_lighting::atmosphere::HeightFog;

// Midpoint quadrature is independent of the shader's closed-form logarithmic integral.
fn fog_oracle(eye: Vec3, offset: Vec3, start: f64) -> f64 {
    let mut density = 0.0;
    let delta = if (0.03 * f64::from(offset.z)).abs() < 0.0001 {
        -0.0001 / 0.03
    } else {
        f64::from(offset.z)
    };
    for sample in 0..4096 {
        let t = (f64::from(sample) + 0.5) / 4096.0;
        let height = f64::from(eye.z) + delta * t;
        density += 0.02 * (-0.03 * height).exp() / 4096.0;
    }
    (-density * (f64::from(offset.length()) - start).max(0.0)).exp()
}

#[test]
fn full_ray_fog_matches_quadrature_for_ascending_descending_and_noncentral_rays() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for offset in [
        Vec3::new(0.0, 80.0, 40.0),
        Vec3::new(0.0, 80.0, -40.0),
        Vec3::new(40.0, 80.0, 10.0),
        Vec3::new(0.0, 80.0, 0.0),
    ] {
        let eye = Vec3::Z * 20.0;
        let mut fog = HeightFog::new(0.02, 0.03, 0.0, 25.0, Vec3::ZERO).unwrap();
        assert!(!fog.full_ray_density);
        fog.full_ray_density = true;
        let expected = fog_oracle(eye, offset, 25.0);
        assert!((f64::from(fog.transmittance(eye, offset, Vec3::Z)) - expected).abs() < 2e-6);
        let entry = format!(
            r"
@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;
@compute @workgroup_size(1) fn evaluate_math() {{
    let ray=vec3<f32>({},{},{});
    result[0]=vec4(rd_full_ray_fog_transmittance(log(0.02)-0.6,0.03,25.0,ray,vec3(0.0,0.0,1.0)),0.0,0.0,1.0);
}}",
            offset.x, offset.y, offset.z
        );
        let actual = evaluate_source(&context, ENVIRONMENT_WGSL, &entry, None, None);
        assert!(
            (f64::from(actual[0]) - expected).abs() < 2e-5,
            "{offset}: {actual:?} {expected}"
        );
    }
}

fn frame() -> FrameUniform {
    let view = crate::view::SceneView::new(
        crate::view::CameraPose {
            eye_millimeters: [0, 0, 0],
            target_millimeters: [0, 1000, 0],
        },
        worldrunner_render_plan::SceneCamera {
            vertical_fov_millidegrees: 60_000,
            near_plane_millimeters: 100,
            far_plane_millimeters: 100_000,
        },
        crate::view::Viewport {
            width: 3,
            height: 3,
        },
    )
    .unwrap();
    FrameUniform::unlit(&view)
}

#[test]
fn far_plane_background_matches_forward_depth_for_noncentral_and_sloped_rays() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let frame = frame();
    for direction in [
        Vec3::Y,
        Vec3::new(0.0, 1.0, 0.5),
        Vec3::new(0.0, 1.0, -0.5),
        Vec3::new(0.4, 1.0, 0.3),
    ] {
        let ray = direction.normalize();
        let expected = 100.0 / f64::from(ray.y);
        let entry = format!(
            r"
@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;
@compute @workgroup_size(1) fn evaluate_math() {{
    result[0]=vec4(rd_far_plane_ray_distance(frame.view_projection,
        normalize(vec3<f32>({},{},{})),100.0),0.0,0.0,1.0);
}}",
            direction.x, direction.y, direction.z
        );
        let actual = evaluate_source(&context, ENVIRONMENT_WGSL, &entry, Some(&frame), None);
        // Normalization and forward alignment each contribute single-precision rounding.
        assert!((f64::from(actual[0]) - expected).abs() < expected * 2e-6);
    }
}

#[test]
fn default_fog_uniform_and_shader_result_are_identical() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let mut frame = frame();
    frame.height_fog = Some(HeightFog::new(0.02, 0.03, 0.0, 25.0, Vec3::ZERO).unwrap());
    assert_eq!(&frame.to_bytes()[308..320], &[0; 12]);
    let entry = r"
@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;
@compute @workgroup_size(1) fn evaluate_math() {
    let position=vec3(20.0,80.0,40.0);
    let legacy=rd_height_fog_transmittance(frame.camera_position.w,-frame.sun_direction.w-1.0,
        frame.sun_irradiance.w,position,vec3(0.0,0.0,1.0));
    result[0]=vec4(apply_height_fog(vec3(1.0),position).x,legacy,0.0,1.0);
}";
    let actual = evaluate_source(&context, ENVIRONMENT_WGSL, entry, Some(&frame), None);
    assert_eq!(actual[0].to_bits(), actual[1].to_bits());
}
