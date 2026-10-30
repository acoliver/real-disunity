mod normals;
mod oracle;
mod reduction;
use super::*;
use harness::{Mode, sample_with};
use worldrunner_render_plan::TerrainPageSettings;

struct Fixture {
    cache: ResidentPages,
    leaves: [PageKey; 4],
    images: Vec<(PageKey, Vec<MaterialImage>)>,
}

impl Fixture {
    fn new(context: &crate::device::GpuContext) -> Self {
        Self::new_with(context, |_, uv| stripes(uv))
    }

    fn new_with(
        context: &crate::device::GpuContext,
        mut sample: impl FnMut(PageKey, [f64; 2]) -> Vec<super::super::composition::LayerSample>,
    ) -> Self {
        let device = context.device();
        let authority = SubmissionAuthority::new(context.queue().clone());
        let mut parts = settings().parts();
        parts.interior = [32, 16];
        parts.border = 4;
        parts.upload_bytes_per_frame = 100_000;
        let settings = TerrainPageSettings::new(parts).unwrap();
        let cache =
            ResidentPages::new(device, &authority, settings, 5, AllocationBudget(1_000_000))
                .unwrap();
        let leaves = [[0, 0], [1, 0], [0, 1], [1, 1]].map(|xy| PageKey::new(1, xy).unwrap());
        let writer = PageWriter::new(device, settings);
        let mut images = Vec::new();
        for key in [PageKey::ROOT].into_iter().chain(leaves) {
            let base = super::super::composition::compose_page(settings, key, |uv| sample(key, uv))
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
            images.push((key, super::super::mips::build_mips(base).unwrap()));
        }
        Self {
            cache,
            leaves,
            images,
        }
    }

    fn sample(
        &self,
        context: &crate::device::GpuContext,
        points: &[[f32; 4]],
        mode: Mode,
    ) -> Vec<u8> {
        let authority = SubmissionAuthority::new(context.queue().clone());
        sample_with(
            context.device(),
            &authority,
            &self.cache,
            &self.leaves,
            points,
            mode,
        )
    }
}

fn stripes(uv: [f64; 2]) -> Vec<super::super::composition::LayerSample> {
    let value = if (uv[0] * 32.0).floor().rem_euclid(2.0) < 1.0 {
        32
    } else {
        224
    };
    vec![super::super::composition::LayerSample {
        color: [value, value, value, 255],
        normal: [0, 0, i16::MAX],
        roughness: 180,
        weight: 1,
    }]
}

fn mode(level: u16) -> Mode {
    Mode {
        work: false,
        normals: false,
        level,
        baseline: false,
        extent: [1.0; 2],
    }
}

fn energy(bytes: &[u8]) -> u64 {
    bytes
        .chunks_exact(4)
        .map(|p| i64::from(p[0]) - 128)
        .map(|v| v.unsigned_abs().pow(2))
        .sum()
}

#[test]
fn cached_anisotropy_grazing_preserves_minor_axis_detail_on_every_backend() {
    let context = context();
    let fixture = Fixture::new(&context);
    let points: Vec<_> = (0..16_u16)
        .map(|x| [0.257_812_5 + f32::from(x) / 64.0, 0.25, 1.0 / 64.0, 0.5])
        .collect();
    let one = fixture.sample(&context, &points, mode(1));
    let sixteen = fixture.sample(&context, &points, mode(16));
    let baseline: Vec<_> = points
        .iter()
        .map(|p| [p[0], p[1], 1.0 / 64.0, 1.0 / 32.0])
        .collect();
    assert!(energy(&fixture.sample(&context, &baseline, mode(1))) > 10_000);
    eprintln!(
        "cached adapter={:?}, 1x={one:?}, 16x={sixteen:?}",
        context.adapter().get_info()
    );
    assert!(energy(&sixteen) > energy(&one) + 10_000);
}

