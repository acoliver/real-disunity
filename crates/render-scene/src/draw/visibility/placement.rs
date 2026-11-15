//! Placement bounds for one batch: cached view-independent boxes and a per-view relief term.

use glam::Vec3;
use worldrunner_render_plan::{SceneObject, SceneTransform};

use super::{Bounds, MeshSelection};
use crate::geometry::model_matrix;
use crate::instances::{InstanceBatch, Instances};

/// The materials a batch's placements draw with, and their distinct relief scales.
///
/// A placement draws with the batch material and every slot material of its mesh. Each
/// material's bounds are view-independent except for impostor relief, which grows by
/// the relief scale times the view's ray stretch. The cache therefore keeps one box per
/// distinct relief scale. Each box is the union of its materials' view-independent
/// bounds. Floating-point subtraction and addition round monotonically, so expanding
/// a union equals the union of the expanded boxes, bit for bit.
pub(super) struct BatchShape {
    materials: Vec<usize>,
    /// Distinct relief scales, compared by bit pattern so grouping is exact.
    reliefs: Vec<u32>,
}

impl BatchShape {
    pub fn new(mesh: &MeshSelection, instances: &Instances, material: usize) -> Self {
        let materials: Vec<usize> = std::iter::once(material)
            .chain(
                mesh.material_slots
                    .iter()
                    .flatten()
                    .flatten()
                    .map(|slot| slot.material().raw() as usize),
            )
            .collect();
        let mut reliefs: Vec<u32> = Vec::new();
        for &material in &materials {
            let relief = instances.impostor_extents[material][1].to_bits();
            if !reliefs.contains(&relief) {
                reliefs.push(relief);
            }
        }
        Self { materials, reliefs }
    }

    pub fn reliefs(&self) -> &[u32] {
        &self.reliefs
    }

    /// Stores each relief group's view-independent box around `object` in `boxes`. A skinned
    /// placement is bounded where its pose can carry its vertices, not where its bind geometry
    /// lies; setting a slot's object, and so its pose, clears its stored boxes.
    pub fn fill(
        &self,
        mesh: &MeshSelection,
        instances: &Instances,
        object: &SceneObject,
        boxes: &mut [[Vec3; 2]],
    ) {
        let transform = object.transform();
        let reach = instances.skin.reach(object, mesh.radius());
        for (stored, &relief) in boxes.iter_mut().zip(&self.reliefs) {
            let group = self
                .materials
                .iter()
                .filter(|&&material| instances.impostor_extents[material][1].to_bits() == relief)
                .map(|&material| view_independent(mesh, instances, transform, reach, material))
                .reduce(union)
                .expect("every relief group has a material");
            *stored = [group.min, group.max];
        }
    }
}

/// Fills the cached boxes of every live placement of `batch` and regroups them.
pub(super) fn refresh(
    mesh: &MeshSelection,
    instances: &Instances,
    material: usize,
    batch: &InstanceBatch,
) {
    let shape = BatchShape::new(mesh, instances, material);
    batch.bounds.borrow_mut().refresh(
        shape.reliefs(),
        batch
            .slots()
            .iter()
            .enumerate()
            .filter_map(|(slot, placement)| placement.as_ref().map(|(object, _)| (slot, object))),
        |object, boxes| shape.fill(mesh, instances, object, boxes),
    );
}

/// World bounds of a placement with stored `boxes`, one per relief scale in `reliefs`, for
/// a view with this relief ray stretch.
pub(super) fn placement(boxes: &[[Vec3; 2]], reliefs: &[u32], stretch: f32) -> Bounds {
    boxes
        .iter()
        .zip(reliefs)
        .map(|(&[min, max], &relief)| {
            Bounds { min, max }.expanded(Vec3::splat(f32::from_bits(relief) * stretch))
        })
        .reduce(union)
        .expect("a batch has at least one material")
}

/// The largest relief scale, which bounds every relief group's expansion.
pub(super) fn largest_relief(reliefs: &[u32]) -> f32 {
    reliefs
        .iter()
        .copied()
        .map(f32::from_bits)
        .fold(0.0, f32::max)
}

/// Wind, shaped-wind and impostor capture extents of one material around a placement.
fn view_independent(
    mesh: &MeshSelection,
    instances: &Instances,
    transform: &SceneTransform,
    reach: f32,
    material: usize,
) -> Bounds {
    let impostor = instances.impostor_extents[material];
    mesh.wind_bounds(
        transform,
        instances.wind_extents[material],
        instances.curved_wind[material],
        [impostor[0], impostor[2]],
        reach,
    )
    .expanded(Vec3::splat(
        impostor[2] * model_matrix(transform).x_axis.truncate().length(),
    ))
}

pub(super) fn union(a: Bounds, b: Bounds) -> Bounds {
    Bounds {
        min: a.min.min(b.min),
        max: a.max.max(b.max),
    }
}
