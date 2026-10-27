use super::*;
use worldrunner_render_lighting::atmosphere::HeightFog;

fn fog_frame(scene: &RenderScene) -> FrameUniform {
    let mut frame = frame(scene);
    let mut fog = HeightFog::new(0.2, 0.3, 0.0, 1.0, glam::Vec3::ZERO).unwrap();
    fog.full_ray_density = true;
    frame.height_fog = Some(fog);
    frame
}

fn transmission(frame: &FrameUniform, x: f32, y: f32, depth: f32) -> f32 {
    let h = frame.view_projection.as_dmat4().inverse()
        * glam::DVec4::new(
            f64::from(x / 16.0 - 1.0),
            f64::from(1.0 - y / 16.0),
            f64::from(depth),
            1.0,
        );
    let offset = h.truncate() / h.w - frame.camera_position.as_dvec3();
    let mut average = 0.0;
    for i in 0..4096 {
        let z = f64::from(frame.camera_position.z) + offset.z * (f64::from(i) + 0.5) / 4096.0;
        average += 0.2 * (-0.3 * z).exp() / 4096.0;
    }
    half::f16::from_f64((-average * (offset.length() - 1.0)).exp()).to_f32()
}

#[test]
fn full_ray_water_colour_receiver_and_ssr_replacement_at_one_and_four_samples() {
    let context = context();
    let maps = environment(&context);
    for samples in [1, 4] {
        let miss = source(true, true);
        let hit = source(true, false);
        let frame = fog_frame(&miss);
        let baseline =
            render_with_environment(&context, &miss, samples, &maps, &super::frame(&miss));
        let fogged = render_with_environment(&context, &miss, samples, &maps, &frame);
        let reflected = render_with_environment(&context, &hit, samples, &maps, &frame);
        for sample in 0..samples {
            check_water(&context, &baseline, &fogged, &reflected, &frame, sample);
        }
    }
}

fn check_water(
    context: &GpuContext,
    baseline: &FrameRenderer,
    fogged: &FrameRenderer,
    hit: &FrameRenderer,
    frame: &FrameUniform,
    sample: u32,
) {
    let (_, material, published) = fogged.water_receivers().unwrap();
    let x = channel(context, material.texture(), sample, 0);
    let y = channel(context, material.texture(), sample, 1);
    let depth = channel(context, fogged.targets().depth().texture(), sample, 0);
    let air = channel(context, published.texture(), sample, 3);
    let environment = channel(context, published.texture(), sample, 0);
    let base = channel(
        context,
        baseline.water_receivers().unwrap().2.texture(),
        sample,
        0,
    );
    let color = channel(context, fogged.targets().scene().texture(), sample, 0);
    let reflected = channel(context, hit.targets().scene().texture(), sample, 0);
    let trace = channel(context, hit.ssr.as_ref().unwrap().textures()[4], sample, 0);
    let confidence = channel(context, hit.ssr.as_ref().unwrap().textures()[4], sample, 3);
    let coefficients =
        worldrunner_render_lighting::ibl::brdf_integral(0.5, 0.5, 64).map(rounded_half);
    let factor = rounded_half(0.02037) * coefficients[0] + coefficients[1];
    let mut hits = 0;
    for row in 16..30 {
        for col in 13..19 {
            let i = row * 32 + col;
            let expected = transmission(
                frame,
                gpu_scalar(col) + x[i],
                gpu_scalar(row) + y[i],
                depth[i],
            );
            assert!(
                (air[i] - expected).abs() < 0.001,
                "receiver {sample} {i}: {} != {expected}",
                air[i]
            );
            assert!((environment[i] - base[i] * expected).abs() < 0.0001);
            assert!((color[i] - environment[i]).abs() < 0.0001);
            let replacement =
                color[i] + trace[i] * factor * expected - environment[i] * confidence[i];
            check_replacement(
                i,
                (hit.targets().config().samples, factor, expected),
                (&color, &environment, &reflected, &trace, &confidence),
                replacement,
            );
            if confidence[i] > 0.1 && (reflected[i] - environment[i]).abs() > 0.00001 {
                hits += 1;
            }
        }
    }
    assert!(hits > 0, "SSR hit required for sample {sample}");
}

#[test]
fn full_ray_forward_publication_matches_fogged_colour_at_one_and_four_samples() {
    let context = context();
    let maps = environment(&context);
    for samples in [1, 4] {
        let source = source(false, false);
        let baseline = render_with_environment(&context, &source, samples, &maps, &frame(&source));
        let fogged =
            render_with_environment(&context, &source, samples, &maps, &fog_frame(&source));
        for sample in 0..samples {
            let a = baseline.ssr.as_ref().unwrap().textures();
            let b = fogged.ssr.as_ref().unwrap().textures();
            let base = channel(&context, a[2], sample, 0);
            let env = channel(&context, b[2], sample, 0);
            let factor = channel(&context, b[3], sample, 0);
            let base_factor = channel(&context, a[3], sample, 0);
            let color = channel(&context, fogged.targets().scene().texture(), sample, 0);
            let base_color = channel(&context, baseline.targets().scene().texture(), sample, 0);
            let mut checked = 0;
            for i in 0..1024 {
                if base[i] > 0.001 && factor[i] > 0.0 {
                    let air = factor[i] / base_factor[i];
                    assert!((env[i] - base[i] * air).abs() < 0.0001);
                    assert!((color[i] - base_color[i] * air).abs() < 0.001);
                    assert!(air < 0.99);
                    checked += 1;
                }
            }
            assert!(checked > 16);
        }
    }
}

type ReplacementChannels<'a> = (&'a [f32], &'a [f32], &'a [f32], &'a [f32], &'a [f32]);

fn check_replacement(
    i: usize,
    settings: (u32, f32, f32),
    channels: ReplacementChannels<'_>,
    replacement: f32,
) {
    let (samples, factor, expected) = settings;
    let (color, environment, reflected, trace, confidence) = channels;
    if samples == 1 && confidence[i] > 0.1 {
        assert!(
            (reflected[i] - replacement).abs() < 0.001,
            "replacement {i}: {} != {replacement}",
            reflected[i]
        );
    } else {
        let deltas = [
            i - 33,
            i - 32,
            i - 31,
            i - 1,
            i,
            i + 1,
            i + 31,
            i + 32,
            i + 33,
        ]
        .map(|j| color[i] + trace[j] * factor * expected - environment[i] * confidence[j]);
        let low = deltas.into_iter().fold(f32::INFINITY, f32::min);
        let high = deltas.into_iter().fold(f32::NEG_INFINITY, f32::max);
        assert!(
            reflected[i] >= low - 0.001 && reflected[i] <= high + 0.001,
            "sample replacement {i}: {} outside {low}..{high}",
            reflected[i]
        );
    }
}