#[test]
fn cached_anisotropy_matches_independent_oracle_and_straddles_neighbor_pages() {
    let context = context();
    let fixture = Fixture::new(&context);
    for level in [2, 4, 8, 16] {
        for gradients in [
            [0.5, 1.0 / 32.0],
            [0.375, 0.0625],
            [1.0 / 64.0, 0.5],
            [0.0, 0.0],
            [3.0 / 128.0, 1.0 / 64.0],
        ] {
            let points = [
                [0.49, 0.25, gradients[0], gradients[1]],
                [0.51, 0.75, gradients[0], gradients[1]],
            ];
            let bytes = fixture.sample(&context, &points, mode(level));
            for (point, actual) in points.iter().zip(bytes.chunks_exact(4)) {
                let (expected, keys, _) = oracle::sample(&fixture, *point, level);
                // Manual f32 filters have no UNORM weight quantization. For these
                // dyadic extents, a conservative arithmetic and packing bound is
                // 64 * f32::EPSILON * 40 * 192 + 0.5 < 1 byte.
                assert!(
                    (f64::from(actual[0]) - expected).abs() <= 1.0,
                    "point={point:?}, level={level}, GPU={actual:?}, CPU={expected}"
                );
                if level == 16 && gradients[0].to_bits() == 0.5_f32.to_bits() {
                    assert!(keys.iter().any(|k| k.coordinate()[0] == 0));
                    assert!(keys.iter().any(|k| k.coordinate()[0] == 1));
                    assert!(keys.iter().all(|k| k.depth() == 1));
                }
            }
        }
    }
}

#[test]
fn cached_anisotropy_straddling_taps_choose_their_own_resident_ancestor() {
    let context = context();
    let mut fixture = Fixture::new_with(&context, |key, uv| {
        let mut samples = stripes(uv);
        if key == PageKey::ROOT {
            samples[0].color = [16, 16, 16, 255];
        }
        samples
    });
    let point = [0.49, 0.25, 0.5, 1.0 / 32.0];
    let (stale, _, _) = oracle::sample(&fixture, point, 16);
    let stale_gpu = fixture.sample(&context, &[point], mode(16));
    assert!((f64::from(stale_gpu[0]) - stale).abs() <= 1.0);
    let missing = PageKey::new(1, [1, 0]).unwrap();
    fixture.cache.evict(missing);
    fixture.images.retain(|(key, _)| *key != missing);
    let (expected, keys, _) = oracle::sample(&fixture, point, 16);
    assert!(keys.contains(&PageKey::ROOT));
    assert!(keys.iter().any(|key| key.depth() == 1));
    let actual = fixture.sample(&context, &[point], mode(16));
    assert!((expected - stale).abs() > 1.0);
    assert!((f64::from(actual[0]) - stale).abs() > 1.0);
    assert!(actual[0].abs_diff(stale_gpu[0]) > 1);
    assert!(
        (f64::from(actual[0]) - expected).abs() <= 1.0,
        "mixed resident depths: GPU={actual:?}, CPU={expected}, pages={keys:?}"
    );
}

#[test]
fn cached_anisotropy_ratio_one_and_default_match_main_bytes() {
    let context = context();
    let fixture = Fixture::new(&context);
    for gradients in [
        [0.0; 2],
        [1.0 / 32.0, 1.0 / 16.0],
        [0.125, 0.25],
        [0.375, 0.0],
    ] {
        let points = [
            [0.49, 0.25, gradients[0], gradients[1]],
            [0.75, 0.625, gradients[0], gradients[1]],
        ];
        let one = fixture.sample(&context, &points, mode(1));
        assert_eq!(
            one,
            fixture.sample(
                &context,
                &points,
                Mode {
                    baseline: true,
                    ..mode(1)
                }
            )
        );
        if (gradients[0] * 32.0).to_bits() == (gradients[1] * 16.0).to_bits() {
            for level in [2, 4, 8, 16] {
                assert_eq!(one, fixture.sample(&context, &points, mode(level)));
            }
        }
    }
}

#[test]
fn cached_anisotropy_extreme_derivatives_produce_finite_surface() {
    let context = context();
    let fixture = Fixture::new(&context);
    for extent in [[1.0; 2], [0.001, 7.0]] {
        for gradients in [
            [f32::MAX, f32::MAX],
            [f32::MAX, 0.0],
            [7e-14, 0.0],
            [0.0; 2],
        ] {
            let points = [[
                0.49 * extent[0],
                0.25 * extent[1],
                gradients[0],
                gradients[1],
            ]];
            let actual = fixture.sample(&context, &points, Mode { extent, ..mode(16) });
            assert_eq!(
                actual[3], 255,
                "surface must be finite: extent={extent:?}, gradients={gradients:?}"
            );
            assert!(
                (32..=224).contains(&actual[0]),
                "extent={extent:?}, gradients={gradients:?}, bytes={actual:?}"
            );
        }
    }
}
