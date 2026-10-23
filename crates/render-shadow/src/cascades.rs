//! Camera split and stable fitting utilities. All distances use caller-selected world units.
use glam::{Mat4, Vec3};

use crate::InvalidFit;

/// Practical split scheme: `lambda` mixes uniform (zero) and logarithmic (one)
/// distances. The final split is exactly `far`; all inputs must be finite.
pub fn practical_splits(
    near: f32,
    far: f32,
    count: u16,
    lambda: f32,
) -> Result<Vec<f32>, InvalidFit> {
    if !near.is_finite()
        || !far.is_finite()
        || near <= 0.0
        || far <= near
        || count == 0
        || !lambda.is_finite()
        || !(0.0..=1.0).contains(&lambda)
    {
        return Err(InvalidFit);
    }
    let splits: Vec<_> = (1..=count)
        .map(|index| {
            if index == count {
                return far;
            }
            let t = f32::from(index) / f32::from(count);
            let logarithmic = near * (far / near).powf(t);
            (near + (far - near) * t) * (1.0 - lambda) + logarithmic * lambda
        })
        .collect();
    if splits.iter().any(|v| !v.is_finite()) || splits.windows(2).any(|w| w[0] >= w[1]) {
        return Err(InvalidFit);
    }
    Ok(splits)
}

/// Boundary belongs to the nearer cascade; values beyond coverage have no cascade.
pub fn cascade_index(distance: f32, splits: &[f32]) -> Option<usize> {
    splits.iter().position(|&split| distance <= split)
}

/// Cross-fade to the next cascade over the final fraction of the current slice.
/// At the split the next cascade has weight one, so selection remains continuous.
pub fn cascade_blend(distance: f32, start: f32, end: f32, band: f32) -> f32 {
    if band == 0.0 {
        return 0.0;
    }
    ((distance - (end - (end - start) * band)) / ((end - start) * band)).clamp(0.0, 1.0)
}

/// Fits a Z-up slice's bounding sphere, quantizes its radius, and snaps its centre
/// in a fixed light basis. Translation cannot alter the XY scale or rotation.
/// One texel of padding retains coverage after snapping. Casters may extend depth.
pub fn stable_frustum_fit(
    view: Mat4,
    projection: Mat4,
    near: f32,
    far: f32,
    toward_light: Vec3,
    resolution: u32,
) -> Result<Mat4, InvalidFit> {
    if !near.is_finite() || !far.is_finite() || near <= 0.0 || far <= near || resolution < 4 {
        return Err(InvalidFit);
    }
    let inverse = projection.inverse();
    let mut corners = [Vec3::ZERO; 8];
    for (index, corner) in corners.iter_mut().enumerate() {
        let x = if index & 1 == 0 { -1.0 } else { 1.0 };
        let y = if index & 2 == 0 { -1.0 } else { 1.0 };
        // Near-plane rays stay finite even when a distant far plane rounds to infinity.
        let ray = inverse.project_point3(Vec3::new(x, y, 0.0));
        let distance = if index < 4 { near } else { far };
        *corner = ray * (distance / -ray.z);
    }
    let center = corners.iter().copied().sum::<Vec3>() / 8.0;
    let radius = corners
        .iter()
        .map(|corner| corner.distance(center))
        .fold(0.0_f32, f32::max);
    let radius = (radius * 16.0).ceil() / 16.0;
    let resolution = f32::from(u16::try_from(resolution).map_err(|_| InvalidFit)?);
    let extent = radius / (1.0 - 2.0 / resolution);
    stable_box_fit(
        view.inverse().transform_point3(center),
        toward_light,
        Vec3::Z,
        extent,
        extent,
        resolution,
    )
}

