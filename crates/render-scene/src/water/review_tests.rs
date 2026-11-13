use super::*;

#[test]
fn opaque_auxiliary_is_not_a_water_target() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = GpuContext::shared_for_test().unwrap();
    for samples in crate::msaa_tests::supported_samples(&context) {
        let mut renderer = regressions::renderer(&context, samples);
        renderer.install_water(&context, settings()).unwrap();
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
        schedule_water(&context, &renderer, &mut passes, &mut views);
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

fn schedule_water(
    context: &GpuContext,
    renderer: &crate::compositor::FrameRenderer,
    passes: &mut Vec<PassDeclaration>,
    views: &mut Vec<AttachmentView>,
) {
    let encoder = context.submission().begin(context.device()).unwrap();
    renderer
        .water
        .as_ref()
        .unwrap()
        .feature(
            (context.device(), &*encoder),
            &FrameUniform::new(&fixture_view(), None).unwrap(),
            (0.0, 0),
            (None, None),
        )
        .schedule(passes, views)
        .unwrap();
}

// Relief card rejection attaches the Opaque depth read-only. The water pass copies the Opaque
// pass and its pipeline writes depth, so the copy must attach the same depth writable.
#[test]
fn water_writes_depth_after_a_read_only_opaque_depth() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = GpuContext::shared_for_test().unwrap();
    let mut renderer = regressions::renderer(&context, 1);
    renderer.install_water(&context, settings()).unwrap();
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
    let mut opaque = PassDeclaration::new(PassSlot::Opaque);
    opaque.color = Some(crate::compositor::attachment(
        color.resource(),
        &color,
        false,
    ));
    let mut shared = crate::compositor::attachment(depth.resource(), &depth, true);
    shared.load = AttachmentLoad::Load;
    shared.read_only = true;
    opaque.depth = Some(shared);
    let resource = depth.resource();
    let mut passes = vec![opaque];
    let mut views = vec![color, depth];
    schedule_water(&context, &renderer, &mut passes, &mut views);
    assert!(passes[0].depth.as_ref().unwrap().read_only);
    let water: Vec<_> = passes[1..]
        .iter()
        .map(|pass| pass.depth.as_ref().unwrap())
        .collect();
    assert!(!water.is_empty());
    for depth in water {
        assert_eq!(depth.resource, resource);
        assert_eq!(depth.load, AttachmentLoad::Load);
        assert!(!depth.read_only, "{depth:?}");
    }
}

#[test]
fn authored_surface_and_configured_environment_are_public() {
    use crate::compositor::ConfiguredFrameRenderer;
    let _session = crate::test_support::SceneTestSession::acquire();
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
