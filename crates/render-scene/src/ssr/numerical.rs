use super::*;

pub(super) fn resolve_capture(
    context: &GpuContext,
    reflection: f32,
    roughness: f32,
    water: bool,
) -> Vec<u8> {
    let size = [3, 3];
    let ssr = resolve_ssr(context, size);
    upload(context, &ssr.normals, &[0.0, 0.0, 1.0, 1.0].repeat(9));
    upload(
        context,
        &ssr.material,
        &[0.5, 0.5, 0.5, roughness].repeat(9),
    );
    upload(context, &ssr.hierarchy[0], &[3.0; 9]);
    for texture in &ssr.reflections {
        upload(
            context,
            texture,
            &[reflection, reflection, reflection, reflection]
                .repeat(usize::try_from(texture.width() * texture.height()).unwrap()),
        );
    }
    let final_depth =
        crate::ssao::passes::texture(context.device(), size, wgpu::TextureFormat::R32Float);
    upload(context, &final_depth, &[if water { 2.0 } else { 3.0 }; 9]);
    let target =
        crate::ssao::passes::texture(context.device(), size, wgpu::TextureFormat::Rgba16Float);
    upload(context, &target, &[0.25, 0.125, 0.0625, 1.0].repeat(9));
    let color = AttachmentView::new(ResourceRef::new("scene color"), &target).unwrap();
    let mut feature = ReflectionFeature {
        ssr: &ssr,
        frame: frame(),
        color,
        groups: Vec::new(),
    };
    let mut encoder = context
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    let mut prepare = PrepareContext::new(context.device(), &mut encoder);
    feature.commit(&mut prepare);
    prepare.finish();
    let inputs = resolve_inputs(&ssr, &final_depth);
    let group = gpu::group(context.device(), &ssr.resolve, Some(&ssr.uniform), &inputs);
    worldrunner_render_post::fullscreen_pass(
        &mut encoder,
        Some("specular resolve reference"),
        wgpu::RenderPassColorAttachment {
            view: &target.create_view(&wgpu::TextureViewDescriptor::default()),
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Load,
                store: wgpu::StoreOp::Store,
            },
        },
        &ssr.resolve,
        &[&group],
    );
    context.queue().submit([encoder.finish()]);
    read(context, &target)
}
#[test]
fn gpu_resolve_adds_specular_once_and_misses_roughness_and_water_preserve_existing_radiance() {
    let context = context();
    let baseline: Vec<_> = [0.25, 0.125, 0.0625, 1.0]
        .repeat(9)
        .into_iter()
        .flat_map(|value| half(value).to_le_bytes())
        .collect();
    assert_eq!(resolve_capture(&context, 0.0, 0.0, false), baseline);
    assert_eq!(resolve_capture(&context, 1.0, 0.75, false), baseline);
    assert_eq!(resolve_capture(&context, 1.0, 0.0, true), baseline);
    let reflected = resolve_capture(&context, 1.0, 0.0, false);
    let centre = &reflected[4 * 8..5 * 8];
    let expected: Vec<_> = [0.75, 0.625, 0.5625, 1.0]
        .into_iter()
        .flat_map(|value| half(value).to_le_bytes())
        .collect();
    assert_eq!(
        centre, expected,
        "existing diffuse/direct radiance plus one specular contribution"
    );
}

fn resolve_inputs(ssr: &Ssr, final_depth: &wgpu::Texture) -> Vec<wgpu::TextureView> {
    let mut inputs = vec![
        ssr.material
            .create_view(&wgpu::TextureViewDescriptor::default()),
        ssr.normals
            .create_view(&wgpu::TextureViewDescriptor::default()),
        ssr.hierarchy[0].create_view(&wgpu::TextureViewDescriptor::default()),
        final_depth.create_view(&wgpu::TextureViewDescriptor::default()),
    ];
    inputs.extend(
        ssr.reflections
            .iter()
            .map(|texture| texture.create_view(&wgpu::TextureViewDescriptor::default())),
    );
    inputs
}

fn resolve_ssr(context: &GpuContext, size: [u32; 2]) -> Ssr {
    let mut screen = Ssao::new(context.device(), size, neutral_ao()).unwrap();
    Ssr::new(
        context.device(),
        FrameConfig::new(size, wgpu::TextureFormat::Rgba16Float, 1),
        settings(),
        &mut screen,
    )
    .unwrap()
}
