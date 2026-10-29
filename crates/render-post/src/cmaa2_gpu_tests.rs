//! CMAA2 on synthetic images through the HDR post chain (display-referred, after tone
//! mapping). Scene radiance 3.0 tone maps to 0.75 and 0.0 stays black, so every edge is far
//! above all preset thresholds.
use super::smaa_gpu_tests::{Rig, assert_within_plain_range, changed, red};
use crate::chain::post_effect_tests::{chain, pixels, step};
use crate::{Cmaa2Preset, Cmaa2Settings, HdrPostSettings, PostResolve};

fn render_pair(rig: &Rig, cmaa2: Cmaa2Settings, size: [u32; 2], body: &str) -> (Vec<u8>, Vec<u8>) {
    (
        rig.render(PostResolve::None, size, body),
        rig.render(PostResolve::Cmaa2(cmaa2), size, body),
    )
}

/// Every preset, and each with the extra sharpness option.
fn presets() -> impl Iterator<Item = Cmaa2Settings> {
    Cmaa2Preset::ALL
        .into_iter()
        .map(Cmaa2Settings::preset)
        .flat_map(|cmaa2| [cmaa2, cmaa2.with_extra_sharpness()])
}

const STEPPED: &str =
    "let edge=select(8.0,9.0,p.y>8.0);let v=select(0.0,3.0,p.x>edge);return vec4<f32>(v,v,v,1.0);";
const DIAGONAL: &str = "let v=select(0.0,3.0,p.x>p.y);return vec4<f32>(v,v,v,1.0);";

#[test]
fn constant_images_are_unchanged_by_every_preset() {
    let rig = Rig::new();
    for cmaa2 in presets() {
        let (plain, filtered) = render_pair(
            &rig,
            cmaa2,
            [16, 16],
            "return vec4<f32>(0.5,1.25,2.0,0.75);",
        );
        assert_eq!(plain, filtered, "{cmaa2:?}");
    }
}

#[test]
fn straight_axis_aligned_edges_stay_sharp() {
    let rig = Rig::new();
    let vertical = "let v=select(0.0,3.0,p.x>8.0);return vec4<f32>(v,v,v,1.0);";
    let horizontal = "let v=select(0.0,3.0,p.y>8.0);return vec4<f32>(v,v,v,1.0);";
    for cmaa2 in presets() {
        for body in [vertical, horizontal] {
            let (plain, filtered) = render_pair(&rig, cmaa2, [16, 16], body);
            assert_eq!(plain, filtered, "{cmaa2:?}");
        }
    }
}

/// The display-referred intermediate is half precision. A smooth ramp has no edges, so
/// CMAA2 must reproduce the plain output to within one 8-bit code of that rounding.
#[test]
fn half_precision_input_keeps_smooth_ramps_within_one_code() {
    let rig = Rig::new();
    let ramp = "let v=3.0*p.x/64.0;return vec4<f32>(v,v*0.5,v*0.25,1.0);";
    for cmaa2 in presets() {
        let (plain, filtered) = render_pair(&rig, cmaa2, [64, 8], ramp);
        let worst = plain
            .iter()
            .zip(&filtered)
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap();
        assert!(worst <= 1, "{cmaa2:?} moved a ramp value by {worst} codes");
    }
}

#[test]
fn a_one_pixel_step_blends_only_next_to_the_edge() {
    let rig = Rig::new();
    let size = [24, 24];
    for cmaa2 in presets() {
        let (plain, filtered) = render_pair(&rig, cmaa2, size, STEPPED);
        let changes = changed(&plain, &filtered, size[0]);
        assert!(!changes.is_empty(), "{cmaa2:?} did not blend the step");
        assert!(
            changes.iter().all(|(x, _)| (7..=9).contains(x)),
            "{cmaa2:?} touched {changes:?}"
        );
        assert_within_plain_range(&plain, &filtered);
    }
}

