use super::*;
use worldrunner_render_plan::{SceneDirectDiffuse, SceneSpecularConvention, SceneWaterSunDisk};
use worldrunner_render_plan::{SceneWaterShadowOffset, SceneWaterShadowResponse};

#[test]
fn water_shadow_lookup_preserves_roughness_aware_ambient_visibility() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let (mut body, mut frame) = response_fixture(None, true);
    body.material.lighting_normal_micro = Some(1_000_000);
    frame.ambient_sky = glam::Vec3::ONE;
    frame.ambient_ground = glam::Vec3::ONE;
    frame.sun_irradiance = glam::Vec3::splat(3.0);
    frame.specular_convention.occlusion =
        worldrunner_render_plan::SceneSpecularOcclusion::RoughnessAware;
    let source = water_source(false).replace(
        "fn raw_sunlight_visibility(_position: vec3<f32>, _normal: vec3<f32>) -> f32 {\n    return 1.0;",
        "fn raw_sunlight_visibility(_position: vec3<f32>, _normal: vec3<f32>) -> f32 {\n    return select(0.2, 0.8, _normal.x > 0.001);")
        .replace("1.0 /* SCREEN_AMBIENT_FACTOR */", "0.25")
        .replace("return SceneSurfaceLight(color, diffuse_radiance, specular_radiance,\n        direct_diffuse, ambient, environment, sun_specular);",
            "return SceneSurfaceLight(ambient, ambient, vec3<f32>(0.0), vec3<f32>(0.0), ambient, vec3<f32>(0.0), vec3<f32>(0.0));");
    let baseline = response_render_source(&context, body.clone(), &frame, &source);
    body.material.shadow_response = Some(SceneWaterShadowResponse {
        offset: SceneWaterShadowOffset::GeometricNormal,
        diffuse_floor_micro: 1_000_000,
        depth_relief: true,
        ..Default::default()
    });
    let actual = response_render_source(&context, body, &frame, &source);
    assert_eq!(baseline, actual, "direct receiver response changed ambient");
}

#[test]
fn water_shadow_relief_reuses_tint_or_refraction_depth_weight() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for tinted in [false, true] {
        let enabled = u8::from(tinted);
        let source = format!(
            r"
struct TestWater {{ disk: vec4<f32>, wind_flags: vec4<f32>, tint_deep: vec4<f32>, tint_controls: vec4<f32> }}
const water = TestWater(vec4<f32>(0.0,0.0,0.0,1.0), vec4<f32>(0.0,0.0,0.3,1.0),
 vec4<f32>(0.0,0.0,0.0,{enabled}.0), vec4<f32>(1.0,2.0,0.25,-0.125));
{}
{}
{}
@group(0) @binding(0) var<storage, read_write> output: array<vec4<f32>>;
@compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id: vec3<u32>) {{
 let depth = array<f32, 5>(0.0,0.0752575,0.150515,0.5,1.0)[id.x];
 let weight = water_shadow_depth(depth);
 output[id.x * 2u] = vec4<f32>(weight, water_shadow_response(0.2, weight), 0.0, 0.0);
}}",
            include_str!("optics.wgsl"),
            include_str!("shadow_lookup.wgsl"),
            include_str!("shadow_response.wgsl")
        );
        let values = optics_tests::compute(&context, &source);
        for (i, depth) in [0.0f64, 0.075_257_5, 0.150_515, 0.5, 1.0]
            .into_iter()
            .enumerate()
        {
            let weight = if tinted {
                ((100.0f64.powf(-depth) - 0.25).clamp(0.0, 1.0) - 0.125).clamp(0.0, 1.0)
            } else {
                depth
            };
            assert!((f64::from(values[i * 8]) - weight).abs() < 0.000_001);
            assert!((f64::from(values[i * 8 + 1]) - (0.3 + 0.7 * weight)).abs() < 0.000_001);
        }
    }
}

#[test]
fn water_response_reaches_both_lighting_paths_without_changing_snapshots() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for scene_path in [false, true] {
        let baseline = response_render(&context, None, scene_path);
        let neutral = response_render(
            &context,
            Some(SceneWaterShadowResponse::default()),
            scene_path,
        );
        assert_eq!(baseline, neutral);
        for relief in [false, true] {
            let response = SceneWaterShadowResponse {
                diffuse_floor_micro: 600_000,
                depth_relief: relief,
                ..Default::default()
            };
            let actual = response_render(&context, Some(response), scene_path);
            assert_eq!(actual[1], baseline[1], "opaque refraction snapshot changed");
            let offset = (16 * 32 + 16) * 8;
            let value = numerical::positive_half(&actual[0][offset..offset + 2]);
            let expected = if relief { 0.5 } else { 0.3 };
            assert!(
                (value - expected).abs() < 0.0005,
                "scene={scene_path}: {value}"
            );
        }
    }
}

