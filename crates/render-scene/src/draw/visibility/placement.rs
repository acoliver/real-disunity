//! Placement bounds for one batch: a cached view-independent part and a per-view relief term.

use std::cell::RefMut;

use glam::Vec3;
use worldrunner_render_plan::SceneTransform;

use super::{Bounds, MeshSelection};
use crate::geometry::model_matrix;
use crate::instances::{InstanceBatch, Instances, slot_bounds::SlotBounds};

/// The bounds of every placement in one mesh/material batch.
///
/// A placement draws with the batch material and every slot material of its mesh. Each
/// material's bounds are view-independent except for impostor relief, which grows by
/// the relief scale times the view's ray stretch. The cache therefore keeps one box per
/// distinct relief scale. Each box is the union of its materials' view-independent
/// bounds. Floating-point subtraction and addition round monotonically, so expanding
/// a union equals the union of the expanded boxes, bit for bit.
pub(super) struct BatchBounds<'a> {
    mesh: &'a MeshSelection,
    instances: &'a Instances,
    materials: Vec<usize>,
    /// Distinct relief scales, compared by bit pattern so grouping is exact.
    reliefs: Vec<u32>,
    cache: RefMut<'a, SlotBounds>,
}

impl<'a> BatchBounds<'a> {
    pub fn new(
        mesh: &'a MeshSelection,
        instances: &'a Instances,
        material: usize,
        batch: &'a InstanceBatch,
    ) -> Self {
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
        Self {
            mesh,
            instances,
            materials,
            reliefs,
            cache: batch.bounds.borrow_mut(),
        }
    }

    /// World bounds of the placement in `slot` for a view with this relief ray stretch.
    pub fn placement(&mut self, slot: usize, transform: &SceneTransform, stretch: f32) -> Bounds {
        let Self {
            mesh,
            instances,
            materials,
            reliefs,
            cache,
        } = self;
        let boxes = cache.get_or_fill(slot, reliefs.len(), |boxes| {
            for (stored, &relief) in boxes.iter_mut().zip(reliefs.iter()) {
                let group = materials
                    .iter()
                    .filter(|&&material| {
                        instances.impostor_extents[material][1].to_bits() == relief
                    })
                    .map(|&material| view_independent(mesh, instances, transform, material))
                    .reduce(union)
                    .expect("every relief group has a material");
                *stored = [group.min, group.max];
            }
        });
        boxes
            .iter()
            .zip(reliefs.iter())
            .map(|(&[min, max], &relief)| {
                Bounds { min, max }.expanded(Vec3::splat(f32::from_bits(relief) * stretch))
            })
            .reduce(union)
            .expect("a batch has at least one material")
    }
}

/// Wind, shaped-wind and impostor capture extents of one material around a placement.
fn view_independent(
    mesh: &MeshSelection,
    instances: &Instances,
    transform: &SceneTransform,
    material: usize,
) -> Bounds {
    let impostor = instances.impostor_extents[material];
    mesh.wind_bounds(
        transform,
        instances.wind_extents[material],
        instances.curved_wind[material],
        [impostor[0], impostor[2]],
    )
    .expanded(Vec3::splat(
        impostor[2] * model_matrix(transform).x_axis.truncate().length(),
    ))
}

fn union(a: Bounds, b: Bounds) -> Bounds {
    Bounds {
        min: a.min.min(b.min),
        max: a.max.max(b.max),
    }
}
