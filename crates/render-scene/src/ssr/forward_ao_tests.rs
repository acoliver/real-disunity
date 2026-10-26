use super::ao_accuracy_tests::{assert_resolve, half_value, storage_error};
use super::*;

fn source(output: &str) -> String {
    // Fixed cube/LUT samples isolate publication from texture filtering. All
    // forward BRDF, scene occlusion, exposure and metadata math is production.
    let forward = include_str!("forward.wgsl")
        .replace("textureSampleLevel(reflection_brdf, reflection_sampler, vec2<f32>(cosine, roughness), 0.0).rg", "vec2<f32>(1.0, 0.0)")
        .replace("textureSampleLevel(reflection_environment, reflection_sampler, reflect(-view, normal), roughness * f32(textureNumLevels(reflection_environment) - 1u)).rgb", "vec3<f32>(1.0, 0.5, 0.25)");
    let shader = format!(
        r"
        {}
        struct Frame {{ sun_direction: vec4<f32>, camera_position: vec4<f32>, sun_irradiance: vec4<f32>, grading: vec4<f32> }}
        const frame = Frame(vec4<f32>(0.0), vec4<f32>(0.0), vec4<f32>(0.0), vec4<f32>(2.0));
        fn scene_ray_origin(world: vec3<f32>) -> vec3<f32> {{ return vec3<f32>(0.0); }}
        fn scene_environment_factor(f0: vec3<f32>, c: vec2<f32>, v: f32, ao: f32, r: f32) -> vec3<f32> {{
            return rd_split_sum(f0, c) * rd_specular_occlusion(v, ao, r);
        }}
        {forward}
        @fragment fn fs_screen() -> @location(0) vec4<f32> {{
            let environment = forward_reflection(vec3<f32>(0.0, 0.0, -1.0), vec3<f32>(0.0, 0.0, 1.0), vec3<f32>(0.5), 0.5, 0.5, 1.0);
            let direct = vec3<f32>(0.0625, 0.03125, 0.015625);
            let diffuse = vec3<f32>(0.0625, 0.03125, 0.015625);
            let result = reflection_output(vec4<f32>((direct + diffuse + environment) * frame.grading.x, 1.0));
            return result.{output};
        }}",
        worldrunner_render_lighting::WGSL
    );
    forward::application_metadata(shader, true)
}

fn draw(context: &GpuContext, target: &wgpu::Texture, output: &str) {
    let pipeline = gpu::pipeline(context.device(), &source(output), 1, false);
    let mut encoder = context.submission().begin(context.device()).unwrap();
    worldrunner_render_post::fullscreen_pass(
        &mut encoder,
        None,
        wgpu::RenderPassColorAttachment {
            view: &target.create_view(&wgpu::TextureViewDescriptor::default()),
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        },
        &pipeline,
        &[],
    );
    encoder.finish().submit().unwrap();
}

pub(super) fn prepare(context: &GpuContext, ssr: &Ssr, target: &wgpu::Texture) {
    draw(context, target, "color");
    draw(context, &ssr.forward_products[0], "environment");
    draw(context, &ssr.forward_products[1], "factor");
}

#[test]
fn gpu_forward_then_resolve_replaces_actual_lobe_and_preserves_direct_diffuse() {
    let context = context();
    let policy = SceneSsr {
        depth_relative_ao: true,
        roughness_fade_start_micro: Some(550_000),
        composition: worldrunner_render_plan::SceneSsrComposition::ReplaceEnvironment,
        ..settings()
    };
    let staging =
        crate::ssao::passes::texture(context.device(), [3, 3], wgpu::TextureFormat::Rgba16Float);
    draw(&context, &staging, "environment");
    let environment = read(&context, &staging);
    let baseline = numerical::resolve_capture_prepared(&context, 0.0, 0.5, false, policy, true);
    for (channel, direct_diffuse) in [0.25, 0.125, 0.0625].into_iter().enumerate() {
        let offset = 32 + channel * 2;
        let scene = half_value(&baseline[offset..]);
        let lobe = half_value(&environment[offset..]);
        // Feed GPU forward intermediates to the f64 resolve oracle, avoiding
        // assumptions about forward pow accuracy. Independent scene-lobe
        // identity still checks publication, including exposure and old AO.
        assert!(
            (scene - lobe - direct_diffuse).abs() <= storage_error(scene) + storage_error(lobe)
        );
        assert!(lobe > 0.1, "fixture must exercise a nonzero old lobe");
        for confidence in [0.0, 0.5, 1.0] {
            let image =
                numerical::resolve_capture_prepared(&context, confidence, 0.5, false, policy, true);
            assert_resolve(
                half_value(&image[offset..]),
                scene,
                lobe,
                f64::from(confidence),
            );
            if confidence == 0.0 {
                assert_eq!(image, baseline);
            }
        }
    }
}
