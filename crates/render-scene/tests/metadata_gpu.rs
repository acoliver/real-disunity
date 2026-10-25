use worldrunner_render_scene::{
    device::GpuContext,
    executor::{FrameConfig, Readback, ResourceRef},
    frame::metadata::{FrameProduct, MetadataMeaning, MetadataSampling},
    wgpu,
};

fn texture(context: &GpuContext, samples: u32) -> wgpu::Texture {
    context.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("synthetic frame metadata"),
        size: wgpu::Extent3d {
            width: 3,
            height: 5,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: samples,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | if samples == 1 {
                wgpu::TextureUsages::COPY_SRC
            } else {
                wgpu::TextureUsages::empty()
            },
        view_formats: &[],
    })
}

fn encode_clear(
    context: &GpuContext,
    source: &wgpu::Texture,
    resolve: Option<&wgpu::Texture>,
    red: f64,
) -> worldrunner_render_gpu::submission::PreparedFrame {
    let mut encoder = context.submission().begin(context.device()).unwrap();
    let view = source.create_view(&wgpu::TextureViewDescriptor::default());
    let resolve_view =
        resolve.map(|texture| texture.create_view(&wgpu::TextureViewDescriptor::default()));
    {
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("metadata coverage clear"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: resolve_view.as_ref(),
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: red,
                        g: 0.5,
                        b: 1.0,
                        a: 0.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..wgpu::RenderPassDescriptor::default()
        });
    }
    encoder.finish()
}

#[test]
fn gpu_metadata_coverage_clear_resolve_and_cancellation_at_1x_and_4x() {
    let context =
        pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty())).unwrap();
    for samples in [1, 4] {
        let source = texture(&context, samples);
        let output = texture(&context, 1);
        let product = FrameProduct::new(
            ResourceRef::new("synthetic normals"),
            MetadataMeaning::ViewNormalCoverage,
            wgpu::TextureFormat::Rgba16Float,
            MetadataSampling::PerSample(samples),
        )
        .unwrap();
        assert!(product.attachment(&source).is_ok());
        assert_eq!(
            FrameProduct::normals().attachment(&source).is_ok(),
            samples == 1
        );
        let (target, resolve) = if samples == 1 {
            (&output, None)
        } else {
            (&source, Some(&output))
        };
        encode_clear(&context, target, resolve, 0.25)
            .submit()
            .unwrap();
        encode_clear(&context, target, resolve, 0.75).cancel();
        context
            .device()
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        let mut readback = Readback::new(
            context.device(),
            FrameConfig::new([3, 5], wgpu::TextureFormat::Rgba16Float, 1),
            100_000,
        )
        .unwrap();
        let bytes = readback
            .read(context.device(), context.submission(), &output)
            .unwrap();
        let expected: Vec<_> = [0x3400u16, 0x3800, 0x3c00, 0]
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect();
        assert!(bytes.chunks_exact(8).all(|pixel| pixel == expected));
    }
}

#[test]
fn gpu_all_source_products_require_identical_extent_and_coverage() {
    use worldrunner_render_scene::frame::metadata::validate_inputs;
    let context =
        pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty())).unwrap();
    for samples in [1, 4] {
        let normal = texture(&context, samples);
        let material = texture(&context, samples);
        let specular = texture(&context, samples);
        let depth = context.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("synthetic source depth"),
            size: normal.size(),
            mip_level_count: 1,
            sample_count: samples,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let sampling = MetadataSampling::PerSample(samples);
        assert!(
            validate_inputs(&depth, &normal, Some(&material), Some(&specular), sampling).is_ok()
        );
        assert!(validate_inputs(&normal, &depth, None, None, sampling).is_err());
        let other_samples = texture(&context, if samples == 1 { 4 } else { 1 });
        assert!(validate_inputs(&depth, &normal, Some(&other_samples), None, sampling).is_err());
        let small = context.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("mismatched extent"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: samples,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        assert!(validate_inputs(&depth, &normal, None, Some(&small), sampling).is_err());
        context.device().poll(wgpu::PollType::Poll).unwrap();
    }
}
