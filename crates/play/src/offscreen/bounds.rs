//! The extent of a mesh and what a culling plane or a shadow sphere does to it.
//!
//! A culling record carries planes and a shadow list a sphere, and the world holds no extent for
//! a renderer, so the host takes it from the mesh it uploaded: the box that holds the positions
//! of the mesh's vertices, moved by the renderer's matrix. A plane is a unit normal and a
//! distance, and the normals of a view volume point into it, as the engine layer builds them, so
//! a point is inside a plane when `dot(normal, point) + distance` is not negative.

use glam::{Mat4, Vec3};
use worldrunner_render_program::unity::offscreen::frame::{ChannelFormat, Mesh};

use super::assets::POSITION;

/// The box that holds a mesh's vertices in its own coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl Bounds {
    /// The box of the positions in `mesh`, or `None` for a mesh with no position channel of three
    /// floats or with no vertex.
    #[must_use]
    pub fn of_mesh(mesh: &Mesh) -> Option<Self> {
        let channel = mesh.channels.iter().find(|channel| {
            channel.source == POSITION && channel.format == ChannelFormat::Float32x3
        })?;
        let mut floats = channel
            .bytes
            .chunks_exact(4)
            .map(|word| f32::from_le_bytes([word[0], word[1], word[2], word[3]]));
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        let mut vertices = 0_u32;
        while let (Some(x), Some(y), Some(z)) = (floats.next(), floats.next(), floats.next()) {
            for (axis, value) in [x, y, z].into_iter().enumerate() {
                min[axis] = min[axis].min(value);
                max[axis] = max[axis].max(value);
            }
            vertices += 1;
        }
        let finite = min.iter().chain(&max).all(|value| value.is_finite());
        (vertices > 0 && finite).then_some(Self { min, max })
    }

    /// The eight corners of the box moved by `local_to_world`.
    #[must_use]
    pub fn corners(&self, local_to_world: &[f32; 16]) -> [Vec3; 8] {
        let matrix = Mat4::from_cols_array(local_to_world);
        let pick = |upper: bool, axis: usize| {
            if upper {
                self.max[axis]
            } else {
                self.min[axis]
            }
        };
        std::array::from_fn(|corner| {
            let local = Vec3::new(
                pick(corner & 1 != 0, 0),
                pick(corner & 2 != 0, 1),
                pick(corner & 4 != 0, 2),
            );
            matrix.transform_point3(local)
        })
    }
}

/// Whether the points are all outside one of `planes`, so a body that holds them is outside the
/// volume the planes bound. No plane leaves every body inside.
#[must_use]
pub fn outside_planes(planes: &[[f32; 4]], points: &[Vec3]) -> bool {
    planes.iter().any(|plane| {
        let normal = Vec3::new(plane[0], plane[1], plane[2]);
        points
            .iter()
            .all(|point| normal.dot(*point) + plane[3] < 0.0)
    })
}

/// The centre and the radius of the sphere that holds the points around their box's centre.
#[must_use]
pub fn enclosing_sphere(points: &[Vec3]) -> (Vec3, f32) {
    let (min, max) = points.iter().fold(
        (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
        |(min, max), point| (min.min(*point), max.max(*point)),
    );
    let centre = (min + max) * 0.5;
    (centre, (max - centre).length())
}

/// Whether the sphere of `centre` and `radius` touches `sphere`, a centre and a radius. A sphere
/// of no radius is no limit.
#[must_use]
pub fn touches_sphere(sphere: [f32; 4], centre: Vec3, radius: f32) -> bool {
    if sphere[3] <= 0.0 {
        return true;
    }
    let apart = (centre - Vec3::new(sphere[0], sphere[1], sphere[2])).length();
    apart <= sphere[3] + radius
}
