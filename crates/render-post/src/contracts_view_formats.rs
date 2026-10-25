#[path = "contracts_view_format_imports.rs"]
mod imports;

#[test]
fn imported_view_declarations_preserve_identity_and_do_not_survive_replacement() {
    let (device, _) = device();
    let mut registry = ResourceRegistry::default();
    let texture = target(&device, 1, LINEAR, true);
    let key = import(&mut registry, &texture);
    assert_eq!(import(&mut registry, &texture.clone()), key);
    assert_eq!(
        registry.import(
            texture.texture(),
            ResourceLifetime::FrameLocal,
            Semantic::SrgbColor,
            None
        ),
        Err(ContractError::Identity(key))
    );
    let next = registry
        .replace(key, target(&device, 1, LINEAR, false).texture())
        .unwrap();
    assert_eq!(selected(&registry, next, 1, LINEAR).format(), LINEAR);
    assert_eq!(
        registry
            .view_format(
                next,
                TextureRange::single(0, 0, wgpu::TextureAspect::All),
                1,
                wgpu::TextureViewDimension::D2,
                SRGB
            )
            .unwrap_err(),
        ContractError::ViewFormat {
            allocation: next,
            format: SRGB
        }
    );
}

#[test]
fn native_views_keep_wgpu_aspect_specific_defaults() {
    let (device, _) = device();
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("aspect view fixture"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth24PlusStencil8,
        usage: wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let mut registry = ResourceRegistry::default();
    let key = registry
        .import(
            &texture,
            ResourceLifetime::ImportedReadOnly,
            Semantic::DeviceDepth,
            Some(0),
        )
        .unwrap();
    for aspect in [
        wgpu::TextureAspect::DepthOnly,
        wgpu::TextureAspect::StencilOnly,
    ] {
        let view = registry
            .view(key, TextureRange::single(0, 0, aspect), 0)
            .unwrap();
        assert_eq!(view.format(), texture.format());
        registry.check_view(&view).unwrap();
        device.poll(wgpu::PollType::Poll).unwrap();
    }
}

use super::*;

const LINEAR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const SRGB: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

fn target(
    device: &wgpu::Device,
    samples: u32,
    format: wgpu::TextureFormat,
    declared: bool,
) -> DeclaredTexture {
    DeclaredTexture::new(
        device,
        &wgpu::TextureDescriptor {
            label: Some("checked view format fixture"),
            size: wgpu::Extent3d {
                width: 4,
                height: 4,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: samples,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | if samples == 1 {
                    wgpu::TextureUsages::COPY_SRC
                        | if format == LINEAR {
                            wgpu::TextureUsages::STORAGE_BINDING
                                | wgpu::TextureUsages::TEXTURE_BINDING
                        } else {
                            wgpu::TextureUsages::empty()
                        }
                } else {
                    wgpu::TextureUsages::empty()
                },
            view_formats: if declared { &[SRGB] } else { &[] },
        },
    )
}

fn import(registry: &mut ResourceRegistry, texture: &DeclaredTexture) -> AllocationKey {
    registry
        .import_declared(
            texture,
            ResourceLifetime::FrameLocal,
            Semantic::SrgbColor,
            None,
        )
        .unwrap()
}

fn selected(
    registry: &ResourceRegistry,
    key: AllocationKey,
    version: u32,
    format: wgpu::TextureFormat,
) -> CheckedView {
    registry
        .view_format(
            key,
            TextureRange::single(0, 0, wgpu::TextureAspect::All),
            version,
            wgpu::TextureViewDimension::D2,
            format,
        )
        .unwrap()
}

fn draw(device: &wgpu::Device, format: wgpu::TextureFormat, samples: u32) -> CheckedDraw {
    // Decode byte centers on the CPU so GPU sRGB encoding stays away from rounding boundaries.
    let linear = [32_u8, 128, 224].map(|byte| {
        let encoded = f64::from(byte) / 255.0;
        ((encoded + 0.055) / 1.055).powf(2.4)
    });
    let shader = format!(
        "@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {{ let p = array(vec2<f32>(-1., -1.), vec2<f32>(3., -1.), vec2<f32>(-1., 3.)); return vec4<f32>(p[i], 0., 1.); }} @fragment fn fs() -> @location(0) vec4<f32> {{ return vec4<f32>({}, {}, {}, 1.); }}",
        linear[0], linear[1], linear[2]
    );
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(shader.into()),
    });
    CheckedDraw::new(
        device,
        &wgpu::RenderPipelineDescriptor {
            label: None,
            layout: None,
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: samples,
                ..Default::default()
            },
            multiview_mask: None,
            cache: None,
        },
        vec![],
        0..3,
        0..1,
    )
    .unwrap()
}

