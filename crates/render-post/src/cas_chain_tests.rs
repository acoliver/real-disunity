//! CAS inside the HDR post chain: scheduling after each resolve, the unchanged path when it
//! is off, resize and the 8-bit sRGB result of a tone-mapped frame.
use super::{chain, pixels, step};
use crate::{
    CasSettings, ColorGrading, FrameConfig, HdrPostSettings, PostChain, PostResolve, Resources, SmaaPreset,
    StageId, ToneMap, ToneMapOutput, UpscaleMode, UpscaleSettings,
};

/// Radiance ramps from 0.5 to 2.5 over six pixels. Reinhard maps that to display 0.33..0.71,
/// mid-range contrast the limiter lets CAS sharpen.
const SOFT_EDGE: &str =
    "let v=0.5+2.0*clamp((p.x-8.0)/6.0,0.0,1.0);return vec4<f32>(v,v*0.8,v*0.6,0.75);";
const FLAT: &str = "return vec4<f32>(0.5,1.25,2.0,0.75);";

struct Rig {
    device: wgpu::Device,
    queue: wgpu::Queue,
}
impl Rig {
    fn new() -> Self {
        let (device, queue) = crate::gpu_tests::gpu();
        Self { device, queue }
    }

    fn render(&self, settings: &HdrPostSettings, size: [u32; 2], body: &str) -> Vec<u8> {
        let chain = chain(&self.device, settings, size);
        let out = step(&self.device, &self.queue, &chain, 0, body, false);
        pixels(&self.device, &self.queue, &out)
    }
}

fn with(resolve: PostResolve, sharpness: Option<f32>) -> HdrPostSettings {
    HdrPostSettings {
        resolve,
        sharpen: sharpness.map(CasSettings::new),
        ..HdrPostSettings::default()
    }
}

fn resolves() -> [PostResolve; 3] {
    [
        PostResolve::None,
        PostResolve::fxaa(1.0),
        PostResolve::smaa(SmaaPreset::Medium),
    ]
}

fn stage_names(device: &wgpu::Device, settings: &HdrPostSettings) -> Vec<&'static str> {
    chain(device, settings, [8, 8]).configured[0]
        .stages
        .iter()
        .map(|stage| stage.id.name)
        .collect()
}

#[test]
fn off_schedules_exactly_the_stages_it_did_before_the_field_existed() {
    let rig = Rig::new();
    let expected: [&[&str]; 3] = [
        &["tone"],
        &["tone", "fxaa"],
        &["tone", "smaa_edges", "smaa_weights", "smaa_blend"],
    ];
    for (resolve, expected) in resolves().into_iter().zip(expected) {
        assert_eq!(stage_names(&rig.device, &with(resolve, None)), expected);
        // The default descriptor value and an explicit `None` are the same settings.
        assert_eq!(
            with(resolve, None),
            HdrPostSettings {
                resolve,
                ..HdrPostSettings::default()
            }
        );
        let plain = rig.render(&with(resolve, None), [24, 8], SOFT_EDGE);
        let again = rig.render(
            &HdrPostSettings {
                resolve,
                ..HdrPostSettings::default()
            },
            [24, 8],
            SOFT_EDGE,
        );
        assert_eq!(plain, again, "{resolve:?}");
        assert_ne!(
            plain,
            rig.render(&with(resolve, Some(0.0)), [24, 8], SOFT_EDGE),
            "{resolve:?} with sharpness 0 still sharpens"
        );
    }
    assert!(!with(PostResolve::None, None).enabled());
    assert!(with(PostResolve::None, Some(0.0)).enabled());
}

#[test]
fn one_cas_pass_follows_the_resolve_and_has_its_own_stage_identity() {
    let rig = Rig::new();
    let expected: [&[&str]; 3] = [
        &["tone", "cas"],
        &["tone", "fxaa", "cas"],
        &["tone", "smaa_edges", "smaa_weights", "smaa_blend", "cas"],
    ];
    for (resolve, expected) in resolves().into_iter().zip(expected) {
        assert_eq!(
            stage_names(&rig.device, &with(resolve, Some(0.5))),
            expected,
            "{resolve:?}"
        );
    }
}

#[test]
fn cas_runs_before_the_upscale_stage() {
    let rig = Rig::new();
    let settings = HdrPostSettings {
        sharpen: Some(CasSettings::new(0.5)),
        resolve: PostResolve::fxaa(1.0),
        upscale: Some(UpscaleSettings {
            mode: UpscaleMode::Easu,
            sharpness: Some(0.0),
        }),
        ..HdrPostSettings::default()
    };
    let upscaled = PostChain::new(
        &rig.device,
        FrameConfig::new([32, 16], wgpu::TextureFormat::Rgba16Float, 1).with_render_extent([16, 8]),
        Resources::default(),
        ToneMapOutput::new(wgpu::TextureFormat::Rgba8UnormSrgb, 1.0, ToneMap::Reinhard)
            .unwrap()
            .with_effects(&rig.device, settings)
            .unwrap(),
    )
    .unwrap();
    let names: Vec<_> = upscaled.configured[0]
        .stages
        .iter()
        .map(|stage| stage.id)
        .collect();
    assert_eq!(
        names,
        [
            StageId::new("tone", 0),
            StageId::new("fxaa", 0),
            StageId::new("cas", 0),
            StageId::new("easu", 0),
            StageId::new("rcas", 0),
        ]
    );
}

