use super::*;

fn production_depths(context: &GpuContext) -> Vec<Vec<f64>> {
    let settings = SceneSsr {
        traversal: SceneSsrTraversal::Grid,
        grid_level_count: Some(6),
        ..settings()
    };
    let hierarchy = final_depth::FinalDepth::new(context.device(), [32, 32], 1, &settings);
    let input =
        crate::ssao::passes::texture(context.device(), [32, 32], wgpu::TextureFormat::R32Float);
    let mut depths = vec![9000.0; 1024];
    depths[16 * 32 + 15] = 5.0;
    upload(context, &input, &depths);
    let mut encoder = context.submission().begin(context.device()).unwrap();
    hierarchy.encode_for_test(context.device(), &mut encoder, &input);
    encoder.finish().submit().unwrap();
    hierarchy
        .textures()
        .iter()
        .map(|texture| {
            read(context, texture)
                .chunks_exact(4)
                .map(|bytes| f64::from(f32::from_le_bytes(bytes.try_into().unwrap())))
                .collect()
        })
        .collect()
}

#[test]
fn gpu_grid_preserves_thin_foreground_contacts_against_level_zero() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let depths = production_depths(&context);
    for (a, b) in [
        ([2.25, 16.25], [29.25, 16.25]),
        ([29.25, 16.25], [2.25, 16.25]),
        ([15.25, 2.25], [15.25, 29.25]),
        ([15.25, 29.25], [15.25, 2.25]),
    ] {
        for base in [0, 1] {
            let divisor = f64::from(1u32 << base);
            let ray = Ray {
                a: a.map(|value| value / divisor),
                b: b.map(|value| value / divisor),
                weighted: [5.125; 2],
                reciprocal: [1.0; 2],
                skip: 0.0,
                budget: 256,
                behind: 0,
                base,
                side: 32,
                levels: 6 - base,
            };
            let levels = &depths[usize::try_from(base).unwrap()..];
            let mut reference = capture_grid(
                &context,
                Ray {
                    a,
                    b,
                    base: 0,
                    levels: 1,
                    ..ray
                },
                &depths[..1],
            );
            reference[0] /= divisor;
            reference[1] /= divisor;
            assert_eq!(reference[5].to_bits(), 1.0f64.to_bits(), "{ray:?}");
            assert_eq!(reference[2].to_bits(), 5.0f64.to_bits());
            let actual = if base == 0 {
                capture_grid(&context, ray, levels)
            } else {
                child_tests::child_capture(&context, ray, &depths)
            };
            assert_eq!(actual[5].to_bits(), 1.0f64.to_bits(), "skipped: {ray:?}");
            for channel in [0, 1, 2, 6, 7] {
                assert!((actual[channel] - reference[channel]).abs() < 0.025);
            }
            if base == 0 {
                assert_eq!(&actual[6..8], &[15.0, 16.0]);
            }
        }
    }
}

fn ownership_capture(context: &GpuContext, policy: SceneSsr, shader: &str) -> Vec<u8> {
    let size = [65, 65];
    let mut screen = SceneMetadata::new(context.device(), size, neutral_ao()).unwrap();
    let mut ssr = Ssr::new(
        context.device(),
        FrameConfig::new(size, wgpu::TextureFormat::Rgba16Float, 1),
        policy,
        &mut screen,
    )
    .unwrap();
    ssr.trace = gpu::pipeline(context.device(), shader, 1, false);
    let (mut depths, normals, materials, colors) =
        mirror_data(&frame(), 0.0, glam::Vec2::new(0.65, 0.35), false);
    depths[0] = 9000.0;
    assert_eq!(depths[22 * 65 + 42].to_bits(), 5.0f32.to_bits());
    upload(context, &ssr.normals, &normals);
    upload(context, &ssr.material, &materials);
    upload_hierarchy(context, &ssr, size, depths);
    let opaque =
        crate::ssao::passes::texture(context.device(), size, wgpu::TextureFormat::Rgba16Float);
    upload(context, &opaque, &colors);
    draw_trace(context, &ssr, &frame(), &opaque)
}

#[test]
fn gpu_grid_nearest_contact_cannot_borrow_a_background_owner() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let policy = SceneSsr {
        traversal: SceneSsrTraversal::Grid,
        grid_domain: worldrunner_render_plan::SceneSsrGridDomain::ReflectionOutput,
        resolution_scale_micro: 500_000,
        ..settings()
    };
    let source = grid::trace_source(1, false, &policy, [65, 65]);
    for (pixel, accepted) in [("0, 0", false), ("42, 22", true)] {
        // Control the selected child and its normal independently of traversal.
        // The foreground contact must not acquire a different child's depth.
        let shader = source
            .replace(
                "let hit_pixel = vec2<i32>(full_pixel);",
                &format!("let hit_pixel = vec2<i32>({pixel});"),
            )
            .replace(
                "let hit_normal = textureLoad(normals, hit_pixel, 0);",
                "let hit_normal = vec4<f32>(-direction, 1.0);",
            );
        let bytes = ownership_capture(&context, policy, &shader);
        assert_eq!(
            bytes.chunks_exact(8).any(|pixel| pixel[6..8] != [0, 0]),
            accepted,
            "mapped child {pixel}"
        );
        let substituted = shader.replace(
            "intersection.depth != textureLoad(grid_depth_4, hit_pixel, 0).r || !matches_receiver_depth(intersection.depth, metadata_depth",
            "!matches_receiver_depth(textureLoad(grid_depth_4, hit_pixel, 0).r, metadata_depth",
        );
        assert_ne!(substituted, shader);
        let borrowed = ownership_capture(&context, policy, &substituted);
        assert!(borrowed.chunks_exact(8).any(|pixel| pixel[6..8] != [0, 0]));
    }
}
