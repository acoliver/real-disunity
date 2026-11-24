//! The plan checks name the same entry and resource whichever way the frame is indexed: the
//! first repeated entry in list order, the first unauthored feature pass in declaration order,
//! and the reads each pass samples in the order its features declare them.
use super::*;

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const SLOT: PassSlot = PassSlot::Game(GamePlace::BeforePost);

fn page(layer: u32) -> ResourceRef {
    ResourceRef {
        name: "page",
        version: 0,
        mip: 0,
        layer,
    }
}

fn clearing(resource: ResourceRef) -> AttachmentDeclaration {
    AttachmentDeclaration {
        resource,
        format: FORMAT,
        size: [4, 4],
        samples: 1,
        load: AttachmentLoad::ClearColor(wgpu::Color::BLACK),
        store: wgpu::StoreOp::Store,
        read_only: false,
    }
}

/// `count` passes of one role, pass `n` clearing page `n`, and features of four feature passes
/// each, where the feature pass of pass `n` reads page `n - 1`.
fn chain(count: u32) -> (Vec<PassDeclaration>, Vec<FeatureDeclaration>) {
    let passes: Vec<_> = (0..count)
        .map(|index| {
            let mut pass = PassDeclaration::nth(SLOT, index);
            pass.color = Some(clearing(page(index)));
            pass
        })
        .collect();
    let feature_passes: Vec<_> = (0..count)
        .map(|index| {
            let mut feature = FeaturePass::nth(SLOT, index);
            feature.color_format = Some(FORMAT);
            if index > 0 {
                feature.reads = vec![page(index - 1)];
            }
            feature
        })
        .collect();
    let features = feature_passes
        .chunks(4)
        .map(|chunk| FeatureDeclaration {
            passes: chunk.to_vec(),
            effects: Vec::new(),
        })
        .collect();
    (passes, features)
}

#[test]
fn a_frame_one_unproduced_read_away_from_a_valid_frame_is_refused_by_that_read() {
    let (passes, mut features) = chain(40);
    assert_eq!(validate_frame(&passes, &features, &[]), Ok(()));

    let unwritten = ResourceRef::new("unwritten");
    let valid = std::mem::replace(&mut features[6].passes[1].reads, vec![unwritten]);
    assert_eq!(
        validate_frame(&passes, &features, &[]),
        Err(FrameError::MissingProducer(unwritten))
    );

    features[6].passes[1].reads = valid;
    assert_eq!(validate_frame(&passes, &features, &[]), Ok(()));

    // Page 26 is written by pass 26, which runs after pass 25 of feature 6's second pass.
    features[6].passes[1].reads = vec![page(26)];
    assert_eq!(
        validate_frame(&passes, &features, &[]),
        Err(FrameError::MissingProducer(page(26)))
    );
}

#[test]
fn the_first_pass_that_repeats_an_entry_is_the_one_named() {
    let entry = |occurrence| PassEntry::nth(SLOT, occurrence);
    let listed = |occurrences: &[u32]| {
        occurrences
            .iter()
            .map(|&occurrence| PassDeclaration::nth(SLOT, occurrence))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        validate_frame(&listed(&[0, 1, 1, 0]), &[], &[]),
        Err(FrameError::DuplicatePass(entry(1))),
        "entry 1 repeats at position 2, before entry 0 does at position 3"
    );
    assert_eq!(
        validate_frame(&listed(&[0, 1, 0, 1]), &[], &[]),
        Err(FrameError::DuplicatePass(entry(0)))
    );
}

#[test]
fn the_first_feature_pass_in_declaration_order_that_names_an_absent_entry_is_the_one_named() {
    let passes = [PassDeclaration::nth(SLOT, 0)];
    let drawing = |occurrences: &[u32]| FeatureDeclaration {
        passes: occurrences
            .iter()
            .map(|&occurrence| FeaturePass::nth(SLOT, occurrence))
            .collect(),
        effects: vec!["effect"],
    };
    let features = [drawing(&[0, 7]), drawing(&[3, 0])];
    assert_eq!(
        validate_frame(&passes, &features, &[]),
        Err(FrameError::MissingPass(PassEntry::nth(SLOT, 7)))
    );
}

