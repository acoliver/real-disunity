use super::*;
use worldrunner_render_plan::{SceneWaterSeedCoordinates, SceneWaterSeedSampling};

#[test]
fn fragment_seed_stage_keeps_original_shader_source_and_pipeline_keys() {
    let source = water_source(false);
    assert!(matches!(super::super::vertex_seeds::specialize(&source, 0),
        std::borrow::Cow::Borrowed(original) if original.as_ptr() == source.as_ptr()));
    assert_eq!(
        super::super::vertex_seeds::specialize(&source, endpoint::SPECIALIZATION),
        endpoint::specialize(&source, endpoint::SPECIALIZATION)
    );
    for mode in [
        SceneWaterSeedCoordinates::Repeat,
        SceneWaterSeedCoordinates::EndpointClamped,
    ] {
        let mut body = seed_tests::body(mode);
        let key = if endpoint::enabled(&body) {
            endpoint::SPECIALIZATION
        } else {
            0
        };
        assert_eq!(super::super::vertex_seeds::key(&body), key);
        body.material.flow_seed_sampling = SceneWaterSeedSampling::Vertex;
        assert_eq!(
            super::super::vertex_seeds::key(&body),
            key | super::super::vertex_seeds::SPECIALIZATION
        );
    }
}

#[test]
fn vertex_seed_stage_requires_flow_and_preserves_fragment_defaults() {
    assert_eq!(
        SceneWaterSeedSampling::default(),
        SceneWaterSeedSampling::Fragment
    );
    let mut body = settings();
    assert_eq!(
        body.material.flow_seed_sampling,
        SceneWaterSeedSampling::Fragment
    );
    body.material.flow_seed_sampling = SceneWaterSeedSampling::Vertex;
    assert!(body.validate().is_err());
    body.material.flow = Some(worldrunner_render_plan::TextureHandle::from_raw(0));
    assert!(body.validate().is_ok());
    let frame = FrameUniform::new(&fixture_view(), None).unwrap();
    let vertex = uniform_data(&frame, 0.5, &body);
    body.material.flow_seed_sampling = SceneWaterSeedSampling::Fragment;
    assert_eq!(vertex, uniform_data(&frame, 0.5, &body));
    for mode in [
        SceneWaterSeedCoordinates::Repeat,
        SceneWaterSeedCoordinates::EndpointClamped,
    ] {
        let mut body = seed_tests::body(mode);
        let fragment = geometry::allocation_sizes(&body);
        body.material.flow_seed_sampling = SceneWaterSeedSampling::Vertex;
        assert_eq!(geometry::allocation_sizes(&body), fragment);
        assert!(body.validate().is_ok());
    }
}

#[test]
fn vertex_seed_shader_decodes_and_shapes_after_interpolation() {
    let source = super::super::vertex_seeds::source(&water_source(false), false);
    let vertex = source.split("@fragment fn fs_water").next().unwrap();
    let fragment = source.split("@fragment fn fs_water").nth(1).unwrap();
    assert!(vertex.contains("output.flow_seed = bilinear_rg(surface_flow, seed_uv);"));
    assert!(source.contains("@interpolate(perspective, sample) flow_seed: vec2<f32>"));
    assert!(fragment.contains("input.flow_seed * 2.0 - 1.0"));
    assert!(fragment.contains("water_wind_seed(seed, input.wind)"));
    assert!(fragment.contains("velocity *= water_flow_at_depth(shore_depth)"));
}
