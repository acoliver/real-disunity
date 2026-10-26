use super::{ElevationColumnScattering, tests::earth};
use glam::Vec3;

// Scalar f64 reference with analytic sample positions, independent of the
// production vector integration, fit helper and accumulated travel distance.
fn reference(
    model: ElevationColumnScattering,
    view: f64,
    sun: f64,
    altitude: f64,
) -> [[f64; 3]; 3] {
    let radius = f64::from(model.planet_radius);
    let observer = radius + altitude;
    let outer = f64::from(model.outer_radius);
    let step = ((outer * outer - observer.powi(2) * view.cos().powi(2)).sqrt()
        - observer * view.sin())
        / 20.0;
    let scales = model.scale_heights.map(f64::from);
    let weights = scales.map(|h| fitted_weight(h, sun, view));
    let mut output = [[0.0; 3]; 3];
    for (channel, molecular) in model.molecular.to_array().into_iter().enumerate() {
        let coefficients = [
            f64::from(molecular),
            1.05 * f64::from(model.aerosol[channel]),
        ];
        let mut depth = [0.0; 2];
        for index in 0..20 {
            let distance = (f64::from(index) + 0.5) * step;
            let z = observer + distance * view.sin();
            let local_radius = ((distance * view.cos()).powi(2) + z * z).sqrt();
            let segment = scales.map(|h| (-(local_radius - radius) / h).exp() * step);
            let clearance = (radius / local_radius).acos() + (z / local_radius).acos() + sun;
            let visibility = (0.5 + clearance / f64::from(model.solar_diameter)).clamp(0.0, 1.0);
            let mut optical_depth = 0.0;
            for species in 0..2 {
                depth[species] += segment[species];
                optical_depth +=
                    coefficients[species] * (depth[species] + segment[species] * weights[species]);
            }
            for species in 0..2 {
                output[species][channel] += segment[species] * (-optical_depth).exp() * visibility;
            }
        }
        output[2][channel] = (-(coefficients[0] * depth[0] + coefficients[1] * depth[1])).exp();
    }
    output
}
fn fitted_weight(height: f64, sun: f64, view: f64) -> f64 {
    let degrees = sun * 180.0 / std::f64::consts::PI;
    let amplitudes = [
        550.6967 * height.powf(0.459_061_9),
        30.29522 * height.powf(0.453_363_9),
    ];
    let factors = if degrees < 0.0 {
        amplitudes.map(|a| a * (1.45_f64.ln() * -degrees).exp())
    } else {
        let denominator = 1.0 + (degrees / (0.034_256_02 * height.powf(0.444_784))).powf(1.22);
        [
            0.042_687_92 * height - 4.773_326 + amplitudes[0] / denominator,
            0.002_524_803 * height - 1.745_569 + amplitudes[1] / denominator,
        ]
    };
    (factors[0] * view.sin() + factors[1] * (-7.0 * view).exp()) / 255.0
}
#[test]
fn fixed_model_matches_independent_scalar_reference() {
    let model = earth();
    for (view, sun, altitude) in [
        (0.0, 0.0, 0.1),
        (0.6, 1.2, 0.1),
        (
            std::f32::consts::FRAC_PI_2,
            std::f32::consts::FRAC_PI_2,
            0.1,
        ),
        (0.0, -0.05, 0.1),
        (1.0, 0.3, 10_000.0),
    ] {
        let actual = model.integrate(view, sun, altitude);
        let expected = reference(model, f64::from(view), f64::from(sun), f64::from(altitude));
        for (layer, values) in [actual.molecular, actual.aerosol, actual.transmission]
            .iter()
            .enumerate()
        {
            for channel in 0..3 {
                let error = (f64::from(values[channel]) - expected[layer][channel]).abs();
                assert!(
                    error < expected[layer][channel].abs() * 0.0002 + 1e-7,
                    "layer {layer}, channel {channel}: {values:?} vs {:?}",
                    expected[layer]
                );
            }
        }
    }
}
#[test]
fn horizon_twilight_has_no_sunward_shadow_asymmetry() {
    let mut model = earth();
    model.solar_diameter = 1e-6;
    model.aerosol = Vec3::ZERO;
    let columns = model.integrate(0.0, -0.05, 0.1);
    let sunward = model.radiance(columns, (-0.05_f32).cos());
    let antisunward = model.radiance(columns, -(-0.05_f32).cos());
    assert!(sunward.min_element() > 0.0);
    assert_eq!(sunward, antisunward);
    model.aerosol = earth().aerosol;
    let columns = model.integrate(0.0, -0.05, 0.1);
    assert!(model.radiance(columns, 1.0).length() > model.radiance(columns, -1.0).length());
}
#[test]
fn near_outer_boundary_columns_remain_nonnegative() {
    let sample = earth().integrate(0.627_100_9, 0.5, 59_999.9);
    assert!(sample.molecular.min_element() >= 0.0);
    assert!(sample.aerosol.min_element() >= 0.0);
    assert!(sample.transmission.min_element() >= 0.0);
    assert!(sample.transmission.max_element() <= 1.0);
}
