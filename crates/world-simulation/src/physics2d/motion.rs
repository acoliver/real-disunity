use super::{
    BodyType, Collider, Motion, Motions, PhysicsError,
    geometry::{self, Shape},
};
use crate::{ComponentDesc, LocalTransform, ObjectHandle, World, WorldError};
use glam::{Mat4, Vec2};
use std::collections::BTreeMap;

impl World {
    pub(super) fn validate_static_hierarchies(
        &self,
        colliders: &[Collider],
        motions: &Motions,
    ) -> Result<(), WorldError> {
        for collider in colliders.iter().filter(|c| c.kind == BodyType::Static) {
            let mut parent = self.parent(collider.owner)?;
            while let Some(object) = parent {
                if motions.contains_key(&object) {
                    return Err(PhysicsError::UnsupportedHierarchy.into());
                }
                parent = self.parent(object)?;
            }
        }
        Ok(())
    }
    pub(super) fn collect_motions(
        &self,
        dt: f32,
        colliders: &[Collider],
    ) -> Result<Motions, WorldError> {
        let mut motions = Motions::new();
        for (_, c) in self.components.iter(self.identity) {
            let ComponentDesc::Body2d(b) = &c.desc else {
                continue;
            };
            if self.lifetime.components.contains(&c.handle)
                || !b.enabled
                || b.body_type == BodyType::Static
                || !self.active_in_hierarchy(c.object)?
            {
                continue;
            }
            let start = self
                .physics
                .settings
                .plane
                .project(self.world_matrix(c.object)?.w_axis.truncate());
            let velocity = self.body_velocity(c.handle, b, start, dt);
            let (area, moment) = colliders
                .iter()
                .filter(|a| a.owner == c.object && !a.trigger)
                .map(|a| geometry::area_moment(&a.shape, start))
                .fold((0.0, 0.0), |(a, m), (b, n)| (a + b, m + n));
            let inertia = if area > 1e-10 {
                (b.mass * moment / area).max(1e-10)
            } else {
                b.mass
            };
            let dynamic = b.body_type == BodyType::Dynamic;
            let angular = if b.freeze_rotation || b.sleeping {
                0.0
            } else {
                b.angular_velocity
            };
            if !velocity.length_squared().is_finite() || !inertia.is_finite() {
                return Err(PhysicsError::NumericOverflow.into());
            }
            motions.insert(
                c.object,
                Motion {
                    component: c.handle,
                    sleeping: b.sleeping,
                    start,
                    position: start,
                    angle: 0.0,
                    velocity,
                    angular,
                    inverse_mass: if dynamic { 1.0 / b.mass } else { 0.0 },
                    inverse_inertia: if dynamic && !b.freeze_rotation {
                        1.0 / inertia
                    } else {
                        0.0
                    },
                },
            );
        }
        Ok(motions)
    }
    pub(super) fn commit_motions(&mut self, motions: &Motions, dt: f32) -> Result<(), WorldError> {
        let mut locals = BTreeMap::new();
        let mut matrices = BTreeMap::new();
        for &object in motions.keys() {
            self.proposed_matrix(object, motions, &mut locals, &mut matrices)?;
        }
        for m in motions.values() {
            if !m.velocity.is_finite() || !m.angular.is_finite() {
                return Err(PhysicsError::NumericOverflow.into());
            }
        }
        for (object, local) in locals {
            self.set_local_transform(object, local)?;
        }
        for m in motions.values() {
            let ComponentDesc::Body2d(mut body) = self.component(m.component)?.desc.clone() else {
                unreachable!()
            };
            body.sleeping = m.sleeping;
            body.velocity = m.velocity;
            body.angular_velocity = m.angular;
            self.set_component(m.component, ComponentDesc::Body2d(body))?;
            if dt > 0.0 {
                self.physics.targets.remove(&m.component);
            }
        }
        Ok(())
    }
    fn proposed_matrix(
        &self,
        object: ObjectHandle,
        motions: &Motions,
        locals: &mut BTreeMap<ObjectHandle, LocalTransform>,
        matrices: &mut BTreeMap<ObjectHandle, Mat4>,
    ) -> Result<Mat4, WorldError> {
        let mut chain = Vec::new();
        let mut current = Some(object);
        while let Some(node) = current {
            if matrices.contains_key(&node) {
                break;
            }
            chain.push(node);
            current = self.parent(node)?;
        }
        for node in chain.into_iter().rev() {
            let parent = self.parent(node)?;
            let parent_matrix = parent.map_or(Mat4::IDENTITY, |p| matrices[&p]);
            let local = self.proposed_local(node, parent_matrix, motions.get(&node))?;
            if motions.contains_key(&node) {
                locals.insert(node, local);
            }
            matrices.insert(node, parent_matrix * local.matrix());
        }
        Ok(matrices[&object])
    }
    fn proposed_local(
        &self,
        object: ObjectHandle,
        parent_matrix: Mat4,
        motion: Option<&Motion>,
    ) -> Result<LocalTransform, WorldError> {
        let parent = self.parent(object)?;
        let mut local = self.local_transform(object)?;
        if let Some(m) = motion {
            if parent_matrix.determinant().abs() < 1e-12 {
                return Err(WorldError::SingularParent);
            }
            let plane = self.physics.settings.plane;
            let original = self.world_matrix(object)?;
            let position = original.w_axis.truncate() + plane.vector(m.position - m.start);
            local.translation = parent_matrix.inverse().transform_point3(position);
            let previous_parent = parent
                .map(|p| self.world_matrix(p))
                .transpose()?
                .unwrap_or(Mat4::IDENTITY);
            let parent_rotated = parent_matrix.x_axis != previous_parent.x_axis
                || parent_matrix.y_axis != previous_parent.y_axis
                || parent_matrix.z_axis != previous_parent.z_axis;
            if m.angle != 0.0 || parent_rotated {
                let delta = Mat4::from_quat(plane.rotation(m.angle));
                let mut desired = delta * original;
                desired.w_axis = position.extend(1.0);
                local =
                    crate::transform::decompose(parent_matrix.inverse() * desired, local.rotation)?;
            }
            local.validate()?;
        }
        Ok(local)
    }
}
pub(super) fn advance(motions: &mut Motions, dt: f32) -> Result<(), WorldError> {
    for m in motions.values_mut() {
        if m.sleeping {
            continue;
        }
        m.position += m.velocity * dt;
        m.angle += m.angular * dt;
        if !m.position.length_squared().is_finite() || !m.angle.is_finite() {
            return Err(PhysicsError::NumericOverflow.into());
        }
    }
    Ok(())
}
pub(super) fn moved_colliders(
    base: &[Collider],
    motions: &Motions,
) -> Result<Vec<Collider>, WorldError> {
    base.iter()
        .map(|c| {
            let mut c = c.clone();
            if let Some(m) = motions.get(&c.owner) {
                let (sin, cos) = m.angle.sin_cos();
                let map = |p: Vec2| {
                    let p = p - m.start;
                    Vec2::new(cos * p.x - sin * p.y, sin * p.x + cos * p.y) + m.position
                };
                c.shape = match &c.shape {
                    Shape::Circle { center, radius } => Shape::Circle {
                        center: map(*center),
                        radius: *radius,
                    },
                    Shape::Polygon(p) => Shape::Polygon(p.iter().copied().map(map).collect()),
                };
            }
            c.shape.validate()?;
            Ok(c)
        })
        .collect()
}
