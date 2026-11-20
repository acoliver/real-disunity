//! Placement bounds for one batch: cached view-independent boxes and a per-view relief term,
//! and the cached eye-independent inputs of each placement's level.

use glam::Vec3;
use worldrunner_render_plan::{SceneFadeReference, SceneObject, SceneTransform};

use super::{Bounds, MeshSelection};
use crate::geometry::{fade_band_meters, millimeters_to_meters, model_matrix};
use crate::instances::fade::Anchor;
use crate::instances::slot_bounds::{Shape, SlotFade, SlotMut, distinct};
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
    /// The level inputs each slot stores, which also depend only on the mesh and materials.
    levels: Shape,
}

impl BatchShape {
    pub fn new(mesh: &MeshSelection, instances: &Instances, material: usize) -> Self {
        let materials: Vec<usize> = drawn_materials(mesh, material).collect();
        let reliefs = distinct(materials.iter().map(|&m| relief_bits(instances, m)));
        let references = distinct(
            materials
                .iter()
                .filter_map(|&m| measured_reference(instances, m)),
        );
        Self {
            materials,
            reliefs,
            levels: Shape {
                levels: mesh.levels(),
                references,
            },
        }
    }

    pub fn reliefs(&self) -> &[u32] {
        &self.reliefs
    }

    /// Stores the level inputs of `object` that do not depend on the eye: its LOD distance
    /// ceilings with its scale applied, its own fade band and origin in metres, and its
    /// transformed fade references.
    pub fn fill_levels(
        &self,
        mesh: &MeshSelection,
        object: &SceneObject,
        SlotMut {
            ceilings,
            fade,
            anchors,
        }: SlotMut<'_>,
    ) {
        mesh.scaled_ceilings(object.lod_distance_scale_millionths(), ceilings);
        let transform = object.transform();
        *fade = SlotFade {
            band: object.fade().map(fade_band_meters),
            origin: millimeters_to_meters(transform.position_millimeters).to_array(),
        };
        for (anchor, &reference) in anchors.iter_mut().zip(&self.levels.references) {
            *anchor = Anchor::referenced(transform, reference);
        }
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
                .filter(|&&material| relief_bits(instances, material) == relief)
                .map(|&material| view_independent(mesh, instances, transform, reach, material))
                .reduce(union)
                .expect("every relief group has a material");
            *stored = [group.min, group.max];
        }
    }
}

/// The materials a batch draws with: its own, then every slot material of its mesh.
fn drawn_materials(mesh: &MeshSelection, material: usize) -> impl Iterator<Item = usize> {
    std::iter::once(material).chain(
        mesh.material_slots
            .iter()
            .flatten()
            .flatten()
            .map(|slot| slot.material().raw() as usize),
    )
}

/// The relief scale of `material`, by bit pattern so grouping is exact.
fn relief_bits(instances: &Instances, material: usize) -> u32 {
    instances.impostor_extents[material][1].to_bits()
}

/// The fade reference `material` measures from, if it can fade out. A material that keeps
/// fragments at every distance never measures its reference.
fn measured_reference(instances: &Instances, material: usize) -> Option<SceneFadeReference> {
    let fade = instances.fades[material];
    fade.reference().filter(|_| fade.can_empty())
}

/// Whether the stored boxes and level inputs of `batch` are already those of `mesh`, its
/// materials and its placements, so that `refresh` has nothing to fill. It reads the same
/// materials as `BatchShape::new` but builds none of its vectors, because it runs for every
/// batch of every view. `Instances::edits` cannot stand in: a material's relief scale and fade
/// reference change without it.
pub(super) fn is_current(
    mesh: &MeshSelection,
    instances: &Instances,
    material: usize,
    batch: &InstanceBatch,
) -> bool {
    let materials = || drawn_materials(mesh, material);
    batch.bounds.borrow().is_current(
        materials().map(|m| relief_bits(instances, m)),
        mesh.levels(),
        materials().filter_map(|m| measured_reference(instances, m)),
    )
}

#[cfg(test)]
thread_local! {
    /// How many times `refresh` has run on this thread, so tests can pin how often a frame asks.
    pub(super) static CHECKS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Fills the cached boxes and level inputs of every live placement of `batch` and regroups
/// them.
pub(super) fn refresh(
    mesh: &MeshSelection,
    instances: &Instances,
    material: usize,
    batch: &InstanceBatch,
) {
    #[cfg(test)]
    CHECKS.with(|checks| checks.set(checks.get() + 1));
    if is_current(mesh, instances, material, batch) {
        return;
    }
    let shape = BatchShape::new(mesh, instances, material);
    batch.bounds.borrow_mut().refresh(
        (shape.reliefs(), &shape.levels),
        batch
            .slots()
            .iter()
            .enumerate()
            .filter_map(|(slot, placement)| placement.as_ref().map(|(object, _)| (slot, object))),
        |object, boxes, slot| {
            shape.fill(mesh, instances, object, boxes);
            shape.fill_levels(mesh, object, slot);
        },
    );
}

/// Refreshes every batch's cached boxes and level inputs unless the placements and tables
/// are as they were when every batch was last found current, as in every view of a frame
/// while the camera moves and nothing is edited.
pub(super) fn refresh_all(instances: &Instances, meshes: &[MeshSelection]) {
    let stamp = instances.bounds_stamp(meshes.len());
    if instances.bounds_current_at(stamp) {
        return;
    }
    for (&(_, mesh, material), batch) in &instances.batches {
        refresh(&meshes[mesh], instances, material, batch);
    }
    instances.set_bounds_current_at(stamp);
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
