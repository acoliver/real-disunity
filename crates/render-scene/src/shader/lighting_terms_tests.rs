use super::gpu_math_tests::{configured_frame, evaluate_source};
use crate::device::GpuContext;
use worldrunner_render_lighting::ExposureOwnership;
use worldrunner_render_plan::{SceneDirectDiffuse, SceneSpecularOcclusion};

const ENTRY: &str = r"
@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;
@compute @workgroup_size(1) fn evaluate_math() {
    let terms = scene_surface_lighting(vec3(0.0), vec3(0.0,0.0,1.0),
        vec3(0.3,0.5,0.7), vec3(0.25,0.0,0.4), vec3(0.04));
    result[0] = vec4(terms.color, 1.0);
}";

#[test]
fn named_lighting_terms_preserve_main_lit_float_bytes() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for source in [super::ENVIRONMENT_WGSL, super::TERRAIN_WGSL] {
        let baseline = source.replace(
            include_str!("lighting.wgsl"),
            include_str!("lighting_terms_base.wgsl"),
        );
        for occlusion in [
            SceneSpecularOcclusion::default(),
            SceneSpecularOcclusion::RoughnessAware,
        ] {
            for diffuse in [
                SceneDirectDiffuse::default(),
                SceneDirectDiffuse::LambertianPrepared,
            ] {
                let mut frame = configured_frame(ExposureOwnership::SceneLinear(1.0));
                frame.specular_convention.occlusion = occlusion;
                frame.specular_convention.direct_diffuse = diffuse;
                let expected = evaluate_source(&context, &baseline, ENTRY, Some(&frame), None);
                let actual = evaluate_source(&context, source, ENTRY, Some(&frame), None);
                assert_eq!(
                    actual.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                    expected.iter().map(|v| v.to_bits()).collect::<Vec<_>>()
                );
            }
        }
    }
}

#[test]
fn named_terms_share_ambient_ao_and_nonzero_environment_math() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let source = super::ENVIRONMENT_WGSL.replace(
        "vec3<f32>(0.0); /* SCENE_ENVIRONMENT */",
        "vec3<f32>(0.2,0.3,0.4);",
    );
    let entry = ENTRY.replace("vec4(terms.color, 1.0)",
        "vec4(terms.direct_diffuse + terms.ambient_diffuse + terms.environment_specular + terms.sun_specular - terms.color, terms.environment_specular.z)");
    let frame = configured_frame(ExposureOwnership::SceneLinear(1.0));
    let actual = evaluate_source(&context, &source, &entry, Some(&frame), None);
    assert!(actual[..3].iter().all(|value| value.abs() < 1e-6));
    assert!((actual[3] - 0.4).abs() < 1e-6);
}
