use super::*;

fn reduce_levels(context: &GpuContext, encoder: &mut wgpu::CommandEncoder, ssr: &Ssr) {
    for pair in ssr.reflections.windows(2) {
        let group = gpu::group(
            context.device(),
            &ssr.reduce,
            None,
            &[pair[0].create_view(&wgpu::TextureViewDescriptor::default())],
        );
        draw(context, encoder, &pair[1], &ssr.reduce, &group);
    }
}

fn targets(context: &GpuContext, ssr: &Ssr, samples: u32) -> (wgpu::Texture, wgpu::Texture) {
    let size = [ssr.material.width(), ssr.material.height()];
    let output =
        crate::ssao::passes::texture(context.device(), size, wgpu::TextureFormat::Rgba16Float);
    let target = context.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("regression resolve"),
        size: wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: samples,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    (output, target)
}

fn edge_weight(pixel: u32, base: u32, level: u32) -> f32 {
    let footprint = gpu::scalar(1 << level);
    let last = gpu::scalar(base.div_ceil(1 << level) - 1);
    let position = (gpu::scalar(pixel) + 0.5) / 65.0 * gpu::scalar(base);
    let lower = (position / footprint - 0.5).floor().clamp(0.0, last);
    let upper = (lower + 1.0).min(last);
    let centre =
        |index: f32| ((index + 0.5) * footprint).min((index * footprint + gpu::scalar(base)) * 0.5);
    let weight =
        ((position - centre(lower)) / (centre(upper) - centre(lower)).max(0.5)).clamp(0.0, 1.0);
    let a = f32::from(u8::from(lower >= last));
    let b = f32::from(u8::from(upper >= last));
    a * (1.0 - weight) + b * weight
}

#[test]
fn gpu_first_crossing_rejects_uncovered_and_back_facing_targets() {
    let context = context();
    let target = glam::Vec2::new(0.65, 0.35);
    let pixel = (45 * 65 + 32) * 8;
    for normal in [[0.0; 4], [0.0, 0.0, -1.0, 1.0]] {
        let result = trace_fixture_normal(&context, 0.0, target, false, normal);
        assert_eq!(&result[pixel..pixel + 8], &[0; 8]);
    }
    let covered = trace_fixture(&context, 0.0, target, false);
    assert!(u16::from_le_bytes(covered[pixel..pixel + 2].try_into().unwrap()) > 0);
}

pub(super) fn resolve(
    context: &GpuContext,
    ssr: &Ssr,
    opaque: &wgpu::Texture,
    final_depth: &wgpu::Texture,
    reduce: bool,
    samples: u32,
) -> Vec<u8> {
    let (output, target) = targets(context, ssr, samples);
    let mut encoder = context.submission().begin(context.device()).unwrap();
    let mut feature = ReflectionFeature {
        water: None,
        ssr,
        frame: frame(),
        color: AttachmentView::new(ResourceRef::new("scene color"), &target).unwrap(),
        groups: Vec::new(),
    };
    let mut prepare = PrepareContext::new(context.device(), &mut encoder).unwrap();
    feature.commit(&mut prepare);
    prepare.finish();
    if reduce {
        reduce_levels(context, &mut encoder, ssr);
    }
    let mut inputs = vec![
        ssr.material
            .create_view(&wgpu::TextureViewDescriptor::default()),
        ssr.normals
            .create_view(&wgpu::TextureViewDescriptor::default()),
        opaque.create_view(&wgpu::TextureViewDescriptor::default()),
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
    let group = gpu::group(context.device(), &ssr.resolve, Some(&ssr.uniform), &inputs);
    let output_view = output.create_view(&wgpu::TextureViewDescriptor::default());
    let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());
    worldrunner_render_post::fullscreen_pass(
        &mut encoder,
        Some("SSR sample resolve"),
        wgpu::RenderPassColorAttachment {
            view: if samples == 1 {
                &output_view
            } else {
                &target_view
            },
            depth_slice: None,
            resolve_target: (samples != 1).then_some(&output_view),
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        },
        &ssr.resolve,
        &[&group],
    );
    encoder.finish().submit().unwrap();
    read(context, &output)
}

