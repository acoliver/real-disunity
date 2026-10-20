use super::*;
use glam::Vec3;

fn fit() -> DirectionalFit {
    DirectionalFit {
        center: Vec3::new(3.0, 5.0, 7.0),
        toward_light: Vec3::Z,
        up: Vec3::Y,
        half_extent: 4.0,
        depth_half_extent: 10.0,
    }
}

#[test]
fn fit_maps_center_and_depth_band_to_webgpu_clip_space() {
    let fit = fit();
    let matrix = fit.matrix().unwrap();
    assert!((matrix.project_point3(fit.center) - Vec3::new(0.0, 0.0, 0.5)).length() < 1e-6);
    assert!(matrix.project_point3(fit.center + Vec3::Z * 10.0).z.abs() < 1e-6);
    assert!((matrix.project_point3(fit.center - Vec3::Z * 10.0).z - 1.0).abs() < 1e-6);
    assert!((matrix.project_point3(fit.center + Vec3::X * 4.0).x - 1.0).abs() < 1e-6);
    assert!(
        DirectionalFit {
            toward_light: Vec3::Y,
            ..fit
        }
        .matrix()
        .unwrap()
        .is_finite()
    );
}

#[test]
fn invalid_fits_return_errors_instead_of_nonfinite_matrices() {
    for invalid in [
        DirectionalFit {
            center: Vec3::NAN,
            ..fit()
        },
        DirectionalFit {
            toward_light: Vec3::ZERO,
            ..fit()
        },
        DirectionalFit {
            up: Vec3::ZERO,
            ..fit()
        },
        DirectionalFit {
            half_extent: 0.0,
            ..fit()
        },
        DirectionalFit {
            depth_half_extent: f32::INFINITY,
            ..fit()
        },
    ] {
        assert!(invalid.matrix().is_err());
    }
}
