#[path = "contracts_regressions.rs"]
mod regressions;

#[test]
fn mrt_rejects_mixed_samples_and_resolve_semantics() {
    let (device, _) = device();
    let mut registry = ResourceRegistry::default();
    let single = registry
        .import(
            &texture(&device, 1),
            ResourceLifetime::FrameLocal,
            Semantic::LinearColor,
            None,
        )
        .unwrap();
    let multi = registry
        .import(
            &texture(&device, 4),
            ResourceLifetime::FrameLocal,
            Semantic::LinearColor,
            None,
        )
        .unwrap();
    let exposed = registry
        .import(
            &texture(&device, 1),
            ResourceLifetime::FrameLocal,
            Semantic::ExposedLinearColor,
            None,
        )
        .unwrap();
    let mixed = pass(
        "mixed",
        &[view(&registry, single, 1), view(&registry, multi, 1)],
    );
    assert!(registry.compile(&device, &[mixed]).is_err());
    let mut raster = pass("semantic resolve", &[view(&registry, multi, 1)]);
    raster.colors[0].as_mut().unwrap().resolve = Some(view(&registry, exposed, 1));
    assert!(registry.compile(&device, &[raster]).is_err());
}

fn reader(
    device: &wgpu::Device,
    registry: &mut ResourceRegistry,
    name: &'static str,
) -> RasterPass {
    let key = registry
        .import(
            &texture(device, 1),
            ResourceLifetime::FrameLocal,
            Semantic::LinearColor,
            None,
        )
        .unwrap();
    pass(name, &[view(registry, key, 1)])
}

#[test]
fn empty_raster_has_no_attachment_contract() {
    let (device, _) = device();
    let registry = ResourceRegistry::default();
    assert!(registry.compile(&device, &[pass("empty", &[])]).is_err());
}

#[test]
fn stock_lowering_preserves_exact_clear_load_and_resolve_bytes() {
    let (device, queue) = device();
    for samples in [1, 4] {
        let config = crate::FrameConfig::new([4, 4], wgpu::TextureFormat::Rgba8Unorm, samples);
        let direct = crate::FrameTargets::new(&device, config).unwrap();
        let lowered = crate::FrameTargets::new(&device, config).unwrap();
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        for load in [wgpu::LoadOp::Clear(wgpu::Color::GREEN), wgpu::LoadOp::Load] {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: direct.scene(),
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
        }
        direct.encode_resolve(&mut encoder);
        encode_stock_fixture(&device, &mut encoder, &lowered);
        lowered.encode_resolve(&mut encoder);
        queue.submit([encoder.finish()]);
        let mut readback = crate::Readback::new(
            &device,
            crate::FrameConfig {
                samples: 1,
                ..config
            },
            4096,
        )
        .unwrap();
        let expected = readback
            .read(&device, &crate::test_authority(&queue), direct.texture())
            .unwrap();
        let actual = readback
            .read(&device, &crate::test_authority(&queue), lowered.texture())
            .unwrap();
        assert_eq!(actual, expected);
        assert!(
            actual
                .chunks_exact(4)
                .all(|pixel| pixel == [0, 255, 0, 255])
        );
    }
}

fn encode_stock_fixture(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    targets: &crate::FrameTargets,
) {
    let resource = crate::ResourceRef::new("stock fixture");
    let mut first = crate::PassDeclaration::new(crate::PassSlot::DepthNormalPrepass);
    first.color = Some(crate::AttachmentDeclaration {
        resource,
        format: targets.config().format,
        size: [4, 4],
        samples: targets.config().samples,
        load: AttachmentLoad::ClearColor(wgpu::Color::GREEN),
        store: wgpu::StoreOp::Store,
    });
    let mut second = first.clone();
    second.slot = crate::PassSlot::Opaque;
    second.color.as_mut().unwrap().load = AttachmentLoad::Load;
    let views = [crate::AttachmentView::new(resource, targets.scene().texture()).unwrap()];
    let compiled = crate::contracts::stock::compile(device, &[first, second], &views, &[]).unwrap();
    compiled.encode_stock_pass(0, encoder, None, |_| {});
    compiled.encode_stock_pass(1, encoder, None, |_| {});
}

