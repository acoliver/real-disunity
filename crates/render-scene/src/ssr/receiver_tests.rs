use super::*;

fn gather_ssr(context: &GpuContext) -> Ssr {
    use worldrunner_render_plan::{SceneSsaoFilter, SceneSsaoKernel, SceneSsaoReceiver};
    let mut screen = SceneMetadata::new(
        context.device(),
        [3, 3],
        SceneSsao {
            receiver: SceneSsaoReceiver::DepthAwareGather,
            kernel: SceneSsaoKernel::Scalable {
                turns_micro: 7_000_000,
                power_micro: 1_000_000,
            },
            filter: SceneSsaoFilter::ScalableBilateral {
                radius: 4,
                step: 2,
                sigma_scale_micro: 1_500_000,
                sharpness_micro: 7_000_000,
            },
            sample_count: 10,
            ..neutral_ao()
        },
    )
    .unwrap();
    Ssr::new(
        context.device(),
        FrameConfig::new([3, 3], wgpu::TextureFormat::Rgba16Float, 1),
        SceneSsr {
            depth_relative_ao: true,
            roughness_fade_start_micro: Some(550_000),
            ..settings()
        },
        &mut screen,
    )
    .unwrap()
}

#[test]
fn gpu_scalable_gather_application_ao_uses_receiver_and_consumed_metre_depths() {
    let context = context();
    let ssr = gather_ssr(&context);
    assert_eq!(ssr.application_ao.format(), wgpu::TextureFormat::Rg32Float);
    let values = [0.125_f32, 2.0, 0.25, 2.0, 0.75, 4.0, 0.875, 4.0];
    context.queue().write_texture(
        ssr.application_ao.as_image_copy(),
        &values
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect::<Vec<_>>(),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(16),
            rows_per_image: Some(2),
        },
        ssr.application_ao.size(),
    );
    super::numerical::prepare_forward(&context, &ssr);
    upload(&context, &ssr.material, &[0.5; 36]);
    let mut depths = [3.0; 9];
    depths[0] = 20.0;
    upload(&context, &ssr.hierarchy[0], &depths);
    for texture in &ssr.reflections {
        upload(
            &context,
            texture,
            &vec![1.0; usize::try_from(texture.width() * texture.height() * 4).unwrap()],
        );
    }
    let target =
        crate::ssao::passes::texture(context.device(), [3, 3], wgpu::TextureFormat::Rgba16Float);
    upload(&context, &target, &[0.0; 36]);
    let color = AttachmentView::new(ResourceRef::new("scene color"), &target).unwrap();
    let mut feature = ReflectionFeature {
        water: None,
        ssr: &ssr,
        frame: frame(),
        color,
        groups: Vec::new(),
    };
    let mut encoder = context.submission().begin(context.device()).unwrap();
    let mut prepare = PrepareContext::new(context.device(), &mut encoder).unwrap();
    feature.commit(&mut prepare);
    prepare.finish();
    encoder.finish().submit().unwrap();
    let inputs = super::numerical::resolve_inputs(&ssr, &ssr.hierarchy[0]);
    let group = gpu::group(context.device(), &ssr.resolve, Some(&ssr.uniform), &inputs);
    super::application_policy_tests::draw_policy(&context, &ssr.resolve, &group, &target);
    let bytes = read(&context, &target);
    // Receiver depth 3 gives equal weights for consumed depths 2 and 4.
    let ao = 0.505_f64;
    let visibility = (1.0 + ao).powf(2.0_f64.powi(-9)) - 1.0 + ao;
    let expected = 0.5 * visibility;
    let actual = super::ao_accuracy_tests::half_value(&bytes[32..34]);
    assert!(
        (actual - expected).abs() <= super::ao_accuracy_tests::storage_error(expected) + 2e-6,
        "SSR application {actual} != {expected}"
    );
}
