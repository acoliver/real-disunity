//! What a block transfer or a readback asked of game resources refuses before anything is
//! recorded, on a device that records nothing.
use super::{GameBlockDestination, GameLevel, GameResourceError, GameResources, GameTexture};
use crate::compositor::GameTextureDeclaration;
use crate::compositor::game::resources::SharedTextures;
use crate::compositor::game::{GameTargets, TargetDeclaration, TargetExtent, TargetStore};
use std::sync::Arc;
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_gpu::submission::SubmissionAuthority;
use worldrunner_render_program::runtime::BlockSource;

/// A device that accepts every format, and two stores of resources on it.
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

/// A zeroed 16 x 16 texture of `format`, two layers of four mips.
fn declare(resources: &GameResources, label: &str, format: wgpu::TextureFormat) -> GameTexture {
    resources
        .declare_texture(&GameTextureDeclaration {
            label,
            format,
            size: [16, 16, 2],
            dimension: wgpu::TextureViewDimension::D2Array,
            mip_levels: 4,
            texels: None,
        })
        .unwrap()
}

/// One block image texel of the runtime, the source of a BC5 transfer.
fn source(resources: &GameResources) -> BlockSource {
    let texture = resources
        .runtime()
        .create_texture_2d("blocks", [1, 1], wgpu::TextureFormat::Rgba32Uint, &[0; 16])
        .unwrap();
    BlockSource {
        texture,
        mip: 0,
        layer: 0,
        origin: [0, 0],
        extent: [1, 1],
    }
}

fn into(texture: GameTexture, layer: u32, mip: u32) -> GameBlockDestination {
    GameBlockDestination {
        texture,
        mip,
        layer,
        extent: [(16 >> mip).max(1), (16 >> mip).max(1)],
    }
}

/// The targets a game declared: a two-layer `R32Float` target and a two-layer, two-mip BC4
/// copy target that can be read back, and an `R32Float` target that cannot.
fn targets(device: &wgpu::Device) -> GameTargets {
    let declarations = [
        TargetDeclaration::new(
            "probe.float",
            wgpu::TextureFormat::R32Float,
            TargetExtent::Fixed([8, 4]),
        )
        .layers(2)
        .with_usage(wgpu::TextureUsages::COPY_SRC),
        TargetDeclaration::new(
            "probe.pages",
            wgpu::TextureFormat::Bc4RUnorm,
            TargetExtent::Fixed([8, 4]),
        )
        .layers(2)
        .mips(2)
        .with_usage(wgpu::TextureUsages::COPY_SRC),
        TargetDeclaration::new(
            "probe.plain",
            wgpu::TextureFormat::R32Float,
            TargetExtent::Fixed([8, 4]),
        ),
    ];
    GameTargets::new(Arc::new(TargetStore::build(
        device,
        None,
        &declarations,
        [8, 4],
    )))
}

/// The label and reason of a refused readback.
fn readback<T: std::fmt::Debug>(result: Result<T, GameResourceError>) -> (String, String) {
    match result {
        Err(GameResourceError::Readback { label, reason }) => (label, reason),
        other => panic!("expected a refused readback, got {other:?}"),
    }
}

/// The label and reason of a refused transfer.
fn transfer(result: Result<(), GameResourceError>) -> (String, String) {
    match result {
        Err(GameResourceError::Transfer { label, reason }) => (label, reason),
        other => panic!("expected a refused transfer, got {other:?}"),
    }
}

#[test]
fn a_handle_of_another_store_is_refused() {
    let (device, authority) = device();
    let (ours, theirs) = (store(&device, &authority), store(&device, &authority));
    let texture = declare(&theirs, "theirs", wgpu::TextureFormat::Bc5RgUnorm);
    let foreign = Err(GameResourceError::Foreign("game texture"));
    assert_eq!(
        ours.read_back_texture(texture, GameLevel::default())
            .map(|_| ()),
        foreign
    );
    assert_eq!(
        ours.transfer_block_image(&source(&ours), into(texture, 0, 0)),
        foreign
    );
    let ticket = theirs
        .read_back_texture(texture, GameLevel::default())
        .unwrap();
    assert_eq!(
        ours.take_read_back(ticket),
        Err(GameResourceError::Foreign("game readback"))
    );
    assert_eq!(theirs.take_read_back(ticket), Ok(None), "not copied yet");
}