fn output_pass(
    device: &wgpu::Device,
    registry: &mut ResourceRegistry,
    samples: u32,
    output: CheckedView,
) -> RasterPass {
    if samples == 1 {
        return pass("view format output", std::slice::from_ref(&output));
    }
    let multi = target(device, samples, LINEAR, true);
    let key = import(registry, &multi);
    let mut raster = pass("view format output", &[selected(registry, key, 1, SRGB)]);
    raster.colors[0].as_mut().unwrap().resolve = Some(output);
    raster
}

fn assert_storage_view_usage(output: &DeclaredTexture, output_view: &CheckedView) {
    assert!(
        !output_view
            .usage()
            .contains(wgpu::TextureUsages::STORAGE_BINDING)
    );
    assert!(
        output
            .texture()
            .usage()
            .contains(wgpu::TextureUsages::STORAGE_BINDING)
    );
}

#[test]
fn declared_srgb_views_render_exact_cpu_derived_bytes_and_preserve_ids() {
    let (device, queue) = device();
    for samples in [1, 4] {
        let mut registry = ResourceRegistry::default();
        let output = target(&device, 1, LINEAR, true);
        let key = import(&mut registry, &output);
        let output_view = selected(&registry, key, 1, SRGB);
        assert_eq!(output_view.format(), SRGB);
        assert_storage_view_usage(&output, &output_view);
        assert_eq!(view(&registry, key, 1).format(), LINEAR);
        assert_eq!(
            registry
                .view_dimension(
                    key,
                    output_view.range().clone(),
                    1,
                    wgpu::TextureViewDimension::D2
                )
                .unwrap()
                .format(),
            LINEAR
        );
        let mut raster = output_pass(&device, &mut registry, samples, output_view);
        raster.id = RasterId::new(StageId::new("view format output", 7), 3);
        let id = raster.id;
        let plan = registry
            .compile_operations(
                &device,
                &[TypedOperation::new(
                    id,
                    TypedWork::Draws {
                        pass: raster,
                        draws: vec![draw(&device, SRGB, samples)],
                    },
                )],
            )
            .unwrap();
        assert_eq!(plan.ids().collect::<Vec<_>>(), vec![id]);
        let authority = crate::test_authority(&queue);
        authority
            .prepare(&device, |encoder, _| {
                plan.encode_checked(&registry, encoder, None)
            })
            .unwrap()
            .submit()
            .unwrap();
        let bytes = crate::Readback::new(&device, crate::FrameConfig::new([4, 4], LINEAR, 1), 4096)
            .unwrap()
            .read(&device, &authority, output.texture())
            .unwrap();
        assert!(
            bytes
                .chunks_exact(4)
                .all(|pixel| pixel == [32, 128, 224, 255]),
            "{bytes:?}"
        );
        device.poll(wgpu::PollType::Poll).unwrap();
    }
}

#[test]
fn undeclared_format_is_a_typed_error_but_native_format_is_allowed() {
    let (device, _) = device();
    let mut registry = ResourceRegistry::default();
    let key = registry
        .import(
            target(&device, 1, LINEAR, false).texture(),
            ResourceLifetime::FrameLocal,
            Semantic::SrgbColor,
            None,
        )
        .unwrap();
    assert_eq!(selected(&registry, key, 0, LINEAR).format(), LINEAR);
    let error = registry
        .view_format(
            key,
            TextureRange::single(0, 0, wgpu::TextureAspect::All),
            0,
            wgpu::TextureViewDimension::D2,
            SRGB,
        )
        .unwrap_err();
    assert_eq!(
        error,
        ContractError::ViewFormat {
            allocation: key,
            format: SRGB
        }
    );
}

