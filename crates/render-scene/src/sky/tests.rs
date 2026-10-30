use super::*;
use glam::Vec3;

#[test]
fn angular_disc_orientation_and_occlusion_contract() {
    let center = Vec3::Z;
    assert_eq!(disc_uv(center, center, 0.1, 0.0), Some([0.5, 0.5]));
    assert!(disc_uv(Vec3::X, center, 0.1, 0.0).is_none());
    assert!(disc_uv(-center, center, 0.1, 0.0).is_none());
    let ray = Vec3::new(0.05, 0.0, 1.0).normalize();
    let a = disc_uv(ray, center, 0.1, 0.0).unwrap();
    let b = disc_uv(ray, center, 0.1, std::f32::consts::FRAC_PI_2).unwrap();
    assert!((a[0] - 0.5).abs() < 1e-6);
    assert!((b[1] - 0.5).abs() < 1e-6);
    assert!((a[1] - 0.5).abs() > 0.1);
    assert_eq!(depth_state().depth_write_enabled, Some(false));
    assert_eq!(
        depth_state().depth_compare,
        Some(wgpu::CompareFunction::LessEqual)
    );
}

#[test]
fn sky_shader_is_valid_and_exposure_is_owned_by_the_shared_frame() {
    let module = naga::front::wgsl::parse_str(SOURCE).unwrap();
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
    assert_eq!(SOURCE.matches("* frame.grading.x").count(), 1);
}

#[test]
fn sky_pipeline_compiles_for_hdr_and_msaa() {
    let context = pollster::block_on(crate::device::GpuContext::request()).unwrap();
    for samples in crate::msaa_tests::supported_samples(&context) {
        let scope = context
            .device()
            .push_error_scope(wgpu::ErrorFilter::Validation);
        let frame = crate::upload::frame_bind_group_layout(context.device());
        let _ = pipeline(
            context.device(),
            &frame,
            (wgpu::TextureFormat::Rgba16Float, samples, false),
            false,
        );
        assert!(pollster::block_on(scope.pop()).is_none());
    }
}
