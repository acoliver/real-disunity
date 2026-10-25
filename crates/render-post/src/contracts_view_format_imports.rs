use super::*;

fn descriptor() -> wgpu::TextureDescriptor<'static> {
    wgpu::TextureDescriptor {
        label: Some("declared texture fixture"),
        size: wgpu::Extent3d {
            width: 4,
            height: 4,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: LINEAR,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::STORAGE_BINDING,
        view_formats: &[],
    }
}

#[test]
fn foreign_invented_declarations_return_typed_errors_without_uncaptured_errors() {
    let (device, _) = device();
    let descriptor = descriptor();
    let texture = device.create_texture(&descriptor);
    for format in [SRGB, wgpu::TextureFormat::Rgba16Float] {
        let outer = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let declaration = wgpu::TextureDescriptor {
            view_formats: &[format],
            ..descriptor.clone()
        };
        assert_eq!(
            pollster::block_on(DeclaredTexture::from_texture(
                &device,
                &texture,
                &declaration
            ))
            .unwrap_err(),
            ContractError::TextureDeclaration(format)
        );
        device.poll(wgpu::PollType::Poll).unwrap();
        assert!(pollster::block_on(outer.pop()).is_none());
    }
}

#[test]
fn foreign_descriptors_must_match_every_queryable_property() {
    let (device, _) = device();
    let descriptor = descriptor();
    let texture = device.create_texture(&descriptor);
    let mut changed = vec![descriptor.clone(); 6];
    changed[0].format = SRGB;
    changed[1].size.width = 8;
    changed[2].mip_level_count = 2;
    changed[3].sample_count = 4;
    changed[4].dimension = wgpu::TextureDimension::D3;
    changed[5].usage = wgpu::TextureUsages::TEXTURE_BINDING;
    for declaration in changed {
        assert_eq!(
            pollster::block_on(DeclaredTexture::from_texture(
                &device,
                &texture,
                &declaration
            ))
            .unwrap_err(),
            ContractError::TextureDescriptor
        );
        device.poll(wgpu::PollType::Poll).unwrap();
    }
}

#[test]
fn creation_and_validated_foreign_import_capture_the_descriptor_declaration() {
    let (device, _) = device();
    let descriptor = wgpu::TextureDescriptor {
        view_formats: &[SRGB],
        ..descriptor()
    };
    let mut registry = ResourceRegistry::default();
    let created = registry
        .create(
            &device,
            &descriptor,
            ResourceLifetime::FrameLocal,
            Semantic::SrgbColor,
            None,
        )
        .unwrap();
    assert_eq!(selected(&registry, created, 1, SRGB).format(), SRGB);
    let texture = device.create_texture(&descriptor);
    let declared = pollster::block_on(DeclaredTexture::from_texture(
        &device,
        &texture,
        &descriptor,
    ))
    .unwrap();
    let imported = import(&mut registry, &declared);
    assert_eq!(selected(&registry, imported, 1, SRGB).format(), SRGB);
    device.poll(wgpu::PollType::Poll).unwrap();
}

#[test]
fn srgb_storage_bindings_and_usage_are_rejected_before_native_creation() {
    let (device, _) = device();
    let mut registry = ResourceRegistry::default();
    let texture = target(&device, 1, LINEAR, true);
    let key = import(&mut registry, &texture);
    let srgb = selected(&registry, key, 1, SRGB);
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let entry = TextureBinding {
        binding: 0,
        visibility: wgpu::ShaderStages::COMPUTE,
        view: srgb.clone(),
        samples: 1,
        semantic: Semantic::SrgbColor,
        kind: TextureBindingKind::Storage(wgpu::StorageTextureAccess::WriteOnly),
    };
    assert_eq!(
        CheckedBindings::build(&device, &registry, std::slice::from_ref(&entry)).unwrap_err(),
        ContractError::StorageAccess {
            allocation: key,
            format: SRGB,
            access: wgpu::StorageTextureAccess::WriteOnly,
        }
    );
    let native = selected(&registry, key, 1, LINEAR);
    CheckedBindings::build(
        &device,
        &registry,
        &[TextureBinding {
            view: native.clone(),
            ..entry
        }],
    )
    .unwrap();
    let mut raster = pass("restricted storage view", &[native]);
    raster.uses.push(ResourceUse::write(srgb, UseKind::Storage));
    assert!(matches!(
        registry.compile(&device, &[raster]),
        Err(ContractError::Pass { source, .. }) if *source == ContractError::Binding(key)
    ));
    device.poll(wgpu::PollType::Poll).unwrap();
    assert!(pollster::block_on(scope.pop()).is_none());
}