#[test]
fn retired_backing_cannot_be_reimported_as_a_fresh_identity() {
    let (device, _) = device();
    let old_texture = texture(&device, 1);
    let mut registry = ResourceRegistry::default();
    let key = registry
        .import(
            &old_texture,
            ResourceLifetime::FrameLocal,
            Semantic::LinearColor,
            None,
        )
        .unwrap();
    registry.replace(key, &texture(&device, 1)).unwrap();
    assert!(
        registry
            .import(
                &old_texture,
                ResourceLifetime::FrameLocal,
                Semantic::LinearColor,
                None
            )
            .is_err()
    );
}

#[test]
fn mip_layer_validity_is_local_and_discard_invalidates_loads() {
    let (device, _) = device();
    let array = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("subresource fixture"),
        size: wgpu::Extent3d {
            width: 4,
            height: 4,
            depth_or_array_layers: 2,
        },
        mip_level_count: 2,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let mut registry = ResourceRegistry::default();
    let key = registry
        .import(
            &array,
            ResourceLifetime::FrameLocal,
            Semantic::LinearColor,
            None,
        )
        .unwrap();
    let selected = registry
        .view(key, TextureRange::single(1, 1, wgpu::TextureAspect::All), 1)
        .unwrap();
    let mut producer = pass("mip producer", std::slice::from_ref(&selected));
    let mut consumer = reader(&device, &mut registry, "read mip");
    consumer
        .uses
        .push(ResourceUse::read(selected.clone(), UseKind::Sampled));
    assert!(
        registry
            .compile(&device, &[producer.clone(), consumer.clone()])
            .is_ok()
    );
    consumer.uses[0].view = view(&registry, key, 1);
    assert!(
        registry
            .compile(&device, &[producer.clone(), consumer.clone()])
            .is_err()
    );
    consumer.uses[0].view = selected;
    producer.colors[0].as_mut().unwrap().store = wgpu::StoreOp::Discard;
    assert!(registry.compile(&device, &[producer, consumer]).is_err());
    assert!(
        registry
            .view(
                key,
                TextureRange::single(u32::MAX, 0, wgpu::TextureAspect::All),
                0
            )
            .is_err()
    );
}

#[test]
fn load_advances_versions_without_a_physical_replacement() {
    let (device, _) = device();
    let mut registry = ResourceRegistry::default();
    let key = registry
        .import(
            &texture(&device, 1),
            ResourceLifetime::PersistentMutable,
            Semantic::LinearColor,
            Some(4),
        )
        .unwrap();
    let mut loaded = pass("load", &[view(&registry, key, 5)]);
    let attachment = loaded.colors[0].as_mut().unwrap();
    attachment.load = AttachmentLoad::Load;
    attachment.before = Some(4);
    assert!(registry.compile(&device, &[loaded.clone()]).is_ok());
    loaded.colors[0].as_mut().unwrap().before = Some(3);
    assert!(registry.compile(&device, &[loaded]).is_err());
}

use crate::contracts::*;
use crate::{AttachmentLoad, StageId};

fn device() -> (wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap()
}