#[test]
fn a_level_outside_the_texture_or_target_is_refused_by_name() {
    let (device, authority) = device();
    let resources = store(&device, &authority);
    let pages = declare(&resources, "pages", wgpu::TextureFormat::Bc5RgUnorm);
    let level = |layer, mip| GameLevel { layer, mip };
    let (label, reason) = readback(resources.read_back_texture(pages, level(0, 4)));
    assert_eq!(label, "pages");
    assert!(reason.contains("mip 4"), "{reason}");
    let (label, reason) = readback(resources.read_back_texture(pages, level(2, 3)));
    assert_eq!(label, "pages");
    assert!(reason.contains("layer 2"), "{reason}");
    let blocks = source(&resources);
    let (label, reason) = transfer(resources.transfer_block_image(&blocks, into(pages, 0, 4)));
    assert_eq!(label, "pages");
    assert!(reason.contains("mip 4"), "{reason}");
    let (label, reason) = transfer(resources.transfer_block_image(&blocks, into(pages, 2, 3)));
    assert_eq!(label, "pages");
    assert!(reason.contains("layer 2"), "{reason}");
    let targets = targets(&device);
    let (label, reason) =
        readback(resources.read_back_target(&targets, "probe.float", level(1, 1)));
    assert_eq!(label, "probe.float");
    assert!(reason.contains("mip 1"), "{reason}");
    let (label, reason) =
        readback(resources.read_back_target(&targets, "probe.pages", level(1, 2)));
    assert_eq!(label, "probe.pages");
    assert!(reason.contains("mip 2"), "{reason}");
    let (label, reason) =
        readback(resources.read_back_target(&targets, "probe.float", level(2, 0)));
    assert_eq!(label, "probe.float");
    assert!(reason.contains("layer 2"), "{reason}");
    let (label, _) = readback(resources.read_back_target(&targets, "probe.none", level(0, 0)));
    assert_eq!(
        label, "probe.none",
        "an undeclared target is refused by name"
    );
    assert!(resources.read_back_texture(pages, level(1, 3)).is_ok());
    assert!(
        resources
            .read_back_target(&targets, "probe.float", level(1, 0))
            .is_ok()
    );
    assert!(
        resources
            .read_back_target(&targets, "probe.pages", level(1, 1))
            .is_ok()
    );
}

#[test]
fn a_format_or_target_that_cannot_be_read_back_is_refused_by_name() {
    let (device, authority) = device();
    let resources = store(&device, &authority);
    let depth = resources
        .declare_texture(&GameTextureDeclaration {
            label: "depth",
            format: wgpu::TextureFormat::Depth24Plus,
            size: [4, 4, 1],
            dimension: wgpu::TextureViewDimension::D2,
            mip_levels: 1,
            texels: None,
        })
        .unwrap();
    let (label, reason) = readback(resources.read_back_texture(depth, GameLevel::default()));
    assert_eq!(label, "depth");
    assert!(reason.contains("Depth24Plus"), "{reason}");
    let targets = targets(&device);
    let (label, reason) =
        readback(resources.read_back_target(&targets, "probe.plain", GameLevel::default()));
    assert_eq!(label, "probe.plain");
    assert!(reason.contains("COPY_SRC"), "{reason}");
    let colour = declare(&resources, "colour", wgpu::TextureFormat::Rgba8Unorm);
    let (label, reason) =
        transfer(resources.transfer_block_image(&source(&resources), into(colour, 0, 0)));
    assert_eq!(label, "colour");
    assert!(reason.contains("not block-compressed"), "{reason}");
}
