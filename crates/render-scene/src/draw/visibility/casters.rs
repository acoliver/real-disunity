//! Shadow caster bounds, and the depth fit that extends a cascade toward the light over its
//! casters.
use std::collections::BinaryHeap;

use glam::{Mat4, Vec3};
use worldrunner_render_plan::SceneObject;

use super::placement::{self, BatchShape, largest_relief};
use super::{Bounds, MeshSelection};
use crate::instances::slot_bounds::{Node, SlotBounds};
use crate::instances::{InstanceHandle, Instances};

/// Caster bounds of a visible shadow-casting placement, before any view's relief stretch.
pub(in crate::draw) fn caster_placement(
    instances: &Instances,
    meshes: &[MeshSelection],
    handle: InstanceHandle,
) -> Option<super::super::caster_edits::CasterBounds> {
    let (key, slot) = instances.location(handle).ok()?;
    let (object, visible) = instances.batches[&key].get(slot);
    if !visible || !object.casts_shadow() {
        return None;
    }
    let mesh = &meshes[key.1];
    let shape = BatchShape::new(mesh, instances, key.2);
    let mut boxes = vec![[Vec3::ZERO; 2]; shape.reliefs().len()];
    shape.fill(mesh, instances, &object, &mut boxes);
    Some(super::super::caster_edits::CasterBounds {
        bounds: placement::placement(&boxes, shape.reliefs(), 0.0),
        relief: largest_relief(shape.reliefs()),
    })
}

/// Extend only the toward-light depth end of the authored receiver volume.
/// XY coverage and its far end stay fixed. Casters beyond the original near
/// plane must also fit the depth projection, not merely survive CPU culling.
pub(in crate::draw) fn caster_matrix(
    instances: &Instances,
    meshes: &[MeshSelection],
    terrain: Option<Bounds>,
    receiver: Mat4,
) -> Mat4 {
    for (&(_, mesh, material), batch) in &instances.batches {
        placement::refresh(&meshes[mesh], instances, material, batch);
    }
    let borrowed: Vec<_> = instances
        .batches
        .values()
        .map(|batch| (batch.slots(), batch.bounds.borrow()))
        .collect();
    let casters: Vec<Casters<'_>> = borrowed
        .iter()
        .map(|(placements, bounds)| Casters { placements, bounds })
        .collect();
    let near = fit_near(
        &casters,
        receiver,
        terrain.map_or(0.0, |bounds| caster_near(bounds, receiver)),
    );
    let scale = (1.0 - near).recip();
    let mut remap = Mat4::IDENTITY;
    remap.z_axis.z = scale;
    remap.w_axis.z = -near * scale;
    remap * receiver
}

/// One batch's placements and refreshed bounds.
struct Casters<'a> {
    placements: &'a [Option<(SceneObject, bool)>],
    bounds: &'a SlotBounds,
}

impl Casters<'_> {
    /// The receiver depth `slot` reaches toward the light, when it is a visible caster.
    fn near(&self, slot: usize, receiver: Mat4) -> Option<f32> {
        let (object, visible) = self.placements[slot]?;
        (visible && object.casts_shadow()).then(|| {
            caster_near(
                placement::placement(self.bounds.boxes(slot), self.bounds.reliefs(), 0.0),
                receiver,
            )
        })
    }
}

/// A tree node waiting to be examined, ordered so the node reaching furthest toward the
/// light comes first and the fit converges early.
struct Candidate {
    light: Bounds,
    batch: usize,
    node: usize,
}

impl PartialEq for Candidate {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}

impl Eq for Candidate {}

impl PartialOrd for Candidate {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Candidate {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // `BinaryHeap` pops the greatest, so the lowest depth must compare greatest.
        other
            .light
            .min
            .z
            .total_cmp(&self.light.min.z)
            .then(other.batch.cmp(&self.batch))
            .then(other.node.cmp(&self.node))
    }
}

/// Whether no caster inside a node with this enclosing light-space box can change `near`.
/// Casters outside the receiver's XY square or wholly beyond its near plane contribute
/// `+0.0`; casters no nearer than a negative `near` contribute at least `near`.
fn settled(light: Bounds, near: f32) -> bool {
    let zero = light.max.x < -1.0
        || light.min.x > 1.0
        || light.max.y < -1.0
        || light.min.y > 1.0
        || light.min.z > 0.0;
    (zero && !is_negative_zero(near)) || (near < 0.0 && light.min.z >= near)
}

fn is_negative_zero(value: f32) -> bool {
    value.to_bits() == (-0.0_f32).to_bits()
}

