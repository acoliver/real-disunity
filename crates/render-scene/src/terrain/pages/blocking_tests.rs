use super::{
    composition::{LayerSample, MaterialImage, MaterialTexel, compose_page},
    mip_gpu::PageWriter,
    residency::ResidentPages,
    residency_tests::{context, settings},
    selection::PageKey,
};
use worldrunner_render_gpu::{configuration::AllocationBudget, submission::SubmissionAuthority};
use worldrunner_render_plan::TerrainPageSettings;

#[test]
fn blocking_b1_odd_non_affine_neighbors_remain_continuous() {
    let context = context();
    let device = context.device();
    let authority = SubmissionAuthority::new(context.queue().clone());
    let mut parts = settings().parts();
    parts.interior = [3, 3];
    parts.border = 4;
    parts.upload_bytes_per_frame = 100_000;
    let settings = TerrainPageSettings::new(parts).unwrap();
    let cache =
        ResidentPages::new(device, &authority, settings, 5, AllocationBudget(1_000_000)).unwrap();
    let writer = PageWriter::new(device, settings);
    let leaves = [[0, 0], [1, 0], [0, 1], [1, 1]].map(|xy| PageKey::new(1, xy).unwrap());
    for key in [PageKey::ROOT].into_iter().chain(leaves) {
        let base = compose_page(settings, key, |uv| {
            vec![LayerSample {
                color: [if uv[0] < 0.5 { 0 } else { 255 }, 64, 192, 255],
                normal: [0, 0, i16::MAX],
                roughness: 180,
                weight: 1,
            }]
        })
        .unwrap();
        let mut frame = authority.begin(device).unwrap();
        let pending = cache.reserve(&frame, key).unwrap();
        writer
            .encode(device, &mut frame, &cache, pending, &base)
            .unwrap();
        let submission = frame.finish().submit().unwrap();
        device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: None,
            })
            .unwrap();
    }
    for derivative in [0.0, 2.0_f32.sqrt() / 6.0, 2.0 / 6.0] {
        let bytes = super::sampling_tests::harness::sample(
            device,
            &authority,
            &cache,
            &leaves,
            &[
                [0.5 - 0.000_001, 0.25, derivative, 0.0],
                [0.5 + 0.000_001, 0.25, derivative, 0.0],
            ],
        );
        assert!(
            bytes[0].abs_diff(bytes[4]) <= 1,
            "derivative {derivative}: {bytes:?}"
        );
    }
    assert_eq!(settings.maximum_sampling_mip(), 0);
    assert_eq!(settings.mip_extents(), [[11, 11], [5, 5], [2, 2], [1, 1]]);
}

#[test]
fn blocking_b2_perspective_refines_visible_before_behind_eye() {
    let mut parts = settings().parts();
    parts.maximum_selected_pages = 7;
    let selected = super::view::select_volume(
        TerrainPageSettings::new(parts).unwrap(),
        None,
        &super::view::PageView {
            matrix: glam::camera::rh::proj::directx::perspective(1.0, 1.0, 0.01, 100.0)
                * glam::camera::rh::view::look_at_mat4(
                    glam::Vec3::ZERO,
                    glam::Vec3::X,
                    glam::Vec3::Z,
                ),
            viewport: [1024, 1024],
            origin: [-3.0, -1.0],
            extent: [4.0, 2.0],
            elevation: [-0.1, 0.1],
        },
    );
    assert!(
        selected
            .leaves()
            .contains(&PageKey::new(1, [0, 0]).unwrap()),
        "behind-eye child must remain coarse: {selected:?}"
    );
    assert!(
        !selected
            .leaves()
            .contains(&PageKey::new(1, [1, 0]).unwrap()),
        "visible crossing child must win refinement"
    );
}

#[test]
fn blocking_b3_fragment_lookup_work_is_depth_bounded() {
    let shader = include_str!("sampling.wgsl");
    assert!(
        !shader.contains("i < page_table.counts"),
        "fragment lookup must not scan leaf or resident counts"
    );
    assert!(
        shader.contains("level <= 16u"),
        "indexed lookup must have an explicit maximum-depth bound"
    );
}