fn texture(device: &wgpu::Device, samples: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("contract fixture"),
        size: wgpu::Extent3d {
            width: 4,
            height: 4,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: samples,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
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

fn view(registry: &ResourceRegistry, key: AllocationKey, version: u32) -> CheckedView {
    registry
        .view(
            key,
            TextureRange::single(0, 0, wgpu::TextureAspect::All),
            version,
        )
        .unwrap()
}

fn pass(name: &'static str, views: &[CheckedView]) -> RasterPass {
    RasterPass {
        id: RasterId::new(StageId::new(name, 0), 0),
        colors: views
            .iter()
            .map(|view| {
                Some(ColorAttachment {
                    view: view.clone(),
                    before: None,
                    load: AttachmentLoad::ClearColor(wgpu::Color::RED),
                    store: wgpu::StoreOp::Store,
                    resolve: None,
                })
            })
            .collect(),
        depth: None,
        uses: Vec::new(),
        bindings: Vec::new(),
    }
}

#[test]
fn allocation_identity_generation_and_range_are_checked() {
    let (device, _queue) = device();
    let texture = texture(&device, 1);
    let mut registry = ResourceRegistry::default();
    let key = registry
        .import(
            &texture,
            ResourceLifetime::FrameLocal,
            Semantic::LinearColor,
            None,
        )
        .unwrap();
    assert_eq!(
        key,
        registry
            .import(
                &texture.clone(),
                ResourceLifetime::FrameLocal,
                Semantic::LinearColor,
                None
            )
            .unwrap()
    );
    assert!(
        registry
            .view(key, TextureRange::single(1, 0, wgpu::TextureAspect::All), 0)
            .is_err()
    );
    assert!(
        registry
            .view(key, TextureRange::single(0, 1, wgpu::TextureAspect::All), 0)
            .is_err()
    );
    assert!(
        registry
            .view(
                key,
                TextureRange::single(0, 0, wgpu::TextureAspect::DepthOnly),
                0
            )
            .is_err()
    );
    let old = view(&registry, key, 0);
    let compiled = registry
        .compile(&device, &[pass("stale", std::slice::from_ref(&old))])
        .unwrap();
    let next = registry
        .replace(key, &texture::replacement(&device))
        .unwrap();
    assert_eq!(key.id(), next.id());
    assert_ne!(key.generation(), next.generation());
    assert!(registry.check_view(&old).is_err());
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    assert!(
        compiled
            .encode(&registry, &mut encoder, |_, _| panic!(
                "stale pass executed"
            ))
            .is_err()
    );
}

mod texture {
    pub fn replacement(device: &wgpu::Device) -> wgpu::Texture {
        super::texture(device, 1)
    }
}

#[test]
fn missing_producers_stale_versions_and_aliases_fail() {
    let (device, _) = device();
    let mut registry = ResourceRegistry::default();
    let key = registry
        .import(
            &texture(&device, 1),
            ResourceLifetime::FrameLocal,
            Semantic::LinearColor,
            None,
        )
        .unwrap();
    let first = view(&registry, key, 1);
    let mut read = reader(&device, &mut registry, "reader");
    read.uses
        .push(ResourceUse::read(first.clone(), UseKind::Sampled));
    assert!(registry.compile(&device, &[read.clone()]).is_err());
    let producer = pass("producer", std::slice::from_ref(&first));
    assert!(
        registry
            .compile(&device, &[producer.clone(), read.clone()])
            .is_ok()
    );
    let overwrite = pass("overwrite", &[view(&registry, key, 2)]);
    assert!(
        registry
            .compile(&device, &[producer, overwrite, read])
            .is_err()
    );
    let mut alias = pass("alias", std::slice::from_ref(&first));
    alias.uses.push(ResourceUse::read(
        view(&registry, key, 99),
        UseKind::Sampled,
    ));
    assert!(registry.compile(&device, &[alias]).is_err());
}

#[test]
fn checked_bindings_reject_sample_semantic_and_undeclared_access() {
    let (device, _) = device();
    let mut registry = ResourceRegistry::default();
    let key = registry
        .import(
            &texture(&device, 1),
            ResourceLifetime::ImportedReadOnly,
            Semantic::LinearColor,
            Some(0),
        )
        .unwrap();
    let input = view(&registry, key, 0);
    let mut binding = TextureBinding {
        binding: 0,
        visibility: wgpu::ShaderStages::FRAGMENT,
        view: input.clone(),
        samples: 4,
        semantic: Semantic::LinearColor,
        kind: TextureBindingKind::Sampled(wgpu::TextureSampleType::Float { filterable: true }),
    };
    assert!(CheckedBindings::build(&device, &registry, &[binding.clone()]).is_err());
    binding.samples = 1;
    binding.semantic = Semantic::DeviceDepth;
    assert!(CheckedBindings::build(&device, &registry, &[binding.clone()]).is_err());
    binding.semantic = Semantic::LinearColor;
    let checked = CheckedBindings::build(&device, &registry, &[binding]).unwrap();
    let mut consumer = pass("consumer", &[]);
    assert!(checked.validate(&registry, &consumer).is_err());
    consumer
        .uses
        .push(ResourceUse::read(input, UseKind::Sampled));
    assert!(checked.validate(&registry, &consumer).is_ok());
    let write = pass("illegal write", &[view(&registry, key, 1)]);
    assert!(registry.compile(&device, &[write]).is_err());
    assert_eq!(
        registry.replace(key, &texture(&device, 1)),
        Err(ContractError::ReadOnlyReplacement(key))
    );
    assert!(checked.validate(&registry, &consumer).is_ok());
}

#[test]
fn authored_shadow_prepass_orders_are_not_sorted() {
    let (device, _) = device();
    let mut registry = ResourceRegistry::default();
    let keys: Vec<_> = (0..2)
        .map(|_| {
            registry
                .import(
                    &texture(&device, 1),
                    ResourceLifetime::FrameLocal,
                    Semantic::LinearColor,
                    None,
                )
                .unwrap()
        })
        .collect();
    let shadow = pass("shadow", &[view(&registry, keys[0], 1)]);
    let prepass = pass("prepass", &[view(&registry, keys[1], 1)]);
    for passes in [
        vec![shadow.clone(), prepass.clone()],
        vec![prepass.clone(), shadow.clone()],
    ] {
        let compiled = registry.compile(&device, &passes).unwrap();
        assert_eq!(
            compiled.ids().collect::<Vec<_>>(),
            passes.iter().map(|pass| pass.id).collect::<Vec<_>>()
        );
    }
}

#[test]
fn mrt_resolves_execute_at_one_and_four_samples() {
    let (device, queue) = device();
    for samples in [1, 4] {
        let mut registry = ResourceRegistry::default();
        let outputs: Vec<_> = (0..3).map(|_| texture(&device, 1)).collect();
        let output_views: Vec<_> = outputs
            .iter()
            .map(|texture| {
                let key = registry
                    .import(
                        texture,
                        ResourceLifetime::FrameLocal,
                        Semantic::LinearColor,
                        None,
                    )
                    .unwrap();
                view(&registry, key, 1)
            })
            .collect();
        let render_views = if samples == 1 {
            output_views.clone()
        } else {
            (0..3)
                .map(|_| {
                    let key = registry
                        .import(
                            &texture(&device, samples),
                            ResourceLifetime::FrameLocal,
                            Semantic::LinearColor,
                            None,
                        )
                        .unwrap();
                    view(&registry, key, 1)
                })
                .collect()
        };
        let mut raster = pass("mrt", &render_views);
        for (index, color) in raster.colors.iter_mut().enumerate() {
            let color = color.as_mut().unwrap();
            color.load = AttachmentLoad::ClearColor(
                [wgpu::Color::RED, wgpu::Color::GREEN, wgpu::Color::BLUE][index],
            );
            if samples > 1 {
                color.resolve = Some(output_views[index].clone());
            }
        }
        let compiled = registry.compile(&device, &[raster.clone()]).unwrap();
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        compiled.encode(&registry, &mut encoder, |_, _| {}).unwrap();
        let submission = queue.submit([encoder.finish()]);
        device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(std::time::Duration::from_secs(10)),
            })
            .unwrap();
        assert_mrt_outputs(&device, &queue, outputs);
        raster.colors[0].as_mut().unwrap().resolve = Some(render_views[0].clone());
        assert!(registry.compile(&device, &[raster]).is_err());
    }
}

fn assert_mrt_outputs(device: &wgpu::Device, queue: &wgpu::Queue, outputs: Vec<wgpu::Texture>) {
    for (index, output) in outputs.into_iter().enumerate() {
        let mut bytes = crate::Readback::new(
            device,
            crate::FrameConfig::new([4, 4], wgpu::TextureFormat::Rgba8Unorm, 1),
            4096,
        )
        .unwrap();
        let data = bytes
            .read(device, &crate::test_authority(queue), &output)
            .unwrap();
        let expected = [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]][index];
        assert!(data.chunks_exact(4).all(|pixel| pixel == expected));
    }
}
