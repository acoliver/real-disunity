//! Cascaded shadow map fitting for a directional light.
//!
//! Each split covers the slice of the camera's view between two distances. The slice is wrapped
//! in a sphere that depends only on the camera's lens and the distances, not on where the camera
//! looks, so the shadow map keeps its scale when the camera turns. The map is an orthographic box
//! around that sphere along the light's axes, with its centre snapped to whole texels so a
//! moving camera shifts the map by whole texels and shadow edges do not shimmer.

use glam::{Vec3, Vec4};

use super::frame::{LightBasis, orthographic};
use super::{Casters, ShadowCamera, ShadowError, ShadowLight, ShadowSplit};

/// Which split of a cascaded shadow map to fit and how it is drawn. `ratio` holds the far
/// boundary of each of the first three splits as a fraction of the shadow distance, and the last
/// split of `count` ends at the shadow distance. `near_plane_offset` is how far beyond the
/// casters that shadow the slice the near plane of the map is pulled toward the light.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DirectionalSplit {
    pub index: u32,
    pub count: u32,
    pub ratio: Vec3,
    pub resolution: u32,
    pub near_plane_offset: f32,
}

/// The eight corners of the camera's view volume and the axis it looks along.
struct Frustum {
    near: [Vec3; 4],
    far: [Vec3; 4],
    near_depth: f32,
    far_depth: f32,
}

/// The sphere around a slice and the box of the map in light space.
struct Fit {
    centre: Vec3,
    radius: f32,
}

/// Most splits a cascaded map has.
const MAX_SPLITS: u32 = 4;
/// The smallest shadow map, in texels along a side; the largest is what a `u16` holds.
const MIN_RESOLUTION: u16 = 4;
/// The radius of a split's sphere rounds up to a multiple of this fraction so that float noise
/// does not change the scale of the map between frames.
const RADIUS_STEP: f32 = 16.0;

/// The view, projection, culling sphere and planes for one split of a directional light, or
/// `None` when no renderer casts a shadow the light affects.
///
/// # Errors
/// Refuses a split outside its count, ratios that are not above 0, ascending and at most 1, a
/// resolution outside 4 to 65535, a light with no direction and a camera whose volume cannot be fitted.
pub fn directional_split(
    light: &ShadowLight,
    camera: &ShadowCamera,
    casters: &Casters<'_>,
    split: &DirectionalSplit,
) -> Result<Option<ShadowSplit>, ShadowError> {
    let (start, end) = boundaries(split)?;
    let resolution = u16::try_from(split.resolution)
        .ok()
        .filter(|resolution| *resolution >= MIN_RESOLUTION)
        .ok_or(ShadowError::InvalidResolution)?;
    let basis = LightBasis::of(&light.local_to_world)?;
    let frustum = Frustum::of(camera)?;
    let slice = frustum.slice(camera.shadow_distance, (start, end))?;
    let fit = fit_sphere(&slice);
    let boxes = casters.affecting(light);
    if boxes.is_empty() {
        return Ok(None);
    }
    Ok(Some(map(
        &basis,
        &fit,
        &boxes,
        (split, f32::from(resolution)),
    )))
}

/// The slice boundaries of the split as fractions of the shadow distance.
fn boundaries(split: &DirectionalSplit) -> Result<(f32, f32), ShadowError> {
    if split.count == 0 || split.count > MAX_SPLITS || split.index >= split.count {
        return Err(ShadowError::InvalidSplit);
    }
    let ratios = [split.ratio.x, split.ratio.y, split.ratio.z];
    let used = &ratios[..(split.count - 1) as usize];
    let ordered = used.windows(2).all(|pair| pair[0] < pair[1]);
    if !ordered || used.iter().any(|ratio| !(*ratio > 0.0 && *ratio <= 1.0)) {
        return Err(ShadowError::InvalidRatio);
    }
    let end = |index: u32| used.get(index as usize).copied().unwrap_or(1.0);
    let start = if split.index == 0 {
        0.0
    } else {
        end(split.index - 1)
    };
    Ok((start, end(split.index)))
}

