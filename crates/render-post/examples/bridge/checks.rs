use super::{id, recipe, texture};
use worldrunner_render_post::contracts::{
    ContractError, ResourceLifetime, ResourceRegistry, Semantic, TextureRange, TypedWork,
};

#[test]
fn negative_contracts() {
    let instance = wgpu::Instance::default();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    let (device, _) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let mut registry = ResourceRegistry::default();
    let output = texture(&device, 1);
    let (prefix, suffix, owned) = recipe(&device, &mut registry, &output, false);
    assert!(
        registry
            .compile_capture(&device, &prefix, &suffix, &[])
            .is_err()
    );
    check_packet_contracts(&device, &registry, &prefix, &suffix, owned);
    let mut undeclared = prefix.clone();
    if let TypedWork::Draws { pass, .. } = &mut undeclared[1].work {
        pass.uses.clear();
    }
    assert!(matches!(
        underlying(
            registry
                .compile_capture(&device, &undeclared, &suffix, &[owned])
                .err()
                .unwrap()
        ),
        ContractError::Binding(_)
    ));
    let mut stale = suffix.clone();
    if let TypedWork::Draws { pass, .. } = &mut stale[0].work {
        let old = &pass.uses[0].view;
        pass.uses[0] = worldrunner_render_post::contracts::ResourceUse::read(
            registry
                .view(old.allocation(), old.range().clone(), 0)
                .unwrap(),
            worldrunner_render_post::contracts::UseKind::Sampled,
        );
    }
    assert!(
        registry
            .compile_capture(&device, &prefix, &stale, &[owned])
            .is_err()
    );
    check_identity(&device, &mut registry, &output, owned);
}
fn check_identity(
    device: &wgpu::Device,
    registry: &mut ResourceRegistry,
    output: &wgpu::Texture,
    owned: worldrunner_render_post::contracts::AllocationKey,
) {
    let key = registry
        .import(
            &output.clone(),
            ResourceLifetime::PersistentMutable,
            Semantic::LinearColor,
            None,
        )
        .unwrap();
    assert_eq!(key, owned);
    assert!(matches!(
        registry.import(
            output,
            ResourceLifetime::ImportedReadOnly,
            Semantic::LinearColor,
            Some(0)
        ),
        Err(ContractError::Identity(_))
    ));
    let source = texture(device, 1);
    let key = registry
        .import(
            &source,
            ResourceLifetime::ImportedReadOnly,
            Semantic::LinearColor,
            Some(0),
        )
        .unwrap();
    let target = registry
        .view(key, TextureRange::single(0, 0, wgpu::TextureAspect::All), 1)
        .unwrap();
    let pass = super::pass("read-only consumer target", target, None);
    let op = worldrunner_render_post::contracts::TypedOperation::new(
        id("read-only consumer target"),
        TypedWork::Draws {
            pass,
            draws: vec![],
        },
    );
    assert!(matches!(
        underlying(registry.compile_operations(device, &[op]).err().unwrap()),
        ContractError::ReadOnly(_)
    ));
    let old = registry
        .view(
            owned,
            TextureRange::single(0, 0, wgpu::TextureAspect::All),
            1,
        )
        .unwrap();
    registry.replace(owned, &texture(device, 1)).unwrap();
    assert!(matches!(
        registry.check_view(&old),
        Err(ContractError::StaleAllocation(_))
    ));
}
fn underlying(error: ContractError) -> ContractError {
    match error {
        ContractError::Pass { source, .. } => *source,
        error => error,
    }
}

fn check_packet_contracts(
    device: &wgpu::Device,
    registry: &ResourceRegistry,
    prefix: &[worldrunner_render_post::contracts::TypedOperation],
    suffix: &[worldrunner_render_post::contracts::TypedOperation],
    owned: worldrunner_render_post::contracts::AllocationKey,
) {
    let mut many = prefix.to_vec();
    if let TypedWork::Draws { draws, .. } = &mut many[1].work {
        let stock = draws[0].clone();
        for _ in 0..8 {
            draws.insert(0, stock.clone());
        }
    }
    assert!(
        registry
            .compile_capture(device, &many, suffix, &[owned])
            .is_ok()
    );
    let mut mismatch = prefix.to_vec();
    if let TypedWork::Draws { pass, .. } = &mut mismatch[1].work {
        let color = pass.colors[0].as_mut().unwrap();
        color.view = color.resolve.take().unwrap();
    }
    assert!(matches!(
        underlying(
            registry
                .compile_capture(device, &mismatch, suffix, &[owned])
                .err()
                .unwrap()
        ),
        ContractError::Pipeline
    ));
}
