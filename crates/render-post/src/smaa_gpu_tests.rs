//! SMAA 1x on synthetic images through the HDR post chain (display-referred, after
//! tone mapping). Scene radiance 3.0 tone maps to 0.75 and 0.0 stays black, so
//! every edge is far above all preset thresholds.
use crate::chain::post_effect_tests::{chain, output, paint, pixels, step};
use crate::{HdrPostSettings, PostResolve, SmaaEdgeDetection, SmaaPreset, SmaaSettings};

pub(super) struct Rig {
    pub(super) device: wgpu::Device,
    pub(super) queue: wgpu::Queue,
}
impl Rig {
    pub(super) fn new() -> Self {
        let (device, queue) = crate::gpu_tests::gpu();
        Self { device, queue }
    }
    pub(super) fn render(&self, resolve: PostResolve, size: [u32; 2], body: &str) -> Vec<u8> {
        let settings = HdrPostSettings {
            resolve,
            ..HdrPostSettings::default()
        };
        let chain = chain(&self.device, &settings, size);
        let out = step(&self.device, &self.queue, &chain, 0, body, false);
        pixels(&self.device, &self.queue, &out)
    }
    /// The same scene without and with SMAA.
    fn pair(&self, smaa: SmaaSettings, size: [u32; 2], body: &str) -> (Vec<u8>, Vec<u8>) {
        (
            self.render(PostResolve::None, size, body),
            self.render(PostResolve::Smaa(smaa), size, body),
        )
    }
}

fn presets() -> impl Iterator<Item = SmaaSettings> {
    SmaaPreset::ALL
        .into_iter()
        .map(SmaaSettings::preset)
        .chain([SmaaSettings {
            edge_detection: SmaaEdgeDetection::Color,
            ..SmaaSettings::HIGH
        }])
}

/// Pixels whose RGBA bytes differ, as (x, y).
pub(super) fn changed(plain: &[u8], smaa: &[u8], width: u32) -> Vec<(i32, i32)> {
    plain
        .chunks_exact(4)
        .zip(smaa.chunks_exact(4))
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .map(|(i, _)| {
            let i = i32::try_from(i).unwrap();
            let w = i32::try_from(width).unwrap();
            (i % w, i / w)
        })
        .collect()
}

pub(super) fn red(image: &[u8], width: u32, x: u32, y: u32) -> u8 {
    image[usize::try_from((y * width + x) * 4).unwrap()]
}

/// Every channel of every pixel lies between the darkest and brightest plain values.
pub(super) fn assert_within_plain_range(plain: &[u8], smaa: &[u8]) {
    for channel in 0..4 {
        let values = plain.iter().skip(channel).step_by(4);
        let (lo, hi) = (*values.clone().min().unwrap(), *values.max().unwrap());
        assert!(
            smaa.iter()
                .skip(channel)
                .step_by(4)
                .all(|v| (lo..=hi).contains(v)),
            "channel {channel} left {lo}..={hi}"
        );
    }
}

const STEPPED: &str =
    "let edge=select(8.0,9.0,p.y>8.0);let v=select(0.0,3.0,p.x>edge);return vec4<f32>(v,v,v,1.0);";
const DIAGONAL: &str = "let v=select(0.0,3.0,p.x>p.y);return vec4<f32>(v,v,v,1.0);";

#[test]
fn constant_images_are_unchanged_by_every_preset() {
    let rig = Rig::new();
    for smaa in presets() {
        let (plain, filtered) = rig.pair(smaa, [16, 16], "return vec4<f32>(0.5,1.25,2.0,0.75);");
        crate::fxaa_tolerance_tests::assert_paths_agree(&plain, &filtered, &format!("{smaa:?}"));
    }
}

#[test]
fn straight_axis_aligned_edges_stay_sharp() {
    let rig = Rig::new();
    let vertical = "let v=select(0.0,3.0,p.x>8.0);return vec4<f32>(v,v,v,1.0);";
    let horizontal = "let v=select(0.0,3.0,p.y>8.0);return vec4<f32>(v,v,v,1.0);";
    for smaa in presets() {
        for body in [vertical, horizontal] {
            let (plain, filtered) = rig.pair(smaa, [16, 16], body);
            crate::fxaa_tolerance_tests::assert_paths_agree(
                &plain,
                &filtered,
                &format!("{smaa:?}"),
            );
        }
    }
}