#[test]
fn water_clipped_disk_and_shadow_strength_match_cpu_in_both_lighting_paths() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for scene_path in [false, true] {
        for post in [false, true] {
            let response = SceneWaterShadowResponse {
                diffuse_floor_micro: 600_000,
                disk_raw_visibility: post,
                ..Default::default()
            };
            let (mut body, frame) = response_fixture(Some(response), scene_path);
            body.material.sun_disk = Some(SceneWaterSunDisk {
                amplitude_micro: 1_000_000,
                peak_micro: 1_000_000,
                ..Default::default()
            });
            let source = disk_response_source();
            let actual = response_render_source(&context, body, &frame, &source);
            let adjusted = 1.0 - (1.0 - 0.4) * 0.65;
            let (visibility, disk) = response_oracle(adjusted, 0.6, false, post);
            assert_eq!(
                disk.to_bits(),
                (if post { adjusted } else { 1.0 }).to_bits(),
                "disk must clip before the post multiply"
            );
            let expected = 0.5 * visibility + disk;
            let offset = (16 * 32 + 16) * 8;
            let value = f64::from(numerical::positive_half(&actual[0][offset..offset + 2]));
            assert!(
                (value - expected).abs() < 0.001,
                "scene={scene_path}, post={post}: {value} vs {expected}"
            );
        }
    }
}

fn disk_response_source() -> String {
    // Keep production strength adjustment and call sites; hold the BRDF at the
    // same positive value as the equation oracle to remove angular uncertainty.
    let strength = include_str!("../shader/shadows.wgsl")
        .split("fn direct_sunlight_visibility")
        .nth(1)
        .unwrap()
        .split("fn sunlight_visibility")
        .next()
        .unwrap()
        .replace("shadow.bias.w", "0.35");
    let source = water_source(false).replace(
        "fn raw_sunlight_visibility(_position: vec3<f32>, _normal: vec3<f32>) -> f32 {\n    return 1.0;",
        "fn raw_sunlight_visibility(_position: vec3<f32>, _normal: vec3<f32>) -> f32 {\n    return 0.4;")
        .replace("fn direct_sunlight_visibility(visibility: f32) -> f32 {\n    return visibility;\n}",
            &format!("fn direct_sunlight_visibility{strength}"))
        .replace("fn scene_specular_brdf(", "fn unused_specular_brdf(");
    format!(
        "{source}\nfn scene_specular_brdf(d: vec4<f32>, r: f32, f: vec3<f32>) -> vec3<f32> {{ return vec3<f32>(2.0); }}"
    )
}

fn response_oracle(raw: f64, floor: f64, relief: bool, post: bool) -> (f64, f64) {
    let floored = raw.max(floor);
    let visibility = if relief { floored * 0.6 + 0.4 } else { floored };
    let disk = (2.0 * visibility).min(1.0) * if post { raw } else { 1.0 };
    (visibility, disk)
}

fn response_render(
    context: &GpuContext,
    response: Option<SceneWaterShadowResponse>,
    scene_path: bool,
) -> Vec<Vec<u8>> {
    let (body, frame) = response_fixture(response, scene_path);
    let source = water_source(false).replace(
        "fn raw_sunlight_visibility(_position: vec3<f32>, _normal: vec3<f32>) -> f32 {\n    return 1.0;",
        "fn raw_sunlight_visibility(_position: vec3<f32>, _normal: vec3<f32>) -> f32 {\n    return 0.4;");
    response_render_source(context, body, &frame, &source)
}

fn response_render_source(
    context: &GpuContext,
    body: SceneWaterBody,
    frame: &FrameUniform,
    source: &str,
) -> Vec<Vec<u8>> {
    let mut renderer = regressions::renderer(context, 1);
    renderer.install_water(context.device(), body).unwrap();
    let water = renderer.water.as_mut().unwrap();
    let base = frame.specular_convention.shader_options();
    let options = [
        base[0],
        base[1],
        base[2],
        base[3],
        terms::mask(&water.settings),
    ];
    let pipeline = pipeline_with_source(context.device(), 1, &water.layout, false, options, source);
    water
        .specializations
        .borrow_mut()
        .insert(options, [pipeline.clone(), pipeline]);
    let color = AttachmentView::new(
        ResourceRef::new("scene color"),
        renderer.targets().scene().texture(),
    )
    .unwrap();
    let depth = AttachmentView::new(
        ResourceRef::new("scene depth"),
        renderer.targets().depth().texture(),
    )
    .unwrap();
    encode_fixture(context, &mut renderer, (&color, &depth), frame, 0.5)
        .finish()
        .submit()
        .unwrap();
    vec![
        read(context, renderer.targets().texture()),
        read(
            context,
            &renderer.water.as_ref().unwrap().snapshots[0].color,
        ),
    ]
}

