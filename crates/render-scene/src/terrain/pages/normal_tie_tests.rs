use super::{
    composition::{CompositionError, LayerSample, MaterialImage, MaterialTexel, compose_texel},
    gpu::CompositionBatch,
    mip_gpu::{PageInput, PageWriter},
    readback_tests::{assert_reads, encode_reads},
    residency::ResidentPages,
    residency_gpu_tests::producers,
    residency_tests::{context, settings},
    selection::PageKey,
};
use worldrunner_render_gpu::{configuration::AllocationBudget, submission::SubmissionAuthority};
use worldrunner_render_post::contracts::ResourceRegistry;

#[test]
fn prepared_gpu_antipodal_normals_publish_only_defined_reductions() {
    let context = context();
    let device = context.device();
    let authority = SubmissionAuthority::new(context.queue().clone());
    let mut parts = settings().parts();
    parts.interior = [4, 4];
    parts.border = 2;
    parts.upload_bytes_per_frame = 100_000;
    let settings = worldrunner_render_plan::TerrainPageSettings::new(parts).unwrap();
    let samples: Vec<_> = (0..64)
        .map(|i| {
            vec![LayerSample {
                color: [64, 128, 192, 255],
                normal: [if i % 2 == 0 { -i16::MAX } else { i16::MAX }; 3],
                roughness: 180,
                weight: 1,
            }]
        })
        .collect();
    let opposing = [samples[0][0], samples[1][0]];
    assert_eq!(compose_texel(&opposing), Err(CompositionError::Normal));
    let base = MaterialImage::new(
        [8, 8],
        samples
            .iter()
            .map(|layers| compose_texel(layers).unwrap())
            .collect(),
    )
    .unwrap();
    let expected = expected_mips(base);
    assert_eq!(expected[1].texels()[0].normal_roughness, [54, 54, 54, 180]);
    let batch = CompositionBatch::new(&samples).unwrap();
    let cache =
        ResidentPages::new(device, &authority, settings, 1, AllocationBudget(1_000_000)).unwrap();
    let writer = PageWriter::new(device, settings);
    let mut registry = ResourceRegistry::default();
    let (source, producers) = producers(device, &mut registry, &batch);
    let mut frame = authority.begin(device).unwrap();
    let pending = cache.reserve(&frame, PageKey::ROOT).unwrap();
    writer
        .encode_prepared(
            device,
            &mut frame,
            &cache,
            PageInput {
                registry: &mut registry,
                pending,
                source,
                producers,
            },
        )
        .unwrap();
    assert!(cache.lookup(PageKey::ROOT, 0).is_none());
    let reads = encode_reads(device, &mut frame, &cache.storage().texture, &expected, 0);
    frame.finish().submit().unwrap();
    assert!(cache.lookup(PageKey::ROOT, 0).is_some());
    assert_reads(device, &reads, &expected);
}

fn expected_mips(base: MaterialImage) -> Vec<MaterialImage> {
    let mut expected = vec![base];
    for extent in [[4, 4], [2, 2], [1, 1]] {
        expected.push(
            MaterialImage::new(
                extent,
                vec![
                    MaterialTexel {
                        color: [64, 128, 192, 255],
                        normal_roughness: [54, 54, 54, 180],
                    };
                    (extent[0] * extent[1]) as usize
                ],
            )
            .unwrap(),
        );
    }
    expected
}