fn draw(
    _context: &GpuContext,
    encoder: &mut wgpu::CommandEncoder,
    target: &wgpu::Texture,
    pipeline: &wgpu::RenderPipeline,
    group: &wgpu::BindGroup,
) {
    worldrunner_render_post::fullscreen_pass(
        encoder,
        Some("SSR regression"),
        wgpu::RenderPassColorAttachment {
            view: &target.create_view(&wgpu::TextureViewDescriptor::default()),
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        },
        pipeline,
        &[group],
    );
}

fn red(bytes: &[u8], width: usize, x: usize, y: usize) -> f32 {
    let offset = (y * width + x) * 8;
    let value = u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap());
    if value == 0 {
        return 0.0;
    }
    let exponent = i32::from((value >> 10) & 31) - 15;
    (1.0 + f32::from(value & 1023) / 1024.0) * 2.0f32.powi(exponent)
}

#[test]
fn gpu_odd_extent_filtered_bins_preserve_interior_and_partial_edges() {
    let context = context();
    let size = [65, 65];
    for scale in [1_000_000, 500_000] {
        let mut screen = SceneMetadata::new(context.device(), size, neutral_ao()).unwrap();
        let ssr = Ssr::new(
            context.device(),
            FrameConfig::new(size, wgpu::TextureFormat::Rgba16Float, 1),
            SceneSsr {
                resolution_scale_micro: scale,
                ..settings()
            },
            &mut screen,
        )
        .unwrap();
        upload(
            &context,
            &ssr.normals,
            &[0.0, 0.0, 1.0, 1.0].repeat(65 * 65),
        );
        upload(
            &context,
            &ssr.material,
            &[1.0, 1.0, 1.0, 0.5625].repeat(65 * 65),
        );
        let base = &ssr.reflections[0];
        let mut values = Vec::new();
        for y in 0..base.height() {
            for x in 0..base.width() {
                let edge = x == base.width() - 1 || y == base.height() - 1;
                values.extend([if edge { 1.0 } else { 0.0 }, 0.0, 0.0, 1.0]);
            }
        }
        upload(&context, base, &values);
        let depth =
            crate::ssao::passes::texture(context.device(), size, wgpu::TextureFormat::R32Float);
        upload(&context, &depth, &vec![3.0; 65 * 65]);
        let bytes = resolve(&context, &ssr, &depth, &depth, true, 1);
        for (x, y) in [(48, 32), (32, 48), (64, 32), (32, 64), (64, 64), (63, 32)] {
            let field = |level| {
                let a = edge_weight(x, base.width(), level);
                let b = edge_weight(y, base.height(), level);
                a + b - a * b
            };
            let expected = (field(4) * 0.3125 + field(5) * 0.6875) * 0.15625;
            let actual = red(
                &bytes,
                65,
                usize::try_from(x).unwrap(),
                usize::try_from(y).unwrap(),
            );
            assert!(
                (actual - expected).abs() < 0.0003,
                "partial-bin reference at ({x},{y}), scale {scale}: {actual} != {expected}"
            );
        }
        assert!(red(&bytes, 65, 63, 32) < red(&bytes, 65, 64, 32));
    }
}

#[test]
fn gpu_roughness_contribution_fades_before_cutoff() {
    let context = context();
    let full = numerical::resolve_capture(&context, 1.0, 0.0, false);
    let fading = numerical::resolve_capture(&context, 1.0, 0.55, false);
    let cutoff = numerical::resolve_capture(&context, 1.0, 0.6, false);
    assert!(red(&full, 3, 1, 1) > red(&fading, 3, 1, 1));
    assert!(red(&fading, 3, 1, 1) > red(&cutoff, 3, 1, 1));
    assert_eq!(red(&cutoff, 3, 1, 1).to_bits(), 0.25f32.to_bits());
}
