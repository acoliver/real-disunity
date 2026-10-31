//! The material mip bias is derived from the rounded extents and exists only while temporal
//! reconstruction runs below the output extent.
use crate::{
    FrameConfig, HdrPostSettings, PostResolve, RenderScale, UpscaleMode, UpscaleSettings,
    material_mip_bias,
};

const OUTPUTS: [[u32; 2]; 2] = [[1920, 1080], [1452, 921]];
const SCALES: [(u32, f64); 3] = [(500_000, -2.0), (670_000, -1.578), (770_000, -1.377)];

fn scaled(output: [u32; 2], millionths: u32) -> [u32; 2] {
    RenderScale::new(millionths)
        .unwrap()
        .extent(output)
        .unwrap()
}

/// Independent reference: `log2(min(Rw / Ow, Rh / Oh)) - 1` in f64.
fn reference(render: [u32; 2], output: [u32; 2]) -> f64 {
    let ratio = (f64::from(render[0]) / f64::from(output[0]))
        .min(f64::from(render[1]) / f64::from(output[1]));
    ratio.log2() - 1.0
}

#[test]
fn bias_matches_the_design_table_at_both_outputs() {
    for output in OUTPUTS {
        for (millionths, table) in SCALES {
            let render = scaled(output, millionths);
            let bias = f64::from(material_mip_bias(render, output));
            // The table is rounded to three decimals: half a unit in the last place is 5e-4, and
            // the odd output rounds each axis by under one pixel, which moves the ratio by under
            // 1e-3 / 1452 per unit of log2 slope (about 1e-3 / 1452 / ln 2 < 1e-6).
            assert!(
                (bias - table).abs() <= 6e-4,
                "{output:?} at {millionths}: {bias} against table {table}"
            );
        }
    }
}

#[test]
fn bias_equals_the_exact_formula_from_the_rounded_extents() {
    for output in OUTPUTS {
        for (millionths, _) in SCALES {
            let render = scaled(output, millionths);
            let bias = f64::from(material_mip_bias(render, output));
            // f32 has 24 bits of significand, so a value below 4 is within 2^-22 of the f64 one.
            assert!(
                (bias - reference(render, output)).abs() <= 2.0_f64.powi(-22),
                "{output:?} {render:?}"
            );
        }
    }
}

#[test]
fn half_scale_is_exactly_minus_two() {
    assert_eq!(
        material_mip_bias([960, 540], [1920, 1080]).to_bits(),
        (-2.0_f32).to_bits()
    );
    // 461 / 921 is just above one half, so the width (exactly 0.5) is the smaller ratio.
    assert_eq!(
        material_mip_bias([726, 461], [1452, 921]).to_bits(),
        (-2.0_f32).to_bits()
    );
}

#[test]
fn odd_extents_use_the_smaller_axis_ratio() {
    // Width ratio 0.6, height ratio 0.75: the width sets the bias.
    let narrow = material_mip_bias([60, 75], [100, 100]);
    assert!((f64::from(narrow) - reference([60, 75], [100, 100])).abs() <= 2.0_f64.powi(-22));
    assert!(narrow < material_mip_bias([75, 75], [100, 100]));
    // The same ratios on swapped axes give the same bias.
    assert_eq!(
        narrow.to_bits(),
        material_mip_bias([75, 60], [100, 100]).to_bits()
    );
    // Odd extents on both sides.
    let odd = material_mip_bias([727, 460], [1453, 919]);
    assert!((f64::from(odd) - reference([727, 460], [1453, 919])).abs() <= 2.0_f64.powi(-22));
}

#[test]
fn equal_or_larger_extents_give_positive_zero() {
    for output in OUTPUTS {
        assert_eq!(
            material_mip_bias(output, output).to_bits(),
            0.0_f32.to_bits()
        );
    }
    // A render extent above the output is rejected elsewhere; the bias never becomes positive.
    assert_eq!(material_mip_bias([128, 96], [64, 48]).to_bits(), 0);
    // Empty extents are rejected elsewhere; the bias stays zero rather than infinite.
    assert_eq!(material_mip_bias([0, 0], [64, 48]).to_bits(), 0);
    assert_eq!(material_mip_bias([32, 24], [0, 0]).to_bits(), 0);
}

fn settings(mode: Option<UpscaleMode>, resolve: PostResolve) -> HdrPostSettings {
    HdrPostSettings {
        resolve,
        upscale: mode.map(|mode| UpscaleSettings {
            mode,
            sharpness: None,
        }),
        ..HdrPostSettings::default()
    }
}

fn config(render: [u32; 2], output: [u32; 2]) -> FrameConfig {
    FrameConfig::new(output, wgpu::TextureFormat::Rgba16Float, 1).with_render_extent(render)
}

#[test]
fn temporal_reconstruction_derives_the_bias_from_the_config() {
    let taa = settings(Some(UpscaleMode::Temporal), PostResolve::taa(0.9));
    for output in OUTPUTS {
        for (millionths, table) in SCALES {
            let render = scaled(output, millionths);
            let bias = f64::from(taa.material_mip_bias(config(render, output)));
            assert!((bias - table).abs() <= 6e-4, "{output:?} at {millionths}");
            assert_eq!(
                taa.material_mip_bias(config(render, output)).to_bits(),
                material_mip_bias(render, output).to_bits()
            );
        }
    }
}

#[test]
fn every_other_mode_has_exactly_zero_bias() {
    let output = [1920, 1080];
    let reduced = config(scaled(output, 500_000), output);
    let native = config(output, output);
    let cases = [
        ("no upscale", settings(None, PostResolve::None), native),
        ("native TAA", settings(None, PostResolve::taa(0.9)), native),
        (
            "temporal at equal extents",
            settings(Some(UpscaleMode::Temporal), PostResolve::taa(0.9)),
            native,
        ),
        (
            "easu",
            settings(Some(UpscaleMode::Easu), PostResolve::taa(0.9)),
            reduced,
        ),
        (
            "bilinear",
            settings(Some(UpscaleMode::Bilinear), PostResolve::None),
            reduced,
        ),
        (
            "easu with taa",
            settings(Some(UpscaleMode::Easu), PostResolve::taa(0.9)),
            reduced,
        ),
    ];
    for (name, settings, config) in cases {
        assert_eq!(settings.material_mip_bias(config).to_bits(), 0, "{name}");
    }
}
