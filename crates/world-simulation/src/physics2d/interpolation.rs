//! Unity's `Rigidbody2D` interpolation, for drawing only.
//!
//! Physics advances in fixed steps, so a body drawn at its last step pose moves in jumps whenever
//! frames and steps do not line up. An interpolated body is drawn between its poses before and
//! after the last step, by the fraction of a step the clock has run past it, as Unity does. Its
//! transform, contacts and queries keep the physics pose. A body moved outside physics since the
//! step, such as a script teleporting it, is drawn where it is.

use glam::{Mat4, Quat, Vec3};

use super::{BodyType, Interpolation};
use crate::{ComponentDesc, ComponentHandle, ObjectHandle, World, WorldError};

/// An interpolated body's world pose either side of the last physics step.
#[derive(Clone, Copy, Debug)]
pub(crate) struct StepPoses {
    object: ObjectHandle,
    before: Mat4,
    after: Mat4,
}

impl World {
    /// The enabled, moving, interpolated bodies with their current world poses.
    pub(super) fn interpolated_bodies(
        &self,
    ) -> Result<Vec<(ComponentHandle, ObjectHandle, Mat4)>, WorldError> {
        let mut bodies = Vec::new();
        for (_, c) in self.components.iter(self.identity) {
            let ComponentDesc::Body2d(b) = &c.desc else {
                continue;
            };
            if b.interpolation == Interpolation::Interpolate
                && b.enabled
                && b.body_type != BodyType::Static
                && !self.lifetime.components.contains(&c.handle)
                && self.active_in_hierarchy(c.object)?
            {
                bodies.push((c.handle, c.object, self.world_matrix(c.object)?));
            }
        }
        Ok(bodies)
    }

    /// Replaces the step poses with `before` and each body's pose after the step.
    pub(super) fn record_step_poses(
        &mut self,
        before: Vec<(ComponentHandle, ObjectHandle, Mat4)>,
    ) -> Result<(), WorldError> {
        let mut poses = std::collections::BTreeMap::new();
        for (body, object, before) in before {
            let after = self.world_matrix(object)?;
            poses.insert(
                body,
                StepPoses {
                    object,
                    before,
                    after,
                },
            );
        }
        self.physics.step_poses = poses;
        Ok(())
    }

    /// The matrix `object` is drawn with: its world matrix, carried along with the interpolated
    /// pose of the nearest interpolated body at or above it.
    pub fn presented_matrix(&self, object: ObjectHandle) -> Result<Mat4, WorldError> {
        let world = self.world_matrix(object)?;
        let mut current = Some(object);
        while let Some(node) = current {
            if let Some(poses) = self.step_poses_of(node)? {
                let now = self.world_matrix(node)?;
                if now != poses.after {
                    return Ok(world);
                }
                let drawn = blend(poses.before, poses.after, self.step_fraction());
                return Ok(drawn * now.inverse() * world);
            }
            current = self.parent(node)?;
        }
        Ok(world)
    }

    /// The step poses of the body on `object`, while that body is still interpolated.
    fn step_poses_of(&self, object: ObjectHandle) -> Result<Option<StepPoses>, WorldError> {
        let Some((&body, &poses)) = self
            .physics
            .step_poses
            .iter()
            .find(|(_, poses)| poses.object == object)
        else {
            return Ok(None);
        };
        let interpolated = matches!(
            &self.component(body)?.desc,
            ComponentDesc::Body2d(b) if b.interpolation == Interpolation::Interpolate
        );
        Ok(interpolated.then_some(poses))
    }

    /// Unity's `(Time.time - Time.fixedTime) / Time.fixedDeltaTime`, in `[0, 1]`.
    fn step_fraction(&self) -> f32 {
        let step = self.clock.fixed_step.seconds();
        let past = self.clock.accumulator.seconds();
        let fraction = if step > 0.0 {
            (past / step).clamp(0.0, 1.0)
        } else {
            1.0
        };
        std::time::Duration::from_secs_f64(fraction).as_secs_f32()
    }
}

/// Position and rotation interpolated by `t`; scale is the step's result.
fn blend(before: Mat4, after: Mat4, t: f32) -> Mat4 {
    let (_, from_rotation, from) = before.to_scale_rotation_translation();
    let (scale, to_rotation, to) = after.to_scale_rotation_translation();
    let translation: Vec3 = from.lerp(to, t);
    let rotation: Quat = from_rotation.slerp(to_rotation, t);
    Mat4::from_scale_rotation_translation(scale, rotation, translation)
}