/// The minimum of `start` and every caster's reach, visiting nodes nearest the light first and
/// skipping nodes that cannot lower it. A negative minimum is unique whatever the order. A zero
/// minimum after a `-0.0` reach takes its sign from the order of `f32::min` calls, so that case
/// repeats the scan in batch and slot order.
fn fit_near(casters: &[Casters<'_>], receiver: Mat4, start: f32) -> f32 {
    let mut near = start;
    let mut negative_zero = is_negative_zero(start);
    let mut heap = BinaryHeap::new();
    let consider = |heap: &mut BinaryHeap<Candidate>, near: f32, batch: usize, node: usize| {
        let [min, max] = casters[batch].bounds.tree().node(node).bounds();
        let light = Bounds { min, max }.transformed_enclosing(receiver);
        if !settled(light, near) {
            heap.push(Candidate { light, batch, node });
        }
    };
    for (batch, casters) in casters.iter().enumerate() {
        if let Some(root) = casters.bounds.tree().root() {
            consider(&mut heap, near, batch, root);
        }
    }
    while let Some(candidate) = heap.pop() {
        if settled(candidate.light, near) {
            continue;
        }
        let batch = &casters[candidate.batch];
        match batch.bounds.tree().node(candidate.node) {
            Node::Inner { children, .. } => {
                for child in children {
                    consider(&mut heap, near, candidate.batch, child);
                }
            }
            Node::Leaf { slots, .. } => {
                for reach in slots
                    .iter()
                    .filter_map(|&slot| batch.near(slot as usize, receiver))
                {
                    negative_zero |= is_negative_zero(reach);
                    near = near.min(reach);
                }
            }
        }
    }
    if near == 0.0 && negative_zero {
        near = casters.iter().fold(start, |near, batch| {
            (0..batch.placements.len())
                .filter_map(|slot| batch.near(slot, receiver))
                .fold(near, f32::min)
        });
    }
    near
}

pub(super) fn caster_near(bounds: Bounds, receiver: Mat4) -> f32 {
    let bounds = bounds.transformed(receiver);
    if bounds.min.x <= 1.0
        && bounds.max.x >= -1.0
        && bounds.min.y <= 1.0
        && bounds.max.y >= -1.0
        && bounds.min.z <= 1.0
    {
        bounds.min.z.min(0.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use worldrunner_render_plan::{MaterialHandle, MeshHandle, SceneRotation, SceneTransform};

    /// One batch whose casters have exactly `boxes` as bounds.
    fn batch(boxes: &[[Vec3; 2]]) -> (Vec<Option<(SceneObject, bool)>>, SlotBounds) {
        let transform = SceneTransform {
            position_millimeters: [0; 3],
            rotation: SceneRotation::IDENTITY,
            scale_millionths: 1_000_000,
        };
        let object = SceneObject::new(
            transform,
            MeshHandle::from_raw(0),
            MaterialHandle::from_raw(0),
        );
        let mut bounds = SlotBounds::default();
        let mut next = boxes.iter();
        bounds.refresh(
            &[0.0_f32.to_bits()],
            (0..boxes.len()).map(|slot| (slot, &transform)),
            |_, stored| stored[0] = *next.next().unwrap(),
        );
        (vec![Some((object, true)); boxes.len()], bounds)
    }

    #[test]
    fn a_zero_fit_takes_the_sign_the_slot_order_scan_gives() {
        let mut receiver = Mat4::IDENTITY;
        receiver.w_axis.z = -0.0;
        // Reaches exactly -0.0 in light depth, then +0.0 outside the square and in front.
        let at_zero = Vec3::new(-0.5, -0.5, -0.0);
        let first = batch(&[[at_zero, at_zero], [Vec3::splat(5.0), Vec3::splat(6.0)]]);
        let second = batch(&[[Vec3::new(0.4, 0.4, 0.5), Vec3::new(0.6, 0.6, 0.7)]]);
        assert!(is_negative_zero(
            Bounds {
                min: at_zero,
                max: at_zero
            }
            .transformed(receiver)
            .min
            .z
        ));
        let casters = [&first, &second].map(|(placements, bounds)| Casters { placements, bounds });
        let scan = casters.iter().fold(0.0_f32, |near, batch| {
            (0..batch.placements.len())
                .filter_map(|slot| batch.near(slot, receiver))
                .fold(near, f32::min)
        });
        for start in [0.0, -0.0] {
            let expected = casters.iter().fold(start, |near, batch| {
                (0..batch.placements.len())
                    .filter_map(|slot| batch.near(slot, receiver))
                    .fold(near, f32::min)
            });
            assert_eq!(
                fit_near(&casters, receiver, start).to_bits(),
                expected.to_bits()
            );
        }
        assert_eq!(scan.abs().to_bits(), 0.0_f32.to_bits());
        // A negative reach anywhere decides the fit whatever the order.
        let deep = batch(&[[Vec3::new(0.0, 0.0, -2.0), Vec3::new(0.1, 0.1, 0.5)]]);
        let casters =
            [&first, &second, &deep].map(|(placements, bounds)| Casters { placements, bounds });
        assert_eq!(
            fit_near(&casters, receiver, 0.0).to_bits(),
            (-2.0_f32).to_bits()
        );
    }
}