/// The display-referred intermediate is half precision. A smooth ramp has no edges, so
/// SMAA must reproduce the plain output to within one 8-bit code of that rounding.
#[test]
fn half_precision_input_keeps_smooth_ramps_within_one_code() {
    let rig = Rig::new();
    let ramp = "let v=3.0*p.x/64.0;return vec4<f32>(v,v*0.5,v*0.25,1.0);";
    for smaa in presets() {
        let (plain, filtered) = rig.pair(smaa, [64, 8], ramp);
        let worst = plain
            .iter()
            .zip(&filtered)
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap();
        assert!(worst <= 1, "{smaa:?} moved a ramp value by {worst} codes");
    }
}

#[test]
fn a_one_pixel_step_blends_only_next_to_the_edge() {
    let rig = Rig::new();
    let size = [24, 24];
    for smaa in presets() {
        let (plain, filtered) = rig.pair(smaa, size, STEPPED);
        let changes = changed(&plain, &filtered, size[0]);
        assert!(!changes.is_empty(), "{smaa:?} did not blend the step");
        assert!(
            changes.iter().all(|(x, _)| (7..=9).contains(x)),
            "{smaa:?} touched {changes:?}"
        );
        assert_within_plain_range(&plain, &filtered);
    }
}

#[test]
fn a_diagonal_edge_blends_only_along_the_diagonal() {
    let rig = Rig::new();
    let size = [24, 24];
    for smaa in presets() {
        let (plain, filtered) = rig.pair(smaa, size, DIAGONAL);
        let changes = changed(&plain, &filtered, size[0]);
        assert!(changes.len() >= 10, "{smaa:?} changed {}", changes.len());
        assert!(
            changes.iter().all(|(x, y)| (x - y).abs() <= 2),
            "{smaa:?} touched {changes:?}"
        );
        assert_within_plain_range(&plain, &filtered);
    }
}

#[test]
fn a_one_pixel_line_is_kept() {
    let rig = Rig::new();
    let size = [24, 24];
    let line = "let on=p.y>8.0 && p.y<9.0 && p.x>4.0 && p.x<20.0;let v=select(0.0,3.0,on);return vec4<f32>(v,v,v,1.0);";
    for smaa in presets() {
        let (plain, filtered) = rig.pair(smaa, size, line);
        let peak = u32::from(red(&plain, size[0], 10, 8));
        assert!(peak > 150);
        // SMAA softens a thin line but does not erase it: its ends keep at least
        // 70% of the peak and the middle at least 80%, and nothing leaks outside.
        for x in 4..=19 {
            let kept = u32::from(red(&filtered, size[0], x, 8));
            let floor = if (8..=15).contains(&x) { 8 } else { 7 };
            assert!(kept * 10 >= peak * floor, "{smaa:?} eroded x={x}: {kept}");
        }
        assert!(
            changed(&plain, &filtered, size[0])
                .iter()
                .all(|&(_, y)| y == 8),
            "{smaa:?} bled outside the line row"
        );
        assert_within_plain_range(&plain, &filtered);
    }
}

#[test]
fn odd_and_tiny_extents_render() {
    let rig = Rig::new();
    for size in [[1, 1], [2, 1], [1, 5], [3, 3], [17, 9], [31, 13]] {
        for smaa in [SmaaSettings::LOW, SmaaSettings::ULTRA] {
            let (plain, filtered) = rig.pair(smaa, size, DIAGONAL);
            assert_eq!(plain.len(), filtered.len());
            assert_within_plain_range(&plain, &filtered);
            let (flat, filtered_flat) = rig.pair(smaa, size, "return vec4<f32>(1.0,2.0,0.5,1.0);");
            crate::fxaa_tolerance_tests::assert_paths_agree(
                &flat,
                &filtered_flat,
                &format!("{size:?}"),
            );
        }
    }
}