impl Frustum {
    /// The view volume of a projection times view matrix, from the matrix's inverse.
    fn of(camera: &ShadowCamera) -> Result<Self, ShadowError> {
        let determinant = camera.culling_matrix.determinant();
        if !determinant.is_finite() || determinant == 0.0 || !camera.origin.is_finite() {
            return Err(ShadowError::InvalidCamera);
        }
        let inverse = camera.culling_matrix.inverse();
        let corner = |index: usize, depth: f32| {
            let x = if index & 1 == 0 { -1.0 } else { 1.0 };
            let y = if index & 2 == 0 { -1.0 } else { 1.0 };
            let point = inverse * Vec4::new(x, y, depth, 1.0);
            point.truncate() / point.w
        };
        let near = [0, 1, 2, 3].map(|index| corner(index, -1.0));
        let far = [0, 1, 2, 3].map(|index| corner(index, 1.0));
        let middle = |corners: &[Vec3; 4]| corners.iter().sum::<Vec3>() / 4.0;
        let axis = (middle(&far) - middle(&near))
            .try_normalize()
            .ok_or(ShadowError::InvalidCamera)?;
        let depth = |corners: &[Vec3; 4]| (middle(corners) - camera.origin).dot(axis);
        let frustum = Self {
            near,
            far,
            near_depth: depth(&near),
            far_depth: depth(&far),
        };
        let finite = near.iter().chain(&far).all(|point| point.is_finite());
        if finite && frustum.far_depth > frustum.near_depth {
            Ok(frustum)
        } else {
            Err(ShadowError::InvalidCamera)
        }
    }

    /// The eight corners of the part of the view between two fractions of the shadow distance,
    /// the near plane of the camera when a fraction is before it and the far plane when the
    /// shadow distance reaches past it.
    fn slice(&self, distance: f32, (start, end): (f32, f32)) -> Result<[Vec3; 8], ShadowError> {
        if !distance.is_finite() || distance <= self.near_depth {
            return Err(ShadowError::InvalidCamera);
        }
        let reach = distance.min(self.far_depth);
        let depths = [
            (start * reach).max(self.near_depth),
            (end * reach).max(self.near_depth),
        ];
        let at = |depth: f32, index: usize| {
            let along = (depth - self.near_depth) / (self.far_depth - self.near_depth);
            self.near[index] + (self.far[index] - self.near[index]) * along
        };
        Ok(std::array::from_fn(|corner| {
            at(depths[corner / 4], corner % 4)
        }))
    }
}

/// The smallest sphere around the slice centred on its axis, with the radius rounded up.
fn fit_sphere(corners: &[Vec3; 8]) -> Fit {
    let middle = |face: &[Vec3]| face.iter().sum::<Vec3>() / 4.0;
    let (near, far) = (middle(&corners[..4]), middle(&corners[4..]));
    let reach = |face: &[Vec3], centre: Vec3| {
        face.iter()
            .map(|corner| corner.distance(centre))
            .fold(0.0_f32, f32::max)
    };
    let (a, b) = (reach(&corners[..4], near), reach(&corners[4..], far));
    let length = near.distance(far);
    let along = if length > f32::EPSILON {
        ((length * length + b * b - a * a) / (2.0 * length)).clamp(0.0, length)
    } else {
        0.0
    };
    let centre = if length > f32::EPSILON {
        near + (far - near) * (along / length)
    } else {
        near
    };
    let radius = corners
        .iter()
        .map(|corner| corner.distance(centre))
        .fold(0.0_f32, f32::max);
    Fit {
        centre,
        radius: ((radius * RADIUS_STEP).ceil() / RADIUS_STEP).max(1.0 / RADIUS_STEP),
    }
}

