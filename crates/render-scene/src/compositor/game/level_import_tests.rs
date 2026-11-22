//! What an import of one level of a declared texture or target refuses, by name, and what it
//! lends, on a device that records nothing.
use super::LevelAccess;
use crate::compositor::GameTextureDeclaration;
use crate::compositor::game::resources::SharedTextures;
use crate::compositor::game::{
    GameLevel, GameResourceError, GameResources, GameTargets, GameTexture, TargetDeclaration,
    TargetExtent, TargetStore,
};
use std::sync::Arc;
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_gpu::submission::SubmissionAuthority;
use worldrunner_render_post::contracts::TextureRange;
use worldrunner_render_program::binding::EngineSemantic;
use worldrunner_render_program::runtime::{Import, ImportedResource};

fn device() -> (wgpu::Device, SubmissionAuthority) {
    let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor {
        required_features: wgpu::Features::TEXTURE_COMPRESSION_BC,
        ..wgpu::DeviceDescriptor::default()
    });
    (device, SubmissionAuthority::new(queue))
}

fn store(device: &wgpu::Device, authority: &SubmissionAuthority) -> GameResources {
    GameResources::new(
        device,
        authority,
        AllocationBudget(1 << 20),
        SharedTextures::default(),
    )
}

/// A zeroed 16 x 16 `Rgba8Unorm` texture of `dimension`, two layers (or slices) of four mips.
fn declare(
    resources: &GameResources,
    label: &str,
    dimension: wgpu::TextureViewDimension,
) -> GameTexture {
    resources
        .declare_texture(&GameTextureDeclaration {
            label,
            format: wgpu::TextureFormat::Rgba8Unorm,
            size: [16, 16, 2],
            dimension,
            mip_levels: 4,
            texels: None,
        })
        .unwrap()
}

/// A four-layer `R32Float` target a kernel may write, and a two-layer, two-mip BC4 copy target
/// it may only sample.
fn targets(device: &wgpu::Device) -> GameTargets {
    let declarations = [
        TargetDeclaration::new(
            "probe.storage",
            wgpu::TextureFormat::R32Float,
            TargetExtent::Fixed([8, 4]),
        )
        .layers(4)
        .with_usage(wgpu::TextureUsages::STORAGE_BINDING),
        TargetDeclaration::new(
            "probe.sampled",
            wgpu::TextureFormat::Bc4RUnorm,
            TargetExtent::Fixed([8, 4]),
        )
        .layers(2)
        .mips(2),
    ];
    GameTargets::new(Arc::new(TargetStore::build(
        device,
        None,
        &declarations,
        [8, 4],
    )))
}

fn semantic() -> EngineSemantic {
    EngineSemantic::named("probe.level")
}

const fn level(layer: u32, mip: u32) -> GameLevel {
    GameLevel { layer, mip }
}

/// The label and reason of a refused import.
fn refused(result: Result<Import, GameResourceError>) -> (String, String) {
    match result {
        Err(GameResourceError::Import { label, reason }) => (label, reason),
        other => panic!("expected a refused import, got {other:?}"),
    }
}

/// Checks an import lends `texture` through a 2D view to the binding of [`semantic`].
fn assert_lends_2d(import: &Import, texture: &wgpu::Texture) {
    assert_eq!(import.semantic, semantic());
    let ImportedResource::Texture(lent) = &import.resource else {
        panic!("a texture is lent, got {:?}", import.resource)
    };
    assert_eq!(&lent.texture, texture);
    assert_eq!(lent.dimension, wgpu::TextureViewDimension::D2);
}

#[test]
fn a_texture_of_another_store_or_an_undeclared_target_is_refused() {
    let (device, authority) = device();
    let (ours, theirs) = (store(&device, &authority), store(&device, &authority));
    let texture = declare(&theirs, "theirs", wgpu::TextureViewDimension::D2Array);
    assert_eq!(
        ours.import_texture_level(semantic(), texture, level(0, 0), LevelAccess::Sampled),
        Err(GameResourceError::Foreign("game texture"))
    );
    let targets = targets(&device);
    let (label, reason) =
        refused(targets.import_level("probe.none", semantic(), level(0, 0), LevelAccess::Sampled));
    assert_eq!(label, "probe.none");
    assert!(reason.contains("probe.none"), "{reason}");
}

#[test]
fn a_layer_or_mip_outside_the_texture_or_target_is_refused_by_name() {
    let (device, authority) = device();
    let resources = store(&device, &authority);
    let pages = declare(&resources, "pages", wgpu::TextureViewDimension::D2Array);
    let sampled = LevelAccess::Sampled;
    let (label, reason) =
        refused(resources.import_texture_level(semantic(), pages, level(1, 4), sampled));
    assert_eq!(label, "pages");
    assert!(reason.contains("mip 4"), "{reason}");
    let (label, reason) =
        refused(resources.import_texture_level(semantic(), pages, level(2, 3), sampled));
    assert_eq!(label, "pages");
    assert!(reason.contains("layer 2"), "{reason}");
    let targets = targets(&device);
    let storage = LevelAccess::Storage;
    let (label, reason) =
        refused(targets.import_level("probe.storage", semantic(), level(4, 0), storage));
    assert_eq!(label, "probe.storage");
    assert!(reason.contains("layer 4"), "{reason}");
    let (label, reason) =
        refused(targets.import_level("probe.storage", semantic(), level(2, 1), storage));
    assert_eq!(label, "probe.storage");
    assert!(reason.contains("mip 1"), "{reason}");
    let (label, reason) =
        refused(targets.import_level("probe.sampled", semantic(), level(1, 2), sampled));
    assert_eq!(label, "probe.sampled");
    assert!(reason.contains("mip 2"), "{reason}");
}

