//! Presentation poses for 2D bodies, without changing physics state.
//!
//! Interpolate blends the poses before and after the last fixed step. Extrapolate predicts
//! from the step result and current velocity over the remaining fraction of a fixed step.
//! Transforms, contacts and queries keep the physics pose. A body moved outside physics
//! since the step, such as a script teleporting it, is drawn where it is.

use glam::{Mat4, Quat, Vec3};

use super::{Body2d, BodyType, Interpolation, PhysicsError};
use crate::{ComponentDesc, ComponentHandle, ObjectHandle, World, WorldError};

/// A body's world pose either side of the last physics step.
#[derive(Clone, Copy, Debug)]
pub(crate) struct StepPoses {
    object: ObjectHandle,
    before: Mat4,
    after: Mat4,
}

impl World {
    /// The enabled, moving bodies requiring presentation poses, with their current world poses.
    pub(super) fn interpolated_bodies(
        &self,
    ) -> Result<Vec<(ComponentHandle, ObjectHandle, Mat4)>, WorldError> {
        let mut bodies = Vec::new();
        for (_, c) in self.components.iter(self.identity) {
            let ComponentDesc::Body2d(b) = &c.desc else {
                continue;
            };
            if b.interpolation != Interpolation::None
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

    /// The world matrix carried along with the presentation pose of the nearest body at or
    /// above `object`. A script edit since the last step takes precedence over presentation.
    pub fn presented_matrix(&self, object: ObjectHandle) -> Result<Mat4, WorldError> {
        let world = self.world_matrix(object)?;
        let mut current = Some(object);
        while let Some(node) = current {
            if let Some((poses, body)) = self.step_poses_of(node)? {
                let now = self.world_matrix(node)?;
                if now != poses.after {
                    return Ok(world);
                }
                let drawn = match body.interpolation {
                    Interpolation::Interpolate => {
                        blend(poses.before, poses.after, self.step_fraction())
                    }
                    Interpolation::Extrapolate => self.predicted_matrix(poses.after, body)?,
                    Interpolation::None => unreachable!(),
                };
                return Ok(if node == object {
                    drawn
                } else {
                    drawn * now.inverse() * world
                });
            }
            current = self.parent(node)?;
        }
        Ok(world)
    }

    /// The recorded poses and current presentation settings of the body on `object`.
    fn step_poses_of(
        &self,
        object: ObjectHandle,
    ) -> Result<Option<(StepPoses, &Body2d)>, WorldError> {
        let Some((&handle, &poses)) = self
            .physics
            .step_poses
            .iter()
            .find(|(_, poses)| poses.object == object)
        else {
            return Ok(None);
        };
        let ComponentDesc::Body2d(body) = &self.component(handle)?.desc else {
            return Ok(None);
        };
        if body.interpolation == Interpolation::None
            || (body.interpolation == Interpolation::Extrapolate
                && (!body.enabled
                    || body.sleeping
                    || body.body_type == BodyType::Static
                    || self.lifetime.components.contains(&handle)
                    || !self.active_in_hierarchy(object)?))
        {
            return Ok(None);
        }
        Ok(Some((poses, body)))
    }

    fn predicted_matrix(&self, after: Mat4, body: &Body2d) -> Result<Mat4, WorldError> {
        let elapsed = self
            .clock
            .accumulator
            .seconds()
            .clamp(0.0, self.clock.fixed_step.seconds());
        let dt = std::time::Duration::try_from_secs_f64(elapsed)
            .map_err(|_| PhysicsError::NumericOverflow)?
            .as_secs_f32();
        let plane = self.physics.settings.plane;
        let angular = if body.freeze_rotation {
            0.0
        } else {
            body.angular_velocity
        };
        let mut drawn = Mat4::from_quat(plane.rotation(angular * dt)) * after;
        let velocity = plane.vector(body.velocity * body.linear_mask());
        drawn.w_axis = (after.w_axis.truncate() + velocity * dt).extend(1.0);
        if !drawn.is_finite() {
            return Err(PhysicsError::NumericOverflow.into());
        }
        Ok(drawn)
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
