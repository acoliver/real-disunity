use super::*;
use crate::components::Body;
use glam::Quat;

impl DisunityWorld {
    pub(super) fn collect_motions(&self, dt: f32) -> Result<Motions, String> {
        let mut motions = BTreeMap::new();
        let colliders = self.colliders()?;
        for (i, c) in self.components.iter().enumerate() {
            let Some(c) = c else { continue };
            let Fields::Body(b) = &c.fields else { continue };
            if !c.enabled
                || !b.simulated
                || !self.active(c.object)
                || b.body_type == BodyType::Static
            {
                continue;
            }
            let p = self.world_trs(c.object)?.0.truncate();
            let mut velocity = linear_velocity(b, dt, Vec2::from(self.physics.gravity));
            if let Some(target) = b.move_position
                && dt > 0.0
            {
                velocity = (Vec2::from(target) - p) / dt;
            }
            let free = Vec2::new(
                f32::from(b.constraints & 1 == 0),
                f32::from(b.constraints & 2 == 0),
            );
            let inverse_mass = if b.body_type == BodyType::Dynamic {
                free / b.mass
            } else {
                Vec2::ZERO
            };
            let inertia = inertia(&colliders, c.object, p, b.mass);
            let rotating = b.constraints & 4 == 0;
            motions.insert(
                c.object,
                Motion {
                    object: c.object,
                    component: i,
                    velocity: velocity * free,
                    angular: if rotating && !b.sleeping {
                        angular_velocity(b, dt)
                    } else {
                        0.0
                    },
                    inverse_mass,
                    inverse_inertia: if rotating && b.body_type == BodyType::Dynamic {
                        1.0 / inertia
                    } else {
                        0.0
                    },
                    sleeping: b.sleeping,
                },
            );
        }
        Ok(motions)
    }
    pub(super) fn advance(&mut self, motions: &mut Motions, dt: f32) -> Result<(), String> {
        let mut poses = Vec::new();
        for m in motions.values() {
            let (p, r, _) = self.world_trs(m.object)?;
            let mut depth = 0;
            let mut parent = self.metadata[&m.object].parent;
            while let Some(h) = parent {
                depth += 1;
                parent = self.metadata[&h].parent;
            }
            poses.push((
                depth,
                m.object,
                p + m.velocity.extend(0.0) * dt,
                Quat::from_rotation_z(m.angular * dt) * r,
            ));
        }
        poses.sort_by_key(|v| v.0);
        for (_, h, p, r) in poses {
            self.set_trs(h, Some(p.to_array()), Some(r.to_array()), None, "world")?;
        }
        Ok(())
    }
    pub(super) fn finish_motion(&mut self, motions: &Motions, dt: f32) {
        for m in motions.values() {
            if let Fields::Body(b) = &mut self.components[m.component].as_mut().unwrap().fields {
                b.sleeping = m.sleeping;
                b.velocity = if b.move_position.is_some() && dt > 0.0 {
                    [0.0; 2]
                } else {
                    m.velocity.to_array()
                };
                b.angular_velocity = m.angular.to_degrees();
                if dt > 0.0 {
                    b.move_position = None;
                    b.force = [0.0; 2];
                }
            }
        }
    }
}
fn linear_velocity(b: &Body, dt: f32, gravity: Vec2) -> Vec2 {
    if b.sleeping {
        return Vec2::ZERO;
    }
    let velocity = Vec2::from(b.velocity);
    if b.body_type == BodyType::Dynamic && b.move_position.is_none() {
        (velocity + (gravity * b.gravity_scale + Vec2::from(b.force) / b.mass) * dt)
            / (1.0 + b.drag * dt)
    } else {
        velocity
    }
}
fn angular_velocity(b: &Body, dt: f32) -> f32 {
    let damping = if b.body_type == BodyType::Dynamic {
        1.0 + b.angular_drag * dt
    } else {
        1.0
    };
    b.angular_velocity.to_radians() / damping
}
fn inertia(colliders: &[Collider], owner: u64, position: Vec2, mass: f32) -> f32 {
    let (area, moment) = colliders
        .iter()
        .filter(|c| c.owner == owner && !c.trigger)
        .map(|c| geometry::area_moment(&c.shape, position))
        .fold((0.0, 0.0), |(a, m), (b, n)| (a + b, m + n));
    if area > 1e-10 {
        (mass * moment / area).max(1e-10)
    } else {
        mass
    }
}
pub(super) fn angular_horizon(motions: &Motions, remaining: f32) -> f32 {
    let omega = motions
        .values()
        .map(|m| m.angular.abs())
        .fold(0.0_f32, f32::max);
    if omega > 0.0 {
        remaining.min(1.0_f32.to_radians() / omega)
    } else {
        remaining
    }
}
