use super::*;

#[test]
fn opaque_auxiliary_is_not_a_water_target() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for samples in crate::msaa_tests::supported_samples(&context) {
        let mut renderer = regressions::renderer(&context, samples);
        renderer
            .install_water(context.device(), settings())
            .unwrap();
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
        let auxiliary = auxiliary_texture(context.device(), samples);
        let auxiliary =
            AttachmentView::new(ResourceRef::new("opaque auxiliary"), &auxiliary).unwrap();
        let mut opaque = PassDeclaration::new(PassSlot::Opaque);
        opaque.color = Some(crate::compositor::attachment(
            color.resource(),
            &color,
            false,
        ));
        opaque.depth = Some(crate::compositor::attachment(
            depth.resource(),
            &depth,
            true,
        ));
        opaque.additional_colors.push(crate::compositor::attachment(
            auxiliary.resource(),
            &auxiliary,
            false,
        ));
        let original = opaque.additional_colors.clone();
        let mut passes = vec![opaque];
        let mut views = vec![color, depth, auxiliary];
        renderer
            .water
            .as_ref()
            .unwrap()
            .feature(
                context.device(),
                &FrameUniform::new(&fixture_view(), None).unwrap(),
                (0.0, 0),
                None,
            )
            .schedule(&mut passes, &mut views)
            .unwrap();
        assert_eq!(passes[0].additional_colors, original);
        assert_eq!(passes[1].additional_colors.len(), 3);
        assert!(
            passes[1]
                .additional_colors
                .iter()
                .all(|a| a.resource.name != "opaque auxiliary")
        );
    }
}

#[test]
fn authored_surface_and_configured_environment_are_public() {
    use crate::compositor::ConfiguredFrameRenderer;
    let mut body = settings();
    body.material.normal = Some(worldrunner_render_plan::TextureHandle::from_raw(0));
    body.material.flow = Some(worldrunner_render_plan::TextureHandle::from_raw(1));
    assert!(body.validate().is_ok());
    std::hint::black_box(ConfiguredFrameRenderer::set_water_environment);
    std::hint::black_box(ConfiguredFrameRenderer::water_receivers);
}

fn auxiliary_texture(device: &wgpu::Device, samples: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("opaque auxiliary"),
        size: wgpu::Extent3d {
            width: 32,
            height: 32,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: samples,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}