fn column(image: &[u8], width: u32, x: u32, channel: usize) -> u8 {
    image[usize::try_from(x * 4).unwrap() + channel + usize::try_from(width * 4).unwrap() * 3]
}

#[test]
fn a_soft_edge_is_sharpened_after_every_resolve_and_flat_areas_are_untouched() {
    let rig = Rig::new();
    let size = [24, 8];
    for resolve in resolves() {
        let plain = rig.render(&with(resolve, None), size, SOFT_EDGE);
        let soft = rig.render(&with(resolve, Some(0.0)), size, SOFT_EDGE);
        let hard = rig.render(&with(resolve, Some(1.0)), size, SOFT_EDGE);
        let edge = 7..=15;
        let moved = |a: &[u8], b: &[u8]| {
            edge.clone()
                .map(|x| column(a, size[0], x, 1).abs_diff(column(b, size[0], x, 1)))
                .max()
                .unwrap()
        };
        assert!(moved(&plain, &soft) >= 1, "{resolve:?}");
        assert!(moved(&plain, &hard) > moved(&plain, &soft), "{resolve:?}");
        for x in (0..5).chain(18..24) {
            for channel in 0..4 {
                assert_eq!(
                    column(&plain, size[0], x, channel),
                    column(&hard, size[0], x, channel),
                    "{resolve:?} x={x}"
                );
            }
        }
    }
}

#[test]
fn constant_frames_alpha_and_odd_extents_survive_every_resolve_and_sharpness() {
    let rig = Rig::new();
    for resolve in resolves() {
        for size in [[1, 1], [2, 1], [1, 5], [3, 3], [17, 9], [31, 13]] {
            let plain = rig.render(&with(resolve, None), size, FLAT);
            for sharpness in [0.0, 0.5, 1.0] {
                let sharp = rig.render(&with(resolve, Some(sharpness)), size, FLAT);
                assert_eq!(plain, sharp, "{resolve:?} {size:?} sharpness {sharpness}");
            }
            let edge = rig.render(&with(resolve, Some(1.0)), size, SOFT_EDGE);
            let unsharp = rig.render(&with(resolve, None), size, SOFT_EDGE);
            assert_eq!(edge.len(), unsharp.len());
            assert!(
                edge.chunks_exact(4)
                    .zip(unsharp.chunks_exact(4))
                    .all(|(a, b)| a[3] == b[3]),
                "{resolve:?} {size:?}: alpha is not filtered"
            );
        }
    }
}

#[test]
fn resize_rebuilds_the_pass_for_the_new_extent() {
    let rig = Rig::new();
    for resolve in resolves() {
        let settings = with(resolve, Some(1.0));
        let mut chain = chain(&rig.device, &settings, [24, 8]);
        for size in [[40, 8], [7, 9], [24, 8]] {
            chain.resize(&rig.device, size).unwrap();
            let resized = pixels(
                &rig.device,
                &rig.queue,
                &step(&rig.device, &rig.queue, &chain, 0, SOFT_EDGE, false),
            );
            assert_eq!(resized, rig.render(&settings, size, SOFT_EDGE), "{size:?}");
            if size[0] >= 24 {
                let plain = rig.render(&with(resolve, None), size, SOFT_EDGE);
                assert_ne!(resized, plain, "{resolve:?} {size:?} was not sharpened");
            }
        }
    }
}

fn post_bytes(config: FrameConfig, settings: HdrPostSettings) -> u64 {
    crate::allocation::post_allocations(config, settings)
        .validate(&wgpu::Limits::default())
        .unwrap()
}

/// Every intermediate sharpening adds is `Rgba16Float` (8 bytes a texel) at the render extent,
/// one per frame slot, so the configuration budget sees it before anything is allocated.
#[test]
fn the_allocation_budget_counts_the_intermediates_sharpening_adds() {
    let texture = 64 * 32 * 8 * 3;
    let sharpen = Some(CasSettings::new(0.5));
    let same = FrameConfig::new([64, 32], wgpu::TextureFormat::Rgba16Float, 1);
    let reduced = same.with_render_extent([32, 16]);
    let upscale = Some(UpscaleSettings {
        mode: UpscaleMode::Bilinear,
        sharpness: None,
    });
    let cases = [
        // Tone mapping writes a texture that CAS reads.
        (same, PostResolve::None, None, texture),
        // The resolve writes a texture that CAS reads.
        (same, PostResolve::smaa(SmaaPreset::Low), None, texture),
        // CAS writes a texture that the upscale reads.
        (reduced, PostResolve::None, upscale, 32 * 16 * 8 * 3),
    ];
    for (config, resolve, upscale, expected) in cases {
        // A grading change enables the post path without allocating textures, so the
        // difference is only what sharpening adds.
        let off = HdrPostSettings {
            grading: ColorGrading {
                linear_scale: 1.5,
                ..ColorGrading::default()
            },
            resolve,
            upscale,
            ..HdrPostSettings::default()
        };
        let on = HdrPostSettings { sharpen, ..off };
        assert_eq!(
            post_bytes(config, on) - post_bytes(config, off),
            expected,
            "{resolve:?}"
        );
    }
}
