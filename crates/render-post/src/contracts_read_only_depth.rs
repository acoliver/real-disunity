use super::*;

/// One pass's depth: the version it loads, if any, and the version it attaches.
#[derive(Debug, PartialEq, Eq)]
struct PassDepth {
    loads: Option<u32>,
    attaches: u32,
}

const fn pass(loads: Option<u32>, attaches: u32) -> PassDepth {
    PassDepth { loads, attaches }
}

const FORMATS: [wgpu::TextureFormat; 2] = [
    wgpu::TextureFormat::Depth32Float,
    wgpu::TextureFormat::Depth32FloatStencil8,
];

#[test]
fn read_only_stock_depth_is_sampled_in_its_own_pass_without_a_new_version() {
    for (device, format) in devices() {
        let gl = device.adapter_info().backend == wgpu::Backend::Gl;
        let result = compile(&device, format, true, AttachmentLoad::Load, true);
        if gl {
            assert!(matches!(result, Err(ContractError::Pass { source, .. })
                if matches!(*source, ContractError::DepthSampling(_))));
            continue;
        }
        assert_eq!(
            result.unwrap(),
            vec![
                Some(pass(None, 0)),
                Some(pass(Some(0), 0)),
                Some(pass(Some(0), 1)),
            ],
            "{format:?}"
        );
    }
}

#[test]
fn writable_stock_depth_cannot_be_sampled_in_its_own_pass() {
    for (device, format) in devices() {
        let result = compile(&device, format, false, AttachmentLoad::Load, true);
        assert!(matches!(result, Err(ContractError::Pass { source, .. })
            if matches!(*source, ContractError::Hazard(_))));
    }
}

#[test]
fn read_only_stock_depth_must_load() {
    for (device, format) in devices() {
        let result = compile(
            &device,
            format,
            true,
            AttachmentLoad::ClearDepth(1.0),
            false,
        );
        assert!(matches!(result, Err(ContractError::Pass { source, .. })
            if matches!(*source, ContractError::Raster(_))));
    }
}

/// Frame validation runs before the contracts compile, so it must admit the same sampled
/// read of a read-only loaded depth and refuse it for a writable or cleared attachment.
#[test]
fn frame_validation_admits_sampling_only_a_read_only_loaded_depth() {
    let validate = |read_only, load| {
        let (passes, reads) = declarations(FORMATS[0], read_only, load, true);
        crate::validate_frame(&passes, &[reads], &[])
    };
    assert_eq!(validate(true, AttachmentLoad::Load), Ok(()));
    let hazard = Err(crate::FrameError::AttachmentHazard(RESOURCE));
    assert_eq!(validate(false, AttachmentLoad::Load), hazard);
    assert_eq!(validate(true, AttachmentLoad::ClearDepth(1.0)), hazard);
}

/// The featureless device for float depth, plus a combined depth-stencil device when the
/// adapter offers one.
fn devices() -> Vec<(wgpu::Device, wgpu::TextureFormat)> {
    let (plain, _) = device();
    let (combined, _) =
        crate::gpu_tests::gpu_with_optional_features(wgpu::Features::DEPTH32FLOAT_STENCIL8);
    let mut devices = vec![(plain, FORMATS[0])];
    if combined
        .features()
        .contains(wgpu::Features::DEPTH32FLOAT_STENCIL8)
    {
        devices.push((combined, FORMATS[1]));
    }
    devices
}

const RESOURCE: crate::ResourceRef = crate::ResourceRef::new("read-only depth fixture");

/// The prepass clears and stores; `Opaque` attaches the same depth with `opaque_load`;
/// `TransparentAlpha` loads and writes after it. The feature samples the depth in `Opaque`
/// when `sampled` holds.
fn declarations(
    format: wgpu::TextureFormat,
    read_only: bool,
    opaque_load: AttachmentLoad,
    sampled: bool,
) -> (Vec<crate::PassDeclaration>, crate::FeatureDeclaration) {
    let depth = crate::AttachmentDeclaration {
        resource: RESOURCE,
        format,
        size: [4, 4],
        samples: 1,
        load: AttachmentLoad::ClearDepth(1.0),
        store: wgpu::StoreOp::Store,
        read_only: false,
    };
    let mut prepass = crate::PassDeclaration::new(crate::PassSlot::DepthNormalPrepass);
    prepass.depth = Some(depth.clone());
    let mut opaque = crate::PassDeclaration::new(crate::PassSlot::Opaque);
    opaque.depth = Some(crate::AttachmentDeclaration {
        load: opaque_load,
        read_only,
        ..depth.clone()
    });
    let mut transparent = crate::PassDeclaration::new(crate::PassSlot::TransparentAlpha);
    transparent.depth = Some(crate::AttachmentDeclaration {
        load: AttachmentLoad::Load,
        ..depth
    });
    let mut reads = crate::FeatureDeclaration::new(crate::PassSlot::Opaque);
    if sampled {
        reads.passes[0].reads.push(RESOURCE);
    }
    (vec![prepass, opaque, transparent], reads)
}

fn compile(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    read_only: bool,
    opaque_load: AttachmentLoad,
    sampled: bool,
) -> Result<Vec<Option<PassDepth>>, ContractError> {
    let texture = depth_texture(device, format);
    let (passes, reads) = declarations(format, read_only, opaque_load, sampled);
    crate::contracts::stock::compile(
        device,
        &passes,
        &[crate::AttachmentView::new(RESOURCE, &texture).unwrap()],
        &[reads],
        &mut ResourceRegistry::default(),
    )
    .map(|compiled| {
        compiled
            .depth_versions()
            .into_iter()
            .map(|version| version.map(|(loads, attaches)| pass(loads, attaches)))
            .collect()
    })
}

fn depth_texture(device: &wgpu::Device, format: wgpu::TextureFormat) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("read-only depth fixture"),
        size: wgpu::Extent3d {
            width: 4,
            height: 4,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}