#[test]
fn the_checks_run_in_the_order_repeats_effects_and_absent_entries() {
    let repeated = [PassDeclaration::nth(SLOT, 0), PassDeclaration::nth(SLOT, 0)];
    let absent = [FeatureDeclaration {
        passes: vec![FeaturePass::nth(SLOT, 5)],
        effects: Vec::new(),
    }];
    assert_eq!(
        validate_frame(&repeated, &absent, &["unprovided"]),
        Err(FrameError::DuplicatePass(PassEntry::nth(SLOT, 0)))
    );
    assert_eq!(
        validate_frame(&repeated[..1], &absent, &["unprovided"]),
        Err(FrameError::UnsupportedEffect("unprovided"))
    );
    assert_eq!(
        validate_frame(&repeated[..1], &absent, &[]),
        Err(FrameError::MissingPass(PassEntry::nth(SLOT, 5)))
    );
}

#[test]
fn a_pass_samples_what_its_features_declare_in_declaration_order() {
    let (device, _queue) = crate::gpu_tests::gpu();
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("entry features"),
        size: wgpu::Extent3d {
            width: 4,
            height: 4,
            depth_or_array_layers: 3,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let views: Vec<_> = (0..3)
        .map(|layer| AttachmentView::new(page(layer), &texture).unwrap())
        .collect();
    let (passes, _) = chain(3);
    let reading = |occurrence, layers: &[u32]| {
        let mut feature = FeaturePass::nth(SLOT, occurrence);
        feature.color_format = Some(FORMAT);
        feature.reads = layers.iter().map(|&layer| page(layer)).collect();
        feature
    };
    // Pass 2 is named by the first and the last feature pass, with pass 1's between them in the
    // first feature, so grouping by entry must keep each pass's reads in declaration order.
    let features = [
        FeatureDeclaration {
            passes: vec![reading(2, &[0]), reading(1, &[0])],
            effects: Vec::new(),
        },
        FeatureDeclaration {
            passes: vec![reading(2, &[1])],
            effects: Vec::new(),
        },
    ];
    let mut registry = crate::contracts::ResourceRegistry::default();
    let raster =
        crate::contracts::stock::compile(&device, &passes, &views, &features, &mut registry)
            .unwrap();
    let sampled: Vec<_> = raster
        .sampled_views()
        .into_iter()
        .map(|view| view.range().layers.start)
        .collect();
    assert_eq!(
        sampled,
        [0, 0, 1],
        "pass 1 samples page 0, pass 2 pages 0 then 1"
    );
}

#[test]
fn a_view_that_repeats_a_resource_or_a_subresource_is_refused_by_the_first_such_view() {
    let (device, _queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let texture = || {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("entry features"),
            size: wgpu::Extent3d {
                width: 4,
                height: 4,
                depth_or_array_layers: 4,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
    };
    let (first, second) = (texture(), texture());
    let view = |texture: &wgpu::Texture, name: &'static str, layer| {
        let resource = ResourceRef {
            name,
            version: 0,
            mip: 0,
            layer,
        };
        AttachmentView::new(resource, texture).unwrap()
    };
    assert_eq!(
        validate_views(
            &[],
            &[
                view(&first, "a", 0),
                view(&first, "b", 1),
                view(&second, "a", 0),
                view(&second, "c", 2),
                view(&second, "c", 2),
            ]
        ),
        Err(FrameError::AttachmentHazard(
            view(&second, "a", 0).resource()
        )),
        "another texture's view names the resource of an earlier view"
    );
    assert_eq!(
        validate_views(
            &[],
            &[
                view(&first, "a", 0),
                view(&first, "b", 0),
                view(&first, "c", 1),
                view(&second, "c", 1),
            ]
        ),
        Err(FrameError::AttachmentHazard(
            view(&first, "b", 0).resource()
        )),
        "the layer of the texture repeats under another name before a name repeats"
    );
    assert_eq!(
        validate_views(
            &[],
            &[
                view(&first, "a", 0),
                view(&first, "b", 1),
                view(&second, "c", 0)
            ]
        ),
        Ok(())
    );
}