fn warm_cache_scene(
    device: &wgpu::Device,
    authority: &SubmissionAuthority,
    slots: u32,
    leaves: u32,
    depth: u8,
) -> (super::scene::ScenePages, Vec<PageKey>) {
    use super::scene::{ScenePages, TerrainPageSource};
    use std::collections::BTreeSet;

    let mut parts = settings().parts();
    parts.maximum_selected_pages = leaves;
    parts.maximum_depth = depth;
    parts.resident_bytes = 1_000_000;
    parts.upload_bytes_per_frame = 1_000_000;
    let settings = TerrainPageSettings::new(parts).unwrap();
    let mut pages = ScenePages::new(
        device,
        authority,
        TerrainPageSource {
            settings,
            slots,
            budget: AllocationBudget(10_000_000),
            sample: Box::new(|_| panic!("warm preparation must not compose pages")),
        },
        [0.0, 0.0, 1.0, 1.0],
    )
    .unwrap();
    pages.select(glam::Mat4::IDENTITY, [4096, 4096]);
    let selection = pages.selection.as_ref().unwrap();
    assert_eq!(selection.leaves().len(), leaves as usize);
    let protected: BTreeSet<_> = selection
        .leaves()
        .iter()
        .flat_map(|key| (0..=key.depth()).map(|depth| key.ancestor(depth).unwrap()))
        .collect();
    let mut keys = protected.clone();
    let side = 1 << depth;
    for y in 0..side {
        for x in 0..side {
            if keys.len() < slots as usize {
                keys.insert(PageKey::new(depth, [x, y]).unwrap());
            }
        }
    }
    assert_eq!(keys.len(), slots as usize);
    let frame = authority.begin(device).unwrap();
    for key in keys {
        pages
            .cache
            .reserve(&frame, key)
            .unwrap()
            .stage(&frame)
            .unwrap();
    }
    let submission = frame.finish().submit().unwrap();
    device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: None,
        })
        .unwrap();
    (pages, protected.into_iter().collect())
}

#[test]
fn blocking_b4_warm_cache_slot_work_grows_linearly() {
    use worldrunner_render_gpu::device::{DeviceRequirements, request_device};

    let context = context();
    let (device, queue, _) = pollster::block_on(request_device(
        context.adapter(),
        &DeviceRequirements {
            required_limits: wgpu::Limits {
                max_texture_array_layers: 800,
                ..Default::default()
            },
            ..Default::default()
        },
    ))
    .unwrap();
    let authority = SubmissionAuthority::new(queue);
    let mut measured = Vec::new();
    for (slots, leaves, depth) in [(100, 64, 4), (400, 256, 5)] {
        let (mut pages, protected) = warm_cache_scene(&device, &authority, slots, leaves, depth);
        pages.cache.storage().state.borrow().take_slot_inspections();
        let mut frame = authority.begin(&device).unwrap();
        pages
            .prepare(&device, &mut frame, glam::Mat4::IDENTITY, [4096, 4096])
            .unwrap();
        assert!(pages.ready());
        assert_eq!(pages.draw_residents.len(), protected.len());
        let residents = pages.cache.residents();
        assert_eq!(residents.len(), slots as usize);
        assert!(residents.iter().all(|page| pages.cache.is_current(*page)));
        let victim = pages.cache.evict_unselected(&protected).unwrap();
        assert!(!protected.contains(&victim));
        let stale = residents.iter().find(|page| page.key() == victim).unwrap();
        assert!(!pages.cache.is_current(*stale));
        let work = pages.cache.storage().state.borrow().take_slot_inspections();
        println!("warm cache: {slots} residents, {leaves} leaves, {work} slot inspections");
        assert!(
            work <= 6 * slots as usize,
            "warm cache inspected {work} slots for {slots} residents"
        );
        measured.push(work);
        frame.finish().cancel();
    }
    assert!(
        measured[1] <= 4 * measured[0] + 16,
        "quadrupling residents exceeded linear slot work: {measured:?}"
    );
}

#[test]
fn blocking_b5_antipodal_mips_have_a_defined_source_direction() {
    let context = context();
    let device = context.device();
    let authority = SubmissionAuthority::new(context.queue().clone());
    let mut parts = settings().parts();
    parts.interior = [4, 4];
    parts.border = 2;
    parts.upload_bytes_per_frame = 100_000;
    let settings = TerrainPageSettings::new(parts).unwrap();
    let base = MaterialImage::new(
        [8, 8],
        (0..64)
            .map(|i| MaterialTexel {
                color: [64, 128, 192, 255],
                normal_roughness: if i % 2 == 0 {
                    [0, 0, 0, 180]
                } else {
                    [255, 255, 255, 180]
                },
            })
            .collect(),
    )
    .unwrap();
    let expected = super::mips::build_mips(base.clone())
        .expect("antipodal tie uses the first contributing direction");
    assert_eq!(expected[1].texels()[0].normal_roughness, [54, 54, 54, 180]);
    let cache =
        ResidentPages::new(device, &authority, settings, 1, AllocationBudget(1_000_000)).unwrap();
    let writer = PageWriter::new(device, settings);
    let mut frame = authority.begin(device).unwrap();
    let pending = cache.reserve(&frame, PageKey::ROOT).unwrap();
    writer
        .encode(device, &mut frame, &cache, pending, &base)
        .unwrap();
    let reads = super::readback_tests::encode_reads(
        device,
        &mut frame,
        &cache.storage().texture,
        &expected,
        0,
    );
    frame.finish().submit().unwrap();
    assert!(cache.lookup(PageKey::ROOT, 0).is_some());
    super::readback_tests::assert_reads(device, &reads, &expected);
}