/// The map of a fitted slice: the box around the sphere along the light's axes with its centre on
/// the texel grid, reaching toward the light far enough for every caster that can shadow the box.
fn map(
    basis: &LightBasis,
    fit: &Fit,
    boxes: &[(Vec3, Vec3)],
    (split, resolution): (&DirectionalSplit, f32),
) -> ShadowSplit {
    // One texel of padding on each side keeps the sphere inside the box after the snap.
    let half = fit.radius / (1.0 - 2.0 / resolution);
    let texel = 2.0 * half / resolution;
    let centre = basis.coordinates_of(fit.centre);
    let snapped = Vec3::new(
        (centre.x / texel).round() * texel,
        (centre.y / texel).round() * texel,
        centre.z,
    );
    let toward_light = toward_light_reach(basis, boxes, snapped, half).max(fit.radius);
    let near = toward_light + split.near_plane_offset.max(0.0);
    let eye = basis.point_at(Vec3::new(snapped.x, snapped.y, snapped.z - near));
    let view = basis.view(eye);
    let projection = orthographic(half, 0.0, near + fit.radius);
    ShadowSplit {
        view,
        projection,
        culling_matrix: projection * view,
        culling_sphere: fit.centre.extend(fit.radius),
        planes: box_planes(basis, snapped, half, fit.radius),
    }
}

/// How far toward the light, from the box's centre, the casters that overlap the box in the
/// light's x and y reach.
fn toward_light_reach(basis: &LightBasis, boxes: &[(Vec3, Vec3)], centre: Vec3, half: f32) -> f32 {
    let mut reach = 0.0_f32;
    for (min, max) in boxes {
        let (mut low, mut high) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
        for corner in 0..8_u32 {
            let pick = |bit: u32, axis: usize| {
                if (corner >> bit) & 1 == 1 {
                    max[axis]
                } else {
                    min[axis]
                }
            };
            let local = basis.coordinates_of(Vec3::new(pick(0, 0), pick(1, 1), pick(2, 2)));
            low = low.min(local);
            high = high.max(local);
        }
        let overlaps = low.x <= centre.x + half
            && high.x >= centre.x - half
            && low.y <= centre.y + half
            && high.y >= centre.y - half;
        if overlaps {
            reach = reach.max(centre.z - low.z);
        }
    }
    reach
}

/// The four sides of the map's box and the plane behind the slice, normals into the box. The
/// side toward the light stays open: a caster there still shadows the slice.
fn box_planes(basis: &LightBasis, centre: Vec3, half: f32, radius: f32) -> Vec<Vec4> {
    let plane = |normal: Vec3, distance: f32| normal.extend(distance);
    vec![
        plane(basis.right, half - centre.x),
        plane(-basis.right, half + centre.x),
        plane(basis.up, half - centre.y),
        plane(-basis.up, half + centre.y),
        plane(-basis.forward, centre.z + radius),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Mat4;

    #[test]
    fn the_last_split_ends_at_the_shadow_distance_and_each_starts_where_the_one_before_ends() {
        let ratio = Vec3::new(0.1, 0.3, 0.6);
        let split = |index| DirectionalSplit {
            index,
            count: 4,
            ratio,
            resolution: 256,
            near_plane_offset: 0.0,
        };
        let spans: Vec<_> = (0..4)
            .map(|index| boundaries(&split(index)).unwrap())
            .collect();
        assert_eq!(spans, [(0.0, 0.1), (0.1, 0.3), (0.3, 0.6), (0.6, 1.0)]);
    }

    #[test]
    fn identity_matrices_make_no_frustum_and_a_flat_one_is_refused() {
        let camera = ShadowCamera {
            culling_matrix: Mat4::ZERO,
            origin: Vec3::ZERO,
            shadow_distance: 10.0,
        };
        assert!(matches!(
            Frustum::of(&camera),
            Err(ShadowError::InvalidCamera)
        ));
    }
}