#[test]
fn pipeline_must_match_view_in_both_directions() {
    let (device, _) = device();
    let mut registry = ResourceRegistry::default();
    let key = import(&mut registry, &target(&device, 1, LINEAR, true));
    for (view_format, pipeline_format) in [(SRGB, LINEAR), (LINEAR, SRGB)] {
        let raster = pass(
            "mismatched view",
            &[selected(&registry, key, 1, view_format)],
        );
        let id = raster.id;
        let result = registry.compile_operations(
            &device,
            &[TypedOperation::new(
                id,
                TypedWork::Draws {
                    pass: raster,
                    draws: vec![draw(&device, pipeline_format, 1)],
                },
            )],
        );
        assert!(
            matches!(result, Err(ContractError::Pass { source, .. }) if *source == ContractError::Pipeline)
        );
        device.poll(wgpu::PollType::Poll).unwrap();
    }
}

#[test]
fn resolve_compatibility_uses_views_instead_of_storage() {
    let (device, _) = device();
    let mut registry = ResourceRegistry::default();
    let multi = import(&mut registry, &target(&device, 4, LINEAR, true));
    let output = import(&mut registry, &target(&device, 1, LINEAR, true));
    let native_srgb = import(&mut registry, &target(&device, 1, SRGB, false));
    let mut raster = pass("view resolve", &[selected(&registry, multi, 1, SRGB)]);
    raster.colors[0].as_mut().unwrap().resolve = Some(view(&registry, output, 1));
    assert!(registry.compile(&device, &[raster.clone()]).is_err());
    raster.colors[0].as_mut().unwrap().resolve = Some(view(&registry, native_srgb, 1));
    assert!(registry.compile(&device, &[raster]).is_ok());
}

#[test]
fn explicit_formats_keep_range_and_dimension_checks() {
    let (device, _) = device();
    let mut registry = ResourceRegistry::default();
    let key = import(&mut registry, &target(&device, 1, LINEAR, true));
    for range in [
        TextureRange::single(1, 0, wgpu::TextureAspect::All),
        TextureRange::single(0, 1, wgpu::TextureAspect::All),
        TextureRange::single(0, 0, wgpu::TextureAspect::DepthOnly),
    ] {
        assert_eq!(
            registry
                .view_format(key, range, 1, wgpu::TextureViewDimension::D2, SRGB)
                .unwrap_err(),
            ContractError::Range(key)
        );
    }
    assert_eq!(
        registry
            .view_format(
                key,
                TextureRange::single(0, 0, wgpu::TextureAspect::All),
                1,
                wgpu::TextureViewDimension::Cube,
                SRGB
            )
            .unwrap_err(),
        ContractError::Range(key)
    );
}

#[test]
fn explicit_formats_keep_generation_and_version_checks() {
    let (device, _) = device();
    let mut registry = ResourceRegistry::default();
    let key = import(&mut registry, &target(&device, 1, LINEAR, true));
    let old = selected(&registry, key, 1, SRGB);
    let producer = pass("first version", std::slice::from_ref(&old));
    let mut load = pass("next version", &[selected(&registry, key, 2, SRGB)]);
    let attachment = load.colors[0].as_mut().unwrap();
    attachment.load = AttachmentLoad::Load;
    attachment.before = Some(1);
    assert!(
        registry
            .compile(&device, &[producer.clone(), load.clone()])
            .is_ok()
    );
    load.colors[0].as_mut().unwrap().before = Some(0);
    assert!(
        matches!(registry.compile(&device, &[producer.clone(), load.clone()]), Err(ContractError::Pass { source, .. }) if *source == ContractError::Producer(key))
    );
    load.colors[0].as_mut().unwrap().before = Some(1);
    load.colors[0].as_mut().unwrap().view = selected(&registry, key, 1, SRGB);
    assert!(registry.compile(&device, &[producer, load]).is_err());
    registry
        .replace(key, target(&device, 1, LINEAR, true).texture())
        .unwrap();
    assert_eq!(
        registry.check_view(&old),
        Err(ContractError::StaleAllocation(key))
    );
    assert_eq!(
        registry
            .view_format(
                key,
                old.range().clone(),
                2,
                wgpu::TextureViewDimension::D2,
                SRGB
            )
            .unwrap_err(),
        ContractError::StaleAllocation(key)
    );
}