pub(crate) fn stable_box_fit(
    center: Vec3,
    toward_light: Vec3,
    up: Vec3,
    extent: f32,
    depth: f32,
    resolution: f32,
) -> Result<Mat4, InvalidFit> {
    let direction = toward_light.try_normalize().ok_or(InvalidFit)?;
    let up = up.try_normalize().ok_or(InvalidFit)?;
    let up = if direction.dot(up).abs() > 0.99 {
        up.any_orthonormal_vector()
    } else {
        up
    };
    let rotation = glam::camera::rh::view::look_at_mat4(Vec3::ZERO, -direction, up);
    let texel = extent * 2.0 / resolution;
    let center = (rotation.transform_point3(center) / texel).round() * texel;
    let projection = glam::camera::rh::proj::directx::orthographic(
        -extent,
        extent,
        -extent,
        extent,
        0.0,
        depth * 2.0,
    );
    let matrix = projection * Mat4::from_translation(-center - Vec3::Z * depth) * rotation;
    if !matrix.is_finite() || !matrix.determinant().is_finite() || matrix.determinant() == 0.0 {
        return Err(InvalidFit);
    }
    Ok(matrix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn practical_split_policy_and_rejections() {
        assert_eq!(
            practical_splits(1.0, 81.0, 4, 1.0).unwrap(),
            vec![3.0, 9.0, 27.0, 81.0]
        );
        assert_eq!(
            practical_splits(1.0, 81.0, 4, 0.0).unwrap(),
            vec![21.0, 41.0, 61.0, 81.0]
        );
        for (near, far, count, lambda) in [
            (0.0, 10.0, 4, 0.5),
            (1.0, 1.0, 4, 0.5),
            (1.0, 10.0, 0, 0.5),
            (1.0, 10.0, 4, f32::NAN),
        ] {
            assert!(practical_splits(near, far, count, lambda).is_err());
        }
    }

    #[test]
    fn split_boundaries_and_blend_are_continuous() {
        let splits = [3.0, 9.0, 27.0, 81.0];
        for (index, &split) in splits.iter().enumerate() {
            assert_eq!(cascade_index(split, &splits), Some(index));
            assert_eq!(
                cascade_index(split + 0.001, &splits),
                (index < 3).then_some(index + 1)
            );
            let start = if index == 0 { 1.0 } else { splits[index - 1] };
            assert!((cascade_blend(split, start, split, 0.1) - 1.0).abs() < 1e-6);
            assert!(cascade_blend(split - 0.0001, start, split, 0.1) > 0.999);
            assert!(cascade_blend(start, start, split, 0.1).abs() < 1e-6);
        }
    }

    #[test]
    fn camera_translation_changes_only_whole_texels() {
        let projection = glam::camera::rh::proj::directx::perspective(1.0, 1.5, 0.1, 100.0);
        let base =
            stable_frustum_fit(Mat4::IDENTITY, projection, 0.1, 10.0, Vec3::Z, 1024).unwrap();
        for step in 1..20_u16 {
            let shifted = stable_frustum_fit(
                Mat4::from_translation(Vec3::X * f32::from(step) * 0.001),
                projection,
                0.1,
                10.0,
                Vec3::Z,
                1024,
            )
            .unwrap();
            assert_eq!(base.x_axis, shifted.x_axis);
            assert_eq!(base.y_axis, shifted.y_axis);
            assert_eq!(base.z_axis, shifted.z_axis);
            let delta = (shifted.w_axis - base.w_axis) * 512.0;
            assert!(delta.abs_diff_eq(delta.round(), 0.0001), "{delta:?}");
        }
    }
    #[test]
    fn stable_spheres_enclose_all_slice_corners_and_keep_scale_when_rotating() {
        let projection = glam::camera::rh::proj::directx::perspective(1.0, 1.5, 0.1, 100.0);
        let light = Vec3::new(0.6, 0.0, 0.8);
        let mut previous: Option<Mat4> = None;
        for eye in [Vec3::new(3.0, -5.0, 2.0), Vec3::new(-4.0, 1.0, 7.0)] {
            let view = glam::camera::rh::view::look_at_mat4(eye, Vec3::ZERO, Vec3::Z);
            let matrix = stable_frustum_fit(view, projection, 1.0, 10.0, light, 1024).unwrap();
            if let Some(previous) = previous {
                assert_eq!(matrix.x_axis, previous.x_axis);
                assert_eq!(matrix.y_axis, previous.y_axis);
                assert_eq!(matrix.z_axis, previous.z_axis);
            }
            previous = Some(matrix);
            for x in [-1.0, 1.0] {
                for y in [-1.0, 1.0] {
                    let ray = projection.inverse().project_point3(Vec3::new(x, y, 1.0));
                    for depth in [1.0, 10.0] {
                        let world = view.inverse().transform_point3(ray * (depth / -ray.z));
                        let clip = matrix.project_point3(world);
                        assert!(
                            clip.x.abs() <= 1.0
                                && clip.y.abs() <= 1.0
                                && (0.0..=1.0).contains(&clip.z),
                            "{clip:?}"
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn distant_camera_far_plane_does_not_degenerate_near_slice_rays() {
        let projection = glam::camera::rh::proj::directx::perspective(1.0, 1.5, 0.1, 4_294_967.0);
        stable_frustum_fit(Mat4::IDENTITY, projection, 0.1, 10.0, Vec3::Z, 1024).unwrap();
    }
}
