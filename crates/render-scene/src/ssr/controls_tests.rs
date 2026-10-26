use super::*;

fn capture(context: &GpuContext, policy: SceneSsr, scale: f32, occluder: bool) -> Vec<u8> {
    capture_with_shader(context, policy, scale, occluder, None)
}
pub(super) fn capture_with_shader(
    context: &GpuContext,
    policy: SceneSsr,
    scale: f32,
    occluder: bool,
    shader: Option<&str>,
) -> Vec<u8> {
    let size = [65, 65];
    let mut screen = SceneMetadata::new(context.device(), size, neutral_ao()).unwrap();
    let mut ssr = Ssr::new(
        context.device(),
        FrameConfig::new(size, wgpu::TextureFormat::Rgba16Float, 1),
        policy,
        &mut screen,
    )
    .unwrap();
    if let Some(shader) = shader {
        ssr.trace = gpu::pipeline(context.device(), shader, 1, false);
    }
    let frame = frame();
    let (mut depths, normals, materials, colors) =
        mirror_data(&frame, 0.0, glam::Vec2::new(0.65, 0.35), occluder);
    for depth in &mut depths {
        *depth *= scale;
    }
    upload(context, &ssr.normals, &normals);
    upload(context, &ssr.material, &materials);
    upload_hierarchy(context, &ssr, size, depths);
    let opaque =
        crate::ssao::passes::texture(context.device(), size, wgpu::TextureFormat::Rgba16Float);
    upload(context, &opaque, &colors);
    draw_trace(context, &ssr, &frame, &opaque)
}
pub(super) fn center_red(bytes: &[u8]) -> u16 {
    let offset = (45 * 65 + 32) * 8;
    u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap())
}
#[test]
fn gpu_distance_origin_skip_and_pass_behind_control_traversal() {
    let context = context();
    assert!(center_red(&capture(&context, settings(), 1.0, false)) > 0);
    assert_eq!(
        center_red(&capture(
            &context,
            SceneSsr {
                max_distance_millimeters: 500,
                ..settings()
            },
            1.0,
            false
        )),
        0
    );
    assert_eq!(
        center_red(&capture(
            &context,
            SceneSsr {
                origin_skip_pixels: 4096,
                ..settings()
            },
            1.0,
            false
        )),
        0
    );
    assert_eq!(center_red(&capture(&context, settings(), 1.0, true)), 0);
    assert!(
        center_red(&capture(
            &context,
            SceneSsr {
                pass_behind_steps: 256,
                ..settings()
            },
            1.0,
            true
        )) > 0
    );
}
#[test]
fn gpu_luminance_ceiling_reduces_reflected_radiance() {
    let context = context();
    let unrestricted = capture(&context, settings(), 1.0, false);
    let limited = capture(
        &context,
        SceneSsr {
            luminance_limit_milli: Some(100),
            ..settings()
        },
        1.0,
        false,
    );
    assert!(center_red(&limited) > 0);
    assert!(center_red(&limited) < center_red(&unrestricted));
    let offset = (45 * 65 + 32) * 8 + 6;
    assert_eq!(
        &limited[offset..offset + 2],
        &unrestricted[offset..offset + 2]
    );
}
#[test]
fn gpu_luminance_ceiling_preserves_trace_confidence_at_all_brightnesses() {
    let context = context();
    for radiance in [0.0, 0.000_030_517_578_125, 0.25, 2.0] {
        let shader = gpu::trace_source(1, false).replace(
            "var hit = textureLoad(opaque, hit_pixel, 0).rgb;",
            &format!(
                "var hit = textureLoad(opaque, hit_pixel, 0).rgb * 0.0 + vec3<f32>({radiance});"
            ),
        );
        let unrestricted = capture_with_shader(&context, settings(), 1.0, false, Some(&shader));
        let limited = capture_with_shader(
            &context,
            SceneSsr {
                luminance_limit_milli: Some(500),
                ..settings()
            },
            1.0,
            false,
            Some(&shader),
        );
        let offset = (45 * 65 + 32) * 8 + 6;
        assert_ne!(&unrestricted[offset..offset + 2], &[0, 0]);
        assert_eq!(
            &limited[offset..offset + 2],
            &unrestricted[offset..offset + 2]
        );
    }
}
#[test]
fn gpu_luminance_ceiling_matches_reference_floor_and_preserves_confidence() {
    let context = context();
    let shader = format!(
        "{}\n@fragment fn fs_screen(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {{
        let index = u32(p.x);
        let radiance = array<f32, 4>(0.0, 0.000030517578125, 0.25, 2.0)[index];
        let confidence = array<f32, 4>(0.125, 0.25, 0.5, 0.75)[index];
        let bounded = bounded_radiance(vec3<f32>(radiance), 0.5);
        // Scale the dark case before half-float storage so attenuation stays measurable.
        let readback_scale = select(1.0, 16384.0, index == 1u);
        return vec4<f32>(bounded * readback_scale, confidence);
    }}",
        include_str!("controls.wgsl")
    );
    let pipeline = gpu::pipeline(context.device(), &shader, 1, false);
    let target =
        crate::ssao::passes::texture(context.device(), [4, 1], wgpu::TextureFormat::Rgba16Float);
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
    let expected: Vec<_> = [
        [0.0, 0.0, 0.0, 0.125],
        [0.152_587_89, 0.152_587_89, 0.152_587_89, 0.25],
        [0.25, 0.25, 0.25, 0.5],
        [0.5, 0.5, 0.5, 0.75],
    ]
    .into_iter()
    .flatten()
    .flat_map(|value| half(value).to_le_bytes())
    .collect();
    assert_eq!(read(&context, &target), expected);
}
#[test]
fn gpu_depth_scaled_thickness_matches_linear_depth_at_two_depths() {
    let context = context();
    let shader = format!(
        "{}\n@fragment fn fs_screen(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {{
        let depth = select(8.0, 32.0, p.x > 1.0);
        return vec4<f32>(reflection_thickness(0.25, 0.03125, depth), 0.0, 0.0, 1.0);
    }}",
        include_str!("controls.wgsl")
    );
    let pipeline = gpu::pipeline(context.device(), &shader, 1, false);
    let target =
        crate::ssao::passes::texture(context.device(), [2, 1], wgpu::TextureFormat::Rgba16Float);
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
    let expected: Vec<_> = [0.5, 0.0, 0.0, 1.0, 1.25, 0.0, 0.0, 1.0]
        .into_iter()
        .flat_map(|value| half(value).to_le_bytes())
        .collect();
    assert_eq!(read(&context, &target), expected);
}
#[test]
fn gpu_default_trace_bytes_match_fixed_traversal() {
    let context = context();
    let fixed = gpu::trace_source(1, false)
        .replace(
            "reflection_pass_count(roughness, camera.traversal.w, camera.behind)",
            "0u",
        )
        .replace("camera.traversal.x", "100.0")
        .replace("camera.traversal.z", "2.0")
        .replace("camera.traversal.w", "0.0")
        .replace(
            "reflection_thickness(camera.settings.y, camera.traversal.y, surface_depth)",
            "camera.settings.y",
        )
        .replace(
            "min(difference, next_difference) > thickness",
            "difference > thickness",
        );
    for scale in [1.0, 10.0] {
        for occluder in [false, true] {
            assert_eq!(
                capture(&context, settings(), scale, occluder),
                capture_with_shader(&context, settings(), scale, occluder, Some(&fixed))
            );
        }
    }
}