#[test]
fn a_binding_the_texture_or_target_was_not_declared_for_is_refused_by_name() {
    let (device, authority) = device();
    let resources = store(&device, &authority);
    let pages = declare(&resources, "pages", wgpu::TextureViewDimension::D2Array);
    let (label, reason) = refused(resources.import_texture_level(
        semantic(),
        pages,
        level(1, 1),
        LevelAccess::Storage,
    ));
    assert_eq!(label, "pages");
    assert!(reason.contains("STORAGE_BINDING"), "{reason}");
    let targets = targets(&device);
    let (label, reason) = refused(targets.import_level(
        "probe.sampled",
        semantic(),
        level(1, 0),
        LevelAccess::Storage,
    ));
    assert_eq!(label, "probe.sampled");
    assert!(reason.contains("STORAGE_BINDING"), "{reason}");
    let volume = declare(&resources, "volume", wgpu::TextureViewDimension::D3);
    let (label, reason) = refused(resources.import_texture_level(
        semantic(),
        volume,
        level(0, 0),
        LevelAccess::Sampled,
    ));
    assert_eq!(label, "volume");
    assert!(reason.contains("D3"), "{reason}");
}

#[test]
fn a_level_in_range_is_lent_as_a_two_dimensional_view_of_its_texture() {
    let (device, authority) = device();
    let resources = store(&device, &authority);
    let pages = declare(&resources, "pages", wgpu::TextureViewDimension::D2Array);
    let import = resources
        .import_texture_level(semantic(), pages, level(1, 3), LevelAccess::Sampled)
        .unwrap();
    let ImportedResource::Texture(lent) = &import.resource else {
        panic!("a texture is lent")
    };
    assert_eq!(lent.texture.mip_level_count(), 4);
    assert_lends_2d(&import, &lent.texture);
    let targets = targets(&device);
    let storage = targets.texture("probe.storage").unwrap();
    let import = targets
        .import_level(
            "probe.storage",
            semantic(),
            level(3, 0),
            LevelAccess::Storage,
        )
        .unwrap();
    assert_lends_2d(&import, &storage);
    let sampled = targets.texture("probe.sampled").unwrap();
    let import = targets
        .import_level(
            "probe.sampled",
            semantic(),
            level(1, 1),
            LevelAccess::Sampled,
        )
        .unwrap();
    assert_lends_2d(&import, &sampled);
}

#[test]
fn a_target_declared_with_a_usage_its_format_cannot_have_is_refused_by_name() {
    let limits = wgpu::Limits::default();
    let declare = |name, format| {
        TargetDeclaration::new(name, format, TargetExtent::Fixed([8, 4]))
            .with_usage(wgpu::TextureUsages::STORAGE_BINDING)
    };
    let refused = declare("probe.srgb", wgpu::TextureFormat::Rgba8UnormSrgb)
        .validate((&limits, wgpu::Features::empty()), [8, 4])
        .expect_err("an sRGB format has no storage binding")
        .to_string();
    assert!(refused.contains("probe.srgb"), "{refused}");
    assert!(refused.contains("STORAGE_BINDING"), "{refused}");
    declare("probe.float", wgpu::TextureFormat::R32Float)
        .validate((&limits, wgpu::Features::empty()), [8, 4])
        .expect("R32Float is a storage image");
}

#[test]
fn a_level_is_lent_with_the_range_of_that_level_alone() {
    let (device, authority) = device();
    let resources = store(&device, &authority);
    let pages = declare(&resources, "pages", wgpu::TextureViewDimension::D2Array);
    let declared = resources
        .import_texture_level(semantic(), pages, level(1, 3), LevelAccess::Sampled)
        .unwrap();
    let target = targets(&device)
        .import_level(
            "probe.storage",
            semantic(),
            level(2, 0),
            LevelAccess::Storage,
        )
        .unwrap();
    for (import, (layer, mip)) in [(declared, (1, 3)), (target, (2, 0))] {
        let ImportedResource::Texture(lent) = &import.resource else {
            panic!("a texture is lent, got {:?}", import.resource)
        };
        assert_eq!(
            lent.range,
            TextureRange::single(mip, layer, wgpu::TextureAspect::All),
            "a kernel that writes the level writes only it"
        );
    }
}

/// A level imported for an array binding is lent as a one-layer `D2Array` view of its texture
/// with the range of that level alone, and refused as a `D2` import is.
#[test]
fn a_level_for_an_array_binding_is_lent_as_a_one_layer_array_view() {
    let (device, _) = device();
    let targets = targets(&device);
    let storage = targets.texture("probe.storage").unwrap();
    let import = targets
        .import_level_array(
            "probe.storage",
            semantic(),
            level(2, 0),
            LevelAccess::Storage,
        )
        .unwrap();
    assert_eq!(import.semantic, semantic());
    let ImportedResource::Texture(lent) = &import.resource else {
        panic!("a texture is lent, got {:?}", import.resource)
    };
    assert_eq!(&lent.texture, &storage);
    assert_eq!(lent.dimension, wgpu::TextureViewDimension::D2Array);
    assert_eq!(
        lent.range,
        TextureRange::single(0, 2, wgpu::TextureAspect::All)
    );
    let (label, reason) = refused(targets.import_level_array(
        "probe.storage",
        semantic(),
        level(4, 0),
        LevelAccess::Storage,
    ));
    assert_eq!(label, "probe.storage");
    assert!(reason.contains("layer 4"), "{reason}");
    let (label, reason) = refused(targets.import_level_array(
        "probe.sampled",
        semantic(),
        level(0, 0),
        LevelAccess::Storage,
    ));
    assert_eq!(label, "probe.sampled");
    assert!(reason.contains("STORAGE_BINDING"), "{reason}");
}
