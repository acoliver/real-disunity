use super::*;

pub(super) fn resolve_capture(
    context: &GpuContext,
    reflection: f32,
    roughness: f32,
    water: bool,
) -> Vec<u8> {
    resolve_capture_settings(context, reflection, roughness, water, settings())
}
pub(super) fn resolve_capture_settings(
    context: &GpuContext,
    reflection: f32,
    roughness: f32,
    water: bool,
    policy: SceneSsr,
) -> Vec<u8> {
    resolve_capture_prepared(context, reflection, roughness, water, policy, false)
}
pub(super) fn resolve_capture_prepared(
    context: &GpuContext,
    reflection: f32,
    roughness: f32,
    water: bool,
    policy: SceneSsr,
    forward_render: bool,
) -> Vec<u8> {
    let size = [3, 3];
    let ssr = resolve_ssr(context, size, policy);
    prepare_forward(context, &ssr);
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
    super::fidelity_tests::prepare_application_ao(context, &ssr, &final_depth);
    let target =
        crate::ssao::passes::texture(context.device(), size, wgpu::TextureFormat::Rgba16Float);
    upload(context, &target, &[0.25, 0.125, 0.0625, 1.0].repeat(9));
    if forward_render {
        super::forward_ao_tests::prepare(context, &ssr, &target);
    }
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
    encoder.finish().submit().unwrap();
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

pub(super) fn resolve_inputs(ssr: &Ssr, final_depth: &wgpu::Texture) -> Vec<wgpu::TextureView> {
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
    inputs.extend(
        ssr.forward_products
            .each_ref()
            .map(|t| t.create_view(&wgpu::TextureViewDescriptor::default())),
    );
    ssr.application_views(&mut inputs);
    inputs
}

fn resolve_ssr(context: &GpuContext, size: [u32; 2], policy: SceneSsr) -> Ssr {
    let mut screen = SceneMetadata::new(context.device(), size, neutral_ao()).unwrap();
    Ssr::new(
        context.device(),
        FrameConfig::new(size, wgpu::TextureFormat::Rgba16Float, 1),
        policy,
        &mut screen,
    )
    .unwrap()
}

#[test]
fn gpu_roughness_fade_has_full_and_zero_endpoints() {
    let context = context();
    let policy = SceneSsr {
        roughness_fade_start_micro: Some(250_000),
        roughness_cutoff_micro: 500_000,
        ..settings()
    };
    let full = resolve_capture_settings(&context, 1.0, 0.25, false, policy);
    let middle = resolve_capture_settings(&context, 1.0, 0.375, false, policy);
    let zero = resolve_capture_settings(&context, 1.0, 0.5, false, policy);
    let red = |bytes: &[u8]| u16::from_le_bytes(bytes[32..34].try_into().unwrap());
    assert_eq!(red(&full), half(0.75));
    assert_eq!(red(&middle), half(0.5));
    assert_eq!(red(&zero), half(0.25));
    assert_eq!(
        resolve_capture(&context, 1.0, 0.5, false),
        resolve_capture_settings(
            &context,
            1.0,
            0.5,
            false,
            SceneSsr {
                roughness_fade_start_micro: Some(450_000),
                ..settings()
            }
        )
    );
}

#[test]
fn gpu_scene_selected_replacement_preserves_direct_and_diffuse_energy() {
    let context = context();
    let policy = SceneSsr {
        composition: worldrunner_render_plan::SceneSsrComposition::ReplaceEnvironment,
        ..settings()
    };
    for (confidence, expected) in [
        (0.0, [0.25, 0.125, 0.0625, 1.0]),
        (0.5, [0.4375, 0.34375, 0.296_875, 1.0]),
        (1.0, [0.625, 0.5625, 0.53125, 1.0]),
    ] {
        let image = resolve_capture_settings(&context, confidence, 0.0, false, policy);
        let expected: Vec<_> = expected
            .into_iter()
            .flat_map(|value| half(value).to_le_bytes())
            .collect();
        assert_eq!(&image[32..40], expected);
    }
}

pub(super) fn prepare_forward(context: &GpuContext, ssr: &Ssr) {
    if ssr.settings.composition != worldrunner_render_plan::SceneSsrComposition::ReplaceEnvironment
        && !ssr.settings.depth_relative_ao
    {
        return;
    }
    assert_eq!(
        ssr.composition
            == worldrunner_render_lighting::reflections::Composition::ReplaceEnvironment,
        ssr.settings.composition
            == worldrunner_render_plan::SceneSsrComposition::ReplaceEnvironment
    );
    let colors = [
        wgpu::Color {
            r: 0.125,
            g: 0.0625,
            b: 0.03125,
            a: 1.0,
        },
        wgpu::Color {
            r: 0.5,
            g: 0.5,
            b: 0.5,
            a: 1.0,
        },
    ];
    let mut encoder = context.submission().begin(context.device()).unwrap();
    for (texture, color) in ssr.forward_products.iter().zip(colors) {
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(color),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
    }
    encoder.finish().submit().unwrap();
}
