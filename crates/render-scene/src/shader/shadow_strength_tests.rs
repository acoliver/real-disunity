use super::{configured_frame, evaluate_source};
use crate::device::GpuContext;
use worldrunner_render_lighting::ExposureOwnership;
use worldrunner_render_plan::SceneSpecularOcclusion;

const ENTRY: &str = r"
@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;
@compute @workgroup_size(1) fn evaluate_math() {
    let terms = scene_surface_lighting(vec3(0.0), vec3(0.0,0.0,1.0),
        vec3(0.5), vec3(1.0,0.0,0.25), vec3(0.04));
    result[0] = vec4(terms.diffuse.x, terms.specular.x, published_factor.x, published_environment.x);
}
";

fn reflection_fixture() -> String {
    // Constant texture inputs leave the production SSR publication equations intact.
    include_str!("../ssr/forward.wgsl")
        .replace("f32(environment_intensity.x) / 1000000.0", "1.0")
        .replace(
            "textureSampleLevel(reflection_brdf, reflection_sampler, vec2<f32>(cosine, roughness), 0.0).rg",
            "vec2<f32>(0.5, 0.1)",
        )
        .replace(
            "textureSampleLevel(reflection_environment, reflection_sampler, reflect(-view, normal), roughness * f32(textureNumLevels(reflection_environment) - 1u)).rgb",
            "vec3<f32>(2.0)",
        )
}

fn source(visibility: f32, reduction: f32, screen: f32) -> String {
    let shadow = include_str!("shadows.wgsl")
        .split_once("fn direct_sunlight_visibility")
        .unwrap()
        .1;
    let receiver = format!(
        "var<private> samples: u32; var<private> observed_ao: f32;
         struct ShadowProbe {{ bias: vec4<f32>, }}
         const shadow = ShadowProbe(vec4<f32>(0.0,0.0,0.0,{reduction}));
         fn raw_sunlight_visibility(p: vec3<f32>, n: vec3<f32>) -> f32 {{
             samples += 1u; return {visibility};
         }}
         fn direct_sunlight_visibility{shadow}"
    );
    format!(
        "{}\n{}",
        super::ENVIRONMENT_WGSL
            .replace(include_str!("unshadowed.wgsl"), &receiver)
            .replace("1.0 /* SCREEN_AMBIENT_FACTOR */", &format!("{screen}"))
            .replace(
                "let ao = scene_ao(ambient_occlusion, world_position, normal_dot_light, visibility);",
                "let ao = scene_ao(ambient_occlusion, world_position, normal_dot_light, visibility); observed_ao = ao;",
            )
            .replace(
                "vec3<f32>(0.0); /* SCENE_ENVIRONMENT */",
                "forward_reflection(world_position, normal, f0, environment_roughness, ao, ambient_occlusion);",
            ),
        reflection_fixture()
    )
}

fn expected(visibility: f32, strength: f32, screen: f32) -> [f32; 4] {
    let ao = 0.25_f32.min(screen * 1.01) * (1.0 - visibility) + visibility;
    let specular_ao = ((1.0 + ao).powf(2.0_f32.powi(-17)) - 1.0 + ao).clamp(0.0, 1.0);
    let direct_visibility = 1.0 - strength + visibility * strength;
    let ambient = 0.8 * 0.5 * ao;
    let environment_factor = (0.04 * 0.5 + 0.1) * specular_ao;
    [
        0.96 * 0.5 / std::f32::consts::PI * ao * 3.0 * direct_visibility + ambient,
        0.04 / (4.0 * std::f32::consts::PI) * specular_ao * 3.0 * direct_visibility
            + 2.0 * environment_factor,
        environment_factor,
        2.0 * environment_factor,
    ]
}

fn check_strengths(context: &GpuContext, visibility: f32, screen: f32) {
    let mut frame = configured_frame(ExposureOwnership::SceneLinear(1.0));
    frame.sun_irradiance = glam::Vec3::splat(3.0);
    frame.specular_convention.occlusion = SceneSpecularOcclusion::RoughnessAware;
    let baseline_source = source(visibility, 0.0, screen);
    let baseline = evaluate_source(context, &baseline_source, ENTRY, Some(&frame), None);
    for strength in [1.0, 0.8, 0.5, 0.0] {
        let source = source(visibility, 1.0 - strength, screen);
        let actual = evaluate_source(context, &source, ENTRY, Some(&frame), None);
        for (value, expected) in actual.iter().zip(expected(visibility, strength, screen)) {
            assert!(
                (value - expected).abs() < 1e-6,
                "{actual:?}, strength={strength}"
            );
        }
        assert_eq!(&actual[2..], &baseline[2..], "SSR/environment changed");
        let diagnostics = ENTRY.replace(
            "vec4(terms.diffuse.x, terms.specular.x, published_factor.x, published_environment.x)",
            "vec4(observed_ao, scene_screen_ao(vec3(0.0)), f32(samples), terms.color.x)",
        );
        let values = evaluate_source(context, &source, &diagnostics, Some(&frame), None);
        let ao = 0.25_f32.min(screen * 1.01) * (1.0 - visibility) + visibility;
        assert!((values[0] - ao).abs() < 1e-6, "AO changed: {values:?}");
        assert_eq!(values[1].to_bits(), screen.to_bits());
        assert_eq!(
            values[2].to_bits(),
            1.0_f32.to_bits(),
            "sample raw visibility once per surface"
        );
        assert!((values[3] - actual[0] - actual[1]).abs() < 1e-6);
    }
    let previous = baseline_source
        .replace(
            "let shadow = direct_sunlight_visibility(visibility);",
            "let shadow = sunlight_visibility(world_position, normal);",
        )
        .replace(
            "scene_ao(ambient_occlusion, world_position, normal_dot_light, visibility)",
            "scene_ao(ambient_occlusion, world_position, normal_dot_light, shadow)",
        );
    let previous = evaluate_source(context, &previous, ENTRY, Some(&frame), None);
    assert_eq!(
        baseline.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
        previous.iter().map(|v| v.to_bits()).collect::<Vec<_>>()
    );
}

#[test]
fn roughness_aware_shadow_strength_only_changes_direct_sunlight() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for visibility in [0.0, 0.4] {
        for screen in [1.0, 0.5, 0.1] {
            check_strengths(&context, visibility, screen);
        }
    }
}
