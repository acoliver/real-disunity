use super::*;

#[test]
fn sky_depth_tests_without_writing() {
    let _session = crate::test_support::SceneTestSession::acquire();
    assert_eq!(
        depth_state(crate::renderer::depth::DepthMode::Independent).depth_write_enabled,
        Some(false)
    );
    assert_eq!(
        depth_state(crate::renderer::depth::DepthMode::Independent).depth_compare,
        Some(wgpu::CompareFunction::LessEqual)
    );
}

#[test]
fn sky_shader_is_valid_and_exposure_is_owned_by_the_shared_frame() {
    let _session = crate::test_support::SceneTestSession::acquire();
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
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = crate::device::GpuContext::shared_for_test().unwrap();
    for samples in crate::msaa_tests::supported_samples(&context) {
        let scope = context
            .device()
            .push_error_scope(wgpu::ErrorFilter::Validation);
        let frame = crate::upload::frame_bind_group_layout(context.device());
        let _ = pipeline(
            context.device(),
            &frame,
            ColorTarget {
                format: wgpu::TextureFormat::Rgba16Float,
                samples,
            },
            Variant::independent(false),
        );
        assert!(pollster::block_on(scope.pop()).is_none());
    }
}