fn response_fixture(
    response: Option<SceneWaterShadowResponse>,
    scene_path: bool,
) -> (SceneWaterBody, FrameUniform) {
    let mut body = settings();
    body.color_micro = [500_000; 3];
    body.material.direct_surface = true;
    body.material.lighting_normal_micro = Some(0);
    body.material.sun_disk = Some(SceneWaterSunDisk {
        amplitude_micro: 0,
        ..Default::default()
    });
    body.material.shadow_response = response;
    let mut frame = FrameUniform::new(&fixture_view(), None).unwrap();
    frame.unlit = false;
    frame.sun_direction = glam::Vec3::Z;
    frame.sun_irradiance = glam::Vec3::ONE;
    frame.ambient_sky = glam::Vec3::ZERO;
    frame.ambient_ground = glam::Vec3::ZERO;
    if scene_path {
        frame.specular_convention = SceneSpecularConvention {
            direct_diffuse: SceneDirectDiffuse::LambertianPrepared,
            ..Default::default()
        };
    }
    (body, frame)
}

#[test]
fn shadow_response_validates_and_default_preserves_bytes_and_keys() {
    let mut body = settings();
    let frame = FrameUniform::new(&fixture_view(), None).unwrap();
    let original = uniform_data(&frame, 0.0, &body);
    assert!(body.material.shadow_response.is_none());
    assert_eq!(terms::mask(&[body.clone()]), 0);
    let source = water_source(true);
    assert!(source.contains("scene_surface_lighting_with_visibility"));
    body.material.shadow_response = Some(SceneWaterShadowResponse::default());
    assert_eq!(uniform_data(&frame, 0.0, &body), original);
    assert_eq!(terms::mask(&[body.clone()]), 0);
    for floor in [0, 500_000, 1_000_000, 1_000_001, u32::MAX] {
        body.material.shadow_response = Some(SceneWaterShadowResponse {
            diffuse_floor_micro: floor,
            offset: SceneWaterShadowOffset::GeometricNormal,
            ..Default::default()
        });
        assert_eq!(body.validate().is_ok(), floor <= 1_000_000);
    }
}

#[test]
fn shadow_response_floor_relief_and_disk_match_independent_cpu() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for relief in [false, true] {
        for post in [false, true] {
            let source = format!(
                "{}\n{}",
                response_equations(relief, post),
                r"
@group(0) @binding(0) var<storage, read_write> output: array<vec4<f32>>;
@compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id: vec3<u32>) {
 let s = array<f32, 5>(0.0, 0.25, 0.5, 0.75, 1.0)[id.x];
 let v = water_shadow_response(s, 0.4);
 let disk = water_shadow_disk(water_disk_highlight(vec3<f32>(0.0,0.0,1.0), vec3<f32>(0.0,0.0,1.0), vec3<f32>(0.0,0.0,1.0), vec3<f32>(2.0), 0.1, v), s);
 output[id.x * 2u] = vec4<f32>(v, disk.x, 0.0, 0.0);
}"
            );
            let actual = optics_tests::compute(&context, &source);
            for (i, raw) in [0.0f64, 0.25, 0.5, 0.75, 1.0].into_iter().enumerate() {
                let (expected, disk) = response_oracle(raw, 0.3, relief, post);
                assert!((f64::from(actual[i * 8]) - expected).abs() < 0.000_001);
                assert!((f64::from(actual[i * 8 + 1]) - disk).abs() < 0.000_001);
            }
        }
    }
}

fn response_equations(relief: bool, post: bool) -> String {
    let flags = u8::from(relief) + 2 * u8::from(post);
    let source = terms_tests::equation_source()
        .replace(
            "sun_radiance: vec4<f32> }",
            "sun_radiance: vec4<f32>, wind_flags: vec4<f32> }",
        )
        .replace(
            "vec4<f32>(1.0));",
            &format!("vec4<f32>(1.0), vec4<f32>(0.0,0.0,0.3,{flags}.0));"),
        )
        .replace(
            "vec4<f32>(1.0, 1.0, 0.0, 0.0)",
            "vec4<f32>(1.0, 1.0, 0.0, 2.0)",
        );
    format!("{source}\n{}", include_str!("shadow_response.wgsl"))
}