#[test]
fn alpha_is_blended_within_its_input_range_and_flat_alpha_is_exact() {
    let rig = Rig::new();
    let size = [24, 24];
    let (plain, filtered) = rig.pair(SmaaSettings::ULTRA, size, DIAGONAL);
    assert!(
        filtered.chunks_exact(4).all(|p| p[3] == 255),
        "opaque input stays opaque"
    );
    assert_ne!(plain, filtered);
    let translucent =
        "let v=select(0.0,3.0,p.x>p.y);return vec4<f32>(v,v,v,select(0.25,1.0,p.x>p.y));";
    let (plain, filtered) = rig.pair(SmaaSettings::ULTRA, size, translucent);
    assert_within_plain_range(&plain, &filtered);
    assert!(
        plain
            .chunks_exact(4)
            .zip(filtered.chunks_exact(4))
            .any(|(a, b)| a[3] != b[3]),
        "alpha follows the color blend"
    );
}

#[test]
fn resize_rebuilds_the_passes_for_the_new_extent() {
    let rig = Rig::new();
    let mut chain = chain(
        &rig.device,
        &HdrPostSettings {
            resolve: PostResolve::smaa(SmaaPreset::High),
            ..HdrPostSettings::default()
        },
        [24, 24],
    );
    for size in [[40, 18], [7, 9], [24, 24]] {
        chain.resize(&rig.device, size).unwrap();
        let filtered = pixels(
            &rig.device,
            &rig.queue,
            &step(&rig.device, &rig.queue, &chain, 0, DIAGONAL, false),
        );
        let plain = rig.render(PostResolve::None, size, DIAGONAL);
        assert_eq!(filtered.len(), plain.len());
        assert_within_plain_range(&plain, &filtered);
        if size[0] >= 17 {
            assert!(!changed(&plain, &filtered, size[0]).is_empty(), "{size:?}");
        }
    }
}

/// Frames of the timing probe. Diagonal stripes put edges in a few percent of the pixels; the
/// disc has one closed edge, so list-driven passes find little to do.
const PROBE_SCENES: [(&str, &str); 2] = [
    (
        "stripes",
        "let v=0.25+3.0*step(0.5,fract((p.x+p.y*0.5)/37.0));return vec4<f32>(v,v*0.5,v*0.25,1.0);",
    ),
    (
        "disc",
        "let v=select(0.25,3.0,length(p.xy-vec2<f32>(960.0,540.0))<300.0);return vec4<f32>(v,v*0.5,v*0.25,1.0);",
    ),
];

/// Mean GPU time per frame of 24 frames in one submission, without timestamp queries. Copies
/// have no timing slot, so this is how a copy shows up in the probe.
fn untimed_frame_ms(rig: &Rig, settings: &HdrPostSettings, scene: &str) -> f64 {
    const FRAMES: u32 = 24;
    let size = [1920, 1080];
    let chain = chain(&rig.device, settings, size);
    let target = output(&rig.device, size);
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
    let authority = crate::test_authority(&rig.queue);
    let mut runs: Vec<f64> = (0..5)
        .map(|_| {
            let mut encoder = authority.begin(&rig.device).unwrap();
            for _ in 0..FRAMES {
                paint(&rig.device, &mut encoder, chain.targets().resolved(), scene);
                chain.encode(&mut encoder, &view);
            }
            let finished = encoder.finish();
            let started = std::time::Instant::now();
            let submission = finished.submit().unwrap();
            rig.device
                .poll(wgpu::PollType::Wait {
                    submission_index: Some(submission),
                    timeout: Some(std::time::Duration::from_mins(2)),
                })
                .unwrap();
            started.elapsed().as_secs_f64() * 1e3 / f64::from(FRAMES)
        })
        .collect();
    runs.sort_by(f64::total_cmp);
    runs[runs.len() / 2]
}