#[test]
fn a_diagonal_edge_blends_only_along_the_diagonal() {
    let rig = Rig::new();
    let size = [24, 24];
    for cmaa2 in presets() {
        let (plain, filtered) = render_pair(&rig, cmaa2, size, DIAGONAL);
        let changes = changed(&plain, &filtered, size[0]);
        assert!(changes.len() >= 10, "{cmaa2:?} changed {}", changes.len());
        assert!(
            changes.iter().all(|(x, y)| (x - y).abs() <= 2),
            "{cmaa2:?} touched {changes:?}"
        );
        assert_within_plain_range(&plain, &filtered);
    }
}

#[test]
fn a_one_pixel_line_is_kept() {
    let rig = Rig::new();
    let size = [24, 24];
    let line = "let on=p.y>8.0 && p.y<9.0 && p.x>4.0 && p.x<20.0;let v=select(0.0,3.0,on);return vec4<f32>(v,v,v,1.0);";
    for cmaa2 in presets() {
        let (plain, filtered) = render_pair(&rig, cmaa2, size, line);
        let peak = u32::from(red(&plain, size[0], 10, 8));
        assert!(peak > 150);
        for x in 4..=19 {
            let kept = u32::from(red(&filtered, size[0], x, 8));
            assert!(kept * 10 >= peak * 8, "{cmaa2:?} eroded x={x}: {kept}");
        }
        assert_within_plain_range(&plain, &filtered);
    }
}

/// Three 5x7 glyphs, one pixel strokes on black, each in an 8x9 cell.
const TEXT: &str = "var rows = array<u32,14>(17u,17u,17u,31u,17u,17u,17u, 31u,16u,16u,30u,16u,16u,31u);\
    let cx=i32(p.x)-4;let cy=i32(p.y)-4;var on=false;\
    if cx>=0 && cx<24 && cy>=0 && cy<7 && (cx%8)<5 {\
      let cell=cx/8;let col=cx%8;let row=cy+select(0,7,cell==1);\
      on=((rows[row]>>u32(4-col))&1u)==1u;}\
    let v=select(0.0,3.0,on);return vec4<f32>(v,v,v,1.0);";

#[test]
fn text_like_shapes_are_largely_preserved() {
    let rig = Rig::new();
    let size = [32, 16];
    for cmaa2 in presets() {
        let (plain, filtered) = render_pair(&rig, cmaa2, size, TEXT);
        let stroke: Vec<usize> = (0..plain.len() / 4)
            .filter(|&i| plain[i * 4] > 150)
            .collect();
        assert!(stroke.len() > 40, "fixture has strokes");
        let before: u32 = stroke.iter().map(|&i| u32::from(plain[i * 4])).sum();
        let after: u32 = stroke.iter().map(|&i| u32::from(filtered[i * 4])).sum();
        assert!(
            after * 100 >= before * 85,
            "{cmaa2:?} kept {after} of {before} stroke energy"
        );
        let touched = changed(&plain, &filtered, size[0]).len();
        assert!(
            touched * 100 <= stroke.len() * 60,
            "{cmaa2:?} changed {touched} pixels around {} strokes",
            stroke.len()
        );
        assert_within_plain_range(&plain, &filtered);
    }
}

/// Widths and heights around the 28 pixel stride of one edge workgroup and the 2x2 quad grid.
#[test]
fn odd_and_tiny_extents_render() {
    let rig = Rig::new();
    let sizes = [
        [1, 1],
        [2, 1],
        [1, 5],
        [3, 3],
        [17, 9],
        [27, 5],
        [28, 28],
        [29, 31],
        [31, 13],
        [57, 3],
        [58, 59],
    ];
    for size in sizes {
        for cmaa2 in [
            Cmaa2Settings::LOW,
            Cmaa2Settings::ULTRA.with_extra_sharpness(),
        ] {
            let (plain, filtered) = render_pair(&rig, cmaa2, size, DIAGONAL);
            assert_eq!(plain.len(), filtered.len());
            assert_within_plain_range(&plain, &filtered);
            let (flat, filtered_flat) =
                render_pair(&rig, cmaa2, size, "return vec4<f32>(1.0,2.0,0.5,1.0);");
            assert_eq!(flat, filtered_flat, "{size:?}");
        }
    }
}

