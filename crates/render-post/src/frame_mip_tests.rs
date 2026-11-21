//! The frame's checks treat each mip of a texture as a subresource of its own: a pass may sample
//! mip m-1 and render into mip m of one texture, and may not sample the mip it renders into.
use crate::*;

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R32Float;
const SIDE: u32 = 8;

fn mip(mip: u32) -> ResourceRef {
    ResourceRef {
        name: "pyramid",
        version: 0,
        mip,
        layer: 0,
    }
}

fn texture(device: &wgpu::Device) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("pyramid"),
        size: wgpu::Extent3d {
            width: SIDE,
            height: SIDE,
            depth_or_array_layers: 1,
        },
        mip_level_count: 3,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

fn slot() -> PassSlot {
    PassSlot::Game(GamePlace::BeforeOpaque)
}

/// The pass that renders into `mip` with the extent declared, clearing it.
fn writing(mip: u32, size: u32) -> PassDeclaration {
    let mut pass = PassDeclaration::nth(slot(), mip);
    pass.color = Some(AttachmentDeclaration {
        resource: self::mip(mip),
        format: FORMAT,
        size: [size, size],
        samples: 1,
        load: AttachmentLoad::ClearColor(wgpu::Color::BLACK),
        store: wgpu::StoreOp::Store,
        read_only: false,
    });
    pass
}

fn pyramid() -> Vec<PassDeclaration> {
    (0..3).map(|level| writing(level, SIDE >> level)).collect()
}

/// A feature that has the pass of each mip in `reads` sample the mips it lists.
fn reading(reads: &[(u32, &[u32])]) -> Vec<FeatureDeclaration> {
    reads
        .iter()
        .map(|(level, mips)| FeatureDeclaration {
            passes: vec![FeaturePass {
                reads: mips.iter().map(|read| mip(*read)).collect(),
                color_format: Some(FORMAT),
                ..FeaturePass::nth(slot(), *level)
            }],
            effects: Vec::new(),
        })
        .collect()
}

#[test]
fn a_pass_may_sample_the_mip_above_while_it_renders_into_its_own() {
    let reads = reading(&[(1, &[0]), (2, &[1])]);
    assert_eq!(validate_frame(&pyramid(), &reads, &[]), Ok(()));
}

#[test]
fn a_pass_that_samples_a_mip_nothing_wrote_is_refused_by_that_mip() {
    let passes = [writing(0, SIDE), writing(2, SIDE >> 2)];
    assert_eq!(
        validate_frame(&passes, &reading(&[(2, &[1])]), &[]),
        Err(FrameError::MissingProducer(mip(1))),
        "mip 1 has no writer before the pass of mip 2"
    );
    let late = [writing(1, SIDE >> 1), writing(0, SIDE)];
    assert_eq!(
        validate_frame(&late, &reading(&[(1, &[0])]), &[]),
        Err(FrameError::MissingProducer(mip(0))),
        "the writer of mip 0 is listed after its reader"
    );
}

#[test]
fn a_pass_that_samples_the_mip_it_renders_into_is_a_hazard_by_that_mip() {
    let reads = reading(&[(1, &[0, 1])]);
    assert_eq!(
        validate_frame(&pyramid()[..2], &reads, &[]),
        Err(FrameError::AttachmentHazard(mip(1)))
    );
}

#[test]
fn the_views_of_different_mips_of_one_texture_agree_with_each_mips_extent() {
    let (device, _queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let texture = texture(&device);
    let views: Vec<_> = (0..3)
        .map(|level| AttachmentView::new(mip(level), &texture).unwrap())
        .collect();
    assert_eq!(views[2].size(), [2, 2]);
    assert_eq!(crate::frame::validate_views(&pyramid(), &views), Ok(()));
    let mut oversize = pyramid();
    oversize[1].color.as_mut().unwrap().size = [SIDE, SIDE];
    assert_eq!(
        crate::frame::validate_views(&oversize, &views),
        Err(FrameError::AttachmentView(mip(1))),
        "a pass declares the extent of the mip it renders into"
    );
    assert!(AttachmentView::new(mip(3), &texture).is_err());
}
