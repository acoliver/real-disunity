use super::*;
use crate::terrain::pages::{
    residency_tests::{context, settings},
    sampling_tests::harness,
};
use worldrunner_render_gpu::{configuration::AllocationBudget, submission::SubmissionAuthority};
use worldrunner_render_plan::*;

#[test]
fn live_layered_and_cached_anisotropic_footprints_agree() {
    let context = context();
    for level in [1, 2, 4, 8, 16] {
        let source = fixture(level);
        assert_eq!(source.anisotropy.level(), level);
        let authority = SubmissionAuthority::new(context.queue().clone());
        let mut parts = settings().parts();
        parts.interior = [32, 16];
        parts.border = 4;
        parts.upload_bytes_per_frame = 100_000;
        let settings = TerrainPageSettings::new(parts).unwrap();
        let cache = ResidentPages::new_layered(
            context.device(),
            &authority,
            settings,
            1,
            AllocationBudget(1_000_000),
            &source,
        )
        .unwrap();
        let mut frame = authority.begin(context.device()).unwrap();
        let pending = cache.reserve(&frame, PageKey::ROOT).unwrap();
        source
            .encode_page(context.device(), &mut frame, &cache, pending)
            .unwrap();
        let submission = frame.finish().submit().unwrap();
        context
            .device()
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: None,
            })
            .unwrap();
        compare(&context, &source, &authority, &cache, level);
    }
}

fn compare(
    context: &crate::device::GpuContext,
    source: &LayeredGpuSource,
    authority: &SubmissionAuthority,
    cache: &ResidentPages,
    level: u16,
) {
    let points = [[0.265_625, 0.25], [0.296_875, 0.5], [0.328_125, 0.75]];
    for footprint in [[1.0 / 32.0, 1.0], [0.125, 0.125], [0.0625, 0.25]] {
        let live = super::source_mip_tests::execute_footprint(context, source, &points, footprint);
        let points = points.map(|p| [p[0], p[1], footprint[0], footprint[1]]);
        let cached = harness::sample_with(
            context.device(),
            authority,
            cache,
            &[PageKey::ROOT],
            &points,
            harness::Mode {
                level,
                work: false,
                normals: false,
                baseline: false,
                extent: [1.0; 2],
            },
        );
        for (a, b) in live.chunks_exact(16).zip(cached.chunks_exact(4)) {
            // Live hardware filtering rounds two spatial and one mip weight to
            // 8 bits. Across the 192-value fixture range, 3*192/256 + 0.5 < 3.
            assert!(
                a[..3].iter().zip(&b[..3]).all(|(x, y)| x.abs_diff(*y) <= 3),
                "level={level}, footprint={footprint:?}, live={a:?}, cached={b:?}"
            );
        }
    }
}

fn fixture(level: u16) -> LayeredGpuSource {
    let (terrain, resources) = super::source_mip_tests::fixture(0, 1000, [1; 3]);
    let mut appearance = terrain.surfaces()[0].appearance();
    appearance.layered.as_mut().unwrap().offset_millimeters = [0; 2];
    let terrain = SceneTerrain::new(SceneTerrainParts {
        columns: 2,
        rows: 2,
        origin_east_millimeters: 0,
        origin_north_millimeters: 0,
        sample_spacing_millimeters: 1000,
        heights_millimeters: vec![0; 4],
        surface_indices: vec![0; 4],
        surfaces: vec![SceneTerrainSurface::new(
            MaterialHandle::from_raw(0),
            appearance,
        )],
        lod_policy: TerrainLodPolicy::FullResolution,
    })
    .unwrap()
    .with_layered_source(TerrainLayeredSource {
        anisotropy: TextureAnisotropy::new(level).unwrap(),
        ..terrain.layered_source().unwrap().clone()
    })
    .unwrap();
    let mut textures = resources.textures().to_vec();
    let mut bytes = Vec::new();
    for mip in 0..6 {
        let width = (32_u32 >> mip).max(1);
        let height = (16_u32 >> mip).max(1);
        for _ in 0..height {
            for x in 0..width {
                let v = if mip > 0 {
                    128
                } else if x % 2 == 0 {
                    32
                } else {
                    224
                };
                bytes.extend([v, v, v, 255]);
            }
        }
    }
    textures[0] = SceneTexture::Texels {
        width: 32,
        height: 16,
        mips: 6,
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        bytes: bytes.into(),
    };
    LayeredGpuSource::new(&terrain, &SceneResources::new(textures, vec![]).unwrap()).unwrap()
}
