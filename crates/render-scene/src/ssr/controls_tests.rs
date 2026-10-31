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
    capture_material(context, policy, scale, occluder, shader, 0.0)
}
pub(super) fn capture_material(
    context: &GpuContext,
    policy: SceneSsr,
    scale: f32,
    occluder: bool,
    shader: Option<&str>,
    roughness: f32,
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
        mirror_data(&frame, roughness, glam::Vec2::new(0.65, 0.35), occluder);
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
/// Absolute f64 model of the centre pixel of the mirror fixture: the reflected ray meets the red
/// disc at uv (0.65, 0.35) from origin uv (0.5, 0.7), both far from the edge fade (edge distance
/// 0.35 and 0.3 against 0.08), within the step budget (the hit comes well before 204 of 256 steps)
/// and within half the thickness of the surface, so every fade is 1 and the pixel is
/// `(hit * 1, 1)` = red 1 with confidence 1. With an occluder in front the ray is rejected
/// and the pixel is transparent black. The values are at most 1, so the arithmetic range is 1.
pub(super) fn assert_centre_matches_model(bytes: &[u8], hit: bool, context: &str) {
    let offset = (45 * 65 + 32) * 8;
    let want = if hit { [1.0, 0.0, 0.0, 1.0] } else { [0.0; 4] };
    crate::tolerance_tests::assert_half_pixel(&bytes[offset..offset + 8], want, context);
}
/// Two traces of the same hit decisions: every half word agrees within twice the store bound
/// (range 1), except at words where a hit decision may flip when the compared code is not
/// compiled identically; those are bounded to 0.5% of the words.
pub(super) fn assert_traces_agree(actual: &[u8], expected: &[u8], context: &str) {
    assert_eq!(actual.len(), expected.len(), "{context}: lengths");
    let word = |bytes: &[u8], i: usize| {
        f64::from(::half::f16::from_le_bytes([bytes[2 * i], bytes[2 * i + 1]]).to_f32())
    };
    let words = actual.len() / 2;
    let differing = (0..words)
        .filter(|&i| {
            let (a, e) = (word(actual, i), word(expected, i));
            let bound = 2.0 * worldrunner_render_gpu::tolerance_test_support::half_bound(e, 1.0);
            (a - e).abs() > bound
        })
        .count();
    assert!(
        differing * 200 <= words,
        "{context}: {differing} of {words} words differ by more than the store bound"
    );
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
    // `base + depth * slope` is one multiply and one add, 0.25 + 8 * 0.03125 and 0.25 + 32 * 0.03125,
    // each exact in f32; the bound is still the shared half store bound with range equal to the
    // result, because contraction and the half conversion are implementation-defined.
    let bytes = read(&context, &target);
    for (index, want) in [0.5, 0.0, 0.0, 1.0, 1.25, 0.0, 0.0, 1.0]
        .into_iter()
        .enumerate()
    {
        crate::tolerance_tests::assert_half_value(
            f64::from(
                ::half::f16::from_le_bytes([bytes[2 * index], bytes[2 * index + 1]]).to_f32(),
            ),
            want,
            want,
            &format!("thickness word {index}"),
        );
    }
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
            let current = capture(&context, settings(), scale, occluder);
            let retired = capture_with_shader(&context, settings(), scale, occluder, Some(&fixed));
            let case = format!("scale {scale} occluder {occluder}");
            assert_traces_agree(&current, &retired, &case);
            // Both traces share `trace.wgsl`, so agreement cannot see an error in it; the
            // absolute model of the centre pixel can.
            assert_centre_matches_model(&current, !occluder, &case);
            assert_centre_matches_model(&retired, !occluder, &case);
        }
    }
}