/// Per-pass medians for one resolve and optional sharpening on a synthetic 1920x1080 frame.
fn timed_medians(
    rig: &Rig,
    resolve: PostResolve,
    sharpen: Option<crate::CasSettings>,
    scene: &str,
) -> Vec<(String, f64)> {
    let size = [1920, 1080];
    let settings = HdrPostSettings {
        resolve,
        sharpen,
        ..HdrPostSettings::default()
    };
    eprintln!(
        "  untimed chain: {:.3} ms per frame (paint, chain and untimed copies)",
        untimed_frame_ms(rig, &settings, scene)
    );
    let chain = chain(&rig.device, &settings, size);
    chain
        .configure_timing(&rig.device, &rig.queue, crate::TimingMode::Required)
        .unwrap();
    let target = output(&rig.device, size);
    let authority = crate::test_authority(&rig.queue);
    let frames: Vec<_> = (0..7)
        .map(|frame| {
            let mut encoder = authority.begin(&rig.device).unwrap();
            chain.begin_timing(frame).unwrap();
            paint(&rig.device, &mut encoder, chain.targets().resolved(), scene);
            chain.encode(
                &mut encoder,
                &target.create_view(&wgpu::TextureViewDescriptor::default()),
            );
            chain.finish_timing(&mut encoder).unwrap();
            let submission = encoder.finish().submit().unwrap();
            rig.device
                .poll(wgpu::PollType::Wait {
                    submission_index: Some(submission),
                    timeout: Some(std::time::Duration::from_mins(2)),
                })
                .unwrap();
            chain.take_timings().unwrap().pop().expect("timing result")
        })
        .skip(2)
        .collect();
    (0..frames[0].passes.len())
        .map(|pass| {
            let mut durations: Vec<_> = frames
                .iter()
                .map(|frame| frame.passes[pass].duration_ns.expect("valid query pair"))
                .collect();
            durations.sort_by(f64::total_cmp);
            (
                format!("{:?}", frames[0].passes[pass].pass_id),
                durations[durations.len() / 2],
            )
        })
        .collect()
}

#[test]
#[ignore = "explicit 1920x1080 GPU performance probe"]
fn smaa_and_fxaa_stage_timestamps() {
    let (device, queue) =
        crate::gpu_tests::gpu_with_optional_features(wgpu::Features::TIMESTAMP_QUERY);
    let rig = Rig { device, queue };
    if !rig
        .device
        .features()
        .contains(wgpu::Features::TIMESTAMP_QUERY)
    {
        eprintln!("adapter has no timestamp queries; nothing to measure");
        return;
    }
    let cas = Some(crate::CasSettings::new(0.5));
    let cmaa2 = PostResolve::cmaa2;
    for (name, resolve, sharpen) in [
        ("none", PostResolve::None, None),
        ("fxaa", PostResolve::fxaa(1.0), None),
        ("smaa low", PostResolve::smaa(SmaaPreset::Low), None),
        ("smaa medium", PostResolve::smaa(SmaaPreset::Medium), None),
        ("smaa high", PostResolve::smaa(SmaaPreset::High), None),
        ("smaa ultra", PostResolve::smaa(SmaaPreset::Ultra), None),
        ("cmaa2 low", cmaa2(crate::Cmaa2Preset::Low), None),
        ("cmaa2 medium", cmaa2(crate::Cmaa2Preset::Medium), None),
        ("cmaa2 high", cmaa2(crate::Cmaa2Preset::High), None),
        ("cmaa2 ultra", cmaa2(crate::Cmaa2Preset::Ultra), None),
        ("cas", PostResolve::None, cas),
        ("fxaa + cas", PostResolve::fxaa(1.0), cas),
        (
            "smaa medium + cas",
            PostResolve::smaa(SmaaPreset::Medium),
            cas,
        ),
        ("cmaa2 high + cas", cmaa2(crate::Cmaa2Preset::High), cas),
        ("taa", PostResolve::taa(0.9), None),
    ] {
        for (scene_name, scene) in PROBE_SCENES {
            let passes = timed_medians(&rig, resolve, sharpen, scene);
            let total: f64 = passes.iter().map(|(_, ns)| ns).sum();
            eprintln!(
                "{name} [{scene_name}]: total {:.3} ms (median of 5)",
                total / 1e6
            );
            for (pass, ns) in passes {
                eprintln!("  {pass}: {:.3} ms", ns / 1e6);
            }
        }
    }
}
