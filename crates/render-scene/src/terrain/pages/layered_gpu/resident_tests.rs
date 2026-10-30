use super::*;
use crate::terrain::pages::residency_tests::{context, settings};
use worldrunner_render_plan::*;

#[test]
fn uploads_only_positions_and_reuses_all_source_allocations() {
    let context = context();
    let (terrain, resources) = super::tests::fixture(
        TerrainSplatFilter::Bilinear,
        TerrainColorizationChannels::Rgb,
    );
    let source = LayeredGpuSource::new(&terrain, &resources).unwrap();
    let device = context.device();
    let mut registry = ResourceRegistry::default();
    let (_, first) = source
        .operations(device, context.queue(), &mut registry, &[[0.0; 2]])
        .unwrap();
    let (_, second) = source
        .operations(device, context.queue(), &mut registry, &[[1.0; 2]])
        .unwrap();
    for operations in [&first, &second] {
        let uploads: Vec<_> = operations
            .iter()
            .filter_map(|op| match &op.work {
                TypedWork::Upload { bytes, .. } => Some(bytes.len()),
                _ => None,
            })
            .collect();
        assert_eq!(uploads, [16]);
    }
    for index in 0..3 {
        assert_eq!(
            first[1].buffers[index].view.allocation(),
            second[1].buffers[index].view.allocation()
        );
    }
    for (a, b) in first[1].textures.iter().zip(&second[1].textures) {
        assert_eq!(a.view.allocation(), b.view.allocation());
    }
}

#[test]
fn residency_is_atomic_per_device_and_owned_by_the_source() {
    let first = context();
    let second = pollster::block_on(crate::device::GpuContext::from_adapter_with_features(
        first.adapter().clone(),
        wgpu::Features::empty(),
    ))
    .unwrap();
    assert_ne!(first.device(), second.device());
    let (terrain, resources) = super::tests::fixture(
        TerrainSplatFilter::Bilinear,
        TerrainColorizationChannels::Rgb,
    );
    let source = LayeredGpuSource::new(&terrain, &resources).unwrap();
    let barrier = std::sync::Barrier::new(4);
    let residents = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..4)
            .map(|_| {
                scope.spawn(|| {
                    barrier.wait();
                    source.resident(first.device(), first.queue()).unwrap()
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .collect::<Vec<_>>()
    });
    for resident in &residents {
        assert!(std::sync::Arc::ptr_eq(&residents[0], resident));
    }
    let other = source.resident(second.device(), second.queue()).unwrap();
    assert!(!std::sync::Arc::ptr_eq(&residents[0], &other));
    assert_ne!(residents[0].buffers, other.buffers);
    assert_eq!(source.resident.lock().unwrap().len(), 2);
    for context in [&first, &second] {
        let mut registry = ResourceRegistry::default();
        source
            .operations(
                context.device(),
                context.queue(),
                &mut registry,
                &[[0.0; 2]],
            )
            .unwrap();
    }
    let weak = std::sync::Arc::downgrade(&residents[0]);
    let other_weak = std::sync::Arc::downgrade(&other);
    drop(residents);
    drop(other);
    drop(source);
    assert!(weak.upgrade().is_none());
    assert!(other_weak.upgrade().is_none());
}

pub(super) fn compressed(format: u32, size: u32, mips: u32, block: &[u8]) -> SceneTexture {
    let bytes = (0..mips)
        .flat_map(|level| {
            let blocks = (size >> level).max(1).div_ceil(4).pow(2);
            (0..blocks).flat_map(|_| block.iter().copied())
        })
        .collect::<Vec<_>>();
    let mut header = vec![0; 148];
    header[..4].copy_from_slice(b"DDS ");
    header[4..8].copy_from_slice(&124_u32.to_le_bytes());
    header[12..16].copy_from_slice(&size.to_le_bytes());
    header[16..20].copy_from_slice(&size.to_le_bytes());
    header[28..32].copy_from_slice(&mips.to_le_bytes());
    header[84..88].copy_from_slice(b"DX10");
    header[128..132].copy_from_slice(&format.to_le_bytes());
    header.extend(bytes);
    SceneTexture::Encoded {
        encoding: TextureEncoding::DdsContainer,
        color_space: TextureColorSpace::Linear,
        bytes: header.into_boxed_slice(),
    }
}

#[test]
fn large_bc_palette_admission_counts_source_once_for_eight_uploads() {
    let mut source = LayeredGpuSource {
        parameters: vec![0; 64 + 12 * 64],
        grid: vec![0; 16],
        images: vec![],
        arrays: vec![],
        resident: std::sync::Mutex::default(),
        bounds: [0.0; 4],
    };
    for _ in 0..12 {
        for (format, bytes) in [(71, 8), (77, 16), (83, 16)] {
            super::textures::image(&mut source, &compressed(format, 2048, 12, &vec![0; bytes]))
                .unwrap();
        }
    }
    assert_eq!(source.arrays.len(), 3);
    let limits = wgpu::Limits::default();
    let source_bytes = source.allocations().validate(&limits).unwrap();
    assert_eq!(source_bytes, source.input_bytes());
    let mut parts = settings().parts();
    parts.upload_bytes_per_frame = settings().page_bytes() / 2 * 3 * 8;
    let settings = TerrainPageSettings::new(parts).unwrap();
    let allocations =
        super::super::residency::allocation::allocations(settings, 8, 3, &source.allocations())
            .unwrap();
    let bytes = allocations.validate(&limits).unwrap();
    assert_eq!(bytes, 167_791_984);
    assert!(bytes < 200 * 1024 * 1024);
}

#[test]
fn bc1_bc3_bc4_bc5_sources_remain_compressed_and_sampled() {
    let context = pollster::block_on(crate::device::GpuContext::request()).unwrap();
    for height_format in [80, 83] {
        let (terrain, base) = super::source_mip_tests::fixture(0, 1000, [1; 3]);
        let color = [0x00, 0xf8, 0x00, 0xf8, 0, 0, 0, 0];
        let normal = [
            255, 255, 0, 0, 0, 0, 0, 0, 0x10, 0x84, 0x10, 0x84, 0, 0, 0, 0,
        ];
        let height = vec![255; if height_format == 80 { 8 } else { 16 }];
        let resources = SceneResources::new(
            vec![
                compressed(71, 4, 1, &color),
                compressed(77, 4, 1, &normal),
                compressed(height_format, 4, 1, &height),
            ],
            base.materials().to_vec(),
        )
        .unwrap();
        let source = LayeredGpuSource::new(&terrain, &resources).unwrap();
        let actual = super::tests::execute(&context, &source, &[[0.1, 0.2]]);
        assert_eq!(&actual[..4], &[255, 0, 0, 255]);
        assert_eq!(actual[7], 255);
        assert!(actual[8] >= 254);
        assert_eq!(
            super::textures::payload_bytes(&source.arrays),
            24 + height.len() as u64
        );
    }
}

#[test]
fn heterogeneous_arrays_exceeding_the_stage_budget_fail_before_allocation() {
    let mut source = LayeredGpuSource {
        parameters: vec![],
        grid: vec![],
        images: vec![],
        arrays: vec![],
        resident: std::sync::Mutex::default(),
        bounds: [0.0; 4],
    };
    for size in (4..=28).step_by(4) {
        super::textures::image(&mut source, &compressed(71, size, 1, &[0; 8])).unwrap();
    }
    assert!(super::textures::image(&mut source, &compressed(71, 32, 1, &[0; 8])).is_err());
    assert!(source.resident.lock().unwrap().is_empty());
}