/// A diagonal edge that crosses several workgroups is blended on both sides of each seam.
#[test]
fn edges_spanning_several_workgroups_are_blended_everywhere() {
    let rig = Rig::new();
    let size = [91, 67];
    let (plain, filtered) = render_pair(&rig, Cmaa2Settings::HIGH, size, DIAGONAL);
    let changes = changed(&plain, &filtered, size[0]);
    for y in 1..size[1] - 1 {
        let y = i32::try_from(y).unwrap();
        assert!(
            changes
                .iter()
                .any(|&(cx, cy)| cy == y && (cx - y).abs() <= 2),
            "row {y} was not blended"
        );
    }
    assert!(changes.iter().all(|(x, y)| (x - y).abs() <= 2));
    assert_within_plain_range(&plain, &filtered);
}

/// Diagonal stripes queue thousands of shape candidates and touched quads, many times the 128
/// candidates and 32 quads that one workgroup covers, but fewer than the lists hold, so the
/// counter-sized dispatches must reach every list entry: every row is blended, down to the
/// last one.
#[test]
fn dense_stripes_are_blended_in_every_row() {
    let rig = Rig::new();
    let size = [300, 200];
    let stripes = "let v=select(0.0,3.0,fract((p.x+p.y)/32.0)<0.5);return vec4<f32>(v,v,v,1.0);";
    let (plain, filtered) = render_pair(&rig, Cmaa2Settings::HIGH, size, stripes);
    let changes = changed(&plain, &filtered, size[0]);
    assert!(changes.len() > 2000, "{} pixels blended", changes.len());
    for y in 2..size[1] - 2 {
        let y = i32::try_from(y).unwrap();
        assert!(
            changes.iter().any(|&(_, cy)| cy == y),
            "row {y} was not blended"
        );
    }
    assert_within_plain_range(&plain, &filtered);
}

/// Opaque input stays opaque. Alpha is never blended: each pixel keeps the alpha it had, even
/// where its color is blended.
#[test]
fn alpha_passes_through_unchanged() {
    let rig = Rig::new();
    let size = [24, 24];
    let (plain, filtered) = render_pair(&rig, Cmaa2Settings::ULTRA, size, DIAGONAL);
    assert!(
        filtered.chunks_exact(4).all(|p| p[3] == 255),
        "opaque input stays opaque"
    );
    assert_ne!(plain, filtered);
    let translucent =
        "let v=select(0.0,3.0,p.x>p.y);return vec4<f32>(v,v,v,select(0.25,1.0,p.x>p.y));";
    let (plain, filtered) = render_pair(&rig, Cmaa2Settings::ULTRA, size, translucent);
    let alpha = |image: &[u8]| image.chunks_exact(4).map(|p| p[3]).collect::<Vec<_>>();
    assert_eq!(alpha(&plain), alpha(&filtered));
    assert_ne!(plain, filtered, "colors are still blended");
}

#[test]
fn resize_rebuilds_the_passes_for_the_new_extent() {
    let rig = Rig::new();
    let mut chain = chain(
        &rig.device,
        &HdrPostSettings {
            resolve: PostResolve::cmaa2(Cmaa2Preset::High),
            ..HdrPostSettings::default()
        },
        [24, 24],
    );
    for size in [[40, 18], [7, 9], [24, 24], [61, 30]] {
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

/// Work lists and counters are rebuilt every frame: a busy frame followed by a flat one on the
/// same chain leaves nothing behind, and repeating a frame reproduces it.
#[test]
fn frames_do_not_leak_state_into_each_other() {
    let rig = Rig::new();
    let size = [40, 40];
    let chain = chain(
        &rig.device,
        &HdrPostSettings {
            resolve: PostResolve::cmaa2(Cmaa2Preset::Ultra),
            ..HdrPostSettings::default()
        },
        size,
    );
    let frame = |body: &str| {
        pixels(
            &rig.device,
            &rig.queue,
            &step(&rig.device, &rig.queue, &chain, 0, body, false),
        )
    };
    let flat = "return vec4<f32>(3.0,3.0,3.0,1.0);";
    let first = frame(DIAGONAL);
    let _ = frame(TEXT);
    let flat_after_busy = frame(flat);
    assert_eq!(flat_after_busy, rig.render(PostResolve::None, size, flat));
    let second = frame(DIAGONAL);
    assert_eq!(first, second);
}
