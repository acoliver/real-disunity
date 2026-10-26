use super::{
    BodyType, Collider, ColliderGeometry, PhysicsError, Shape2d,
    geometry::{self, Shape},
};
use crate::{ComponentDesc, ComponentHandle, ComponentSnapshot, ObjectHandle, World, WorldError};
use glam::Vec2;
use std::borrow::Cow;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QueryFilter {
    pub layers: u32,
    pub include_triggers: bool,
}
impl Default for QueryFilter {
    fn default() -> Self {
        Self {
            layers: u32::MAX,
            include_triggers: true,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct RayHit {
    pub collider: ComponentHandle,
    pub object: ObjectHandle,
    pub body: Option<ComponentHandle>,
    pub point: Vec2,
    pub normal: Vec2,
    pub distance: f32,
}
impl World {
    pub fn overlap_point(
        &self,
        point: Vec2,
        filter: QueryFilter,
    ) -> Result<Vec<ComponentHandle>, WorldError> {
        if !point.length_squared().is_finite() {
            return Err(PhysicsError::InvalidQuery.into());
        }
        let point = Shape::Circle {
            center: point,
            radius: 0.0,
        };
        let mut hits = Vec::new();
        for c in self.physics_colliders()? {
            if allowed(&c, filter) && geometry::overlap(&point, &c.shape).is_some() {
                hits.push(c.id);
            }
        }
        hits.sort();
        hits.dedup();
        Ok(hits)
    }
    pub fn overlap_shape(
        &self,
        shape: &super::Shape2d,
        position: Vec2,
        filter: QueryFilter,
    ) -> Result<Vec<ComponentHandle>, WorldError> {
        shape.validate()?;
        if !position.is_finite() {
            return Err(PhysicsError::InvalidQuery.into());
        }
        let shape = shape.local_shape().translated(position);
        shape.validate()?;
        let mut hits = Vec::new();
        for c in self.physics_colliders()? {
            if allowed(&c, filter) && geometry::overlap(&shape, &c.shape).is_some() {
                hits.push(c.id);
            }
        }
        hits.sort();
        hits.dedup();
        Ok(hits)
    }
    pub fn raycast(
        &self,
        origin: Vec2,
        direction: Vec2,
        distance: f32,
        filter: QueryFilter,
    ) -> Result<Vec<RayHit>, WorldError> {
        if !origin.is_finite() || !direction.is_finite() || !distance.is_finite() || distance < 0.0
        {
            return Err(PhysicsError::InvalidQuery.into());
        }
        let direction = direction
            .try_normalize()
            .ok_or(PhysicsError::InvalidQuery)?;
        let mut hits = Vec::new();
        for c in self.physics_colliders()? {
            if allowed(&c, filter)
                && let Some((t, n)) = geometry::raycast(&c.shape, origin, direction, distance)
            {
                if !t.is_finite()
                    || !n.is_finite()
                    || n.length_squared() == 0.0
                    || !(origin + direction * t).is_finite()
                {
                    return Err(PhysicsError::NumericOverflow.into());
                }
                hits.push(RayHit {
                    collider: c.id,
                    object: c.object,
                    body: c.body,
                    point: origin + direction * t,
                    normal: n,
                    distance: t,
                });
            }
        }
        hits.sort_by(|a, b| {
            a.distance
                .total_cmp(&b.distance)
                .then_with(|| a.collider.cmp(&b.collider))
        });
        let mut seen = std::collections::BTreeSet::new();
        hits.retain(|h| seen.insert(h.collider));
        Ok(hits)
    }
    pub(super) fn attached_body(
        &self,
        object: ObjectHandle,
    ) -> Result<Option<&ComponentSnapshot>, WorldError> {
        let mut current = Some(object);
        while let Some(o) = current {
            if let Some((_, body)) = self.components.iter(self.identity).find(|(_, c)| {
                c.object == o
                    && !self.lifetime.components.contains(&c.handle)
                    && matches!(c.desc, ComponentDesc::Body2d(_))
            }) {
                return Ok(Some(body));
            }
            current = self.parent(o)?;
        }
        Ok(None)
    }
    /// A collider's own shapes, or those built from the `Tilemap` on its object.
    fn collider_shapes<'a>(
        &'a self,
        object: ObjectHandle,
        geometry: &'a ColliderGeometry,
    ) -> Result<Cow<'a, [Shape2d]>, WorldError> {
        if let ColliderGeometry::Shapes(shapes) = geometry {
            return Ok(Cow::Borrowed(shapes));
        }
        let tilemap = self
            .components
            .iter(self.identity)
            .find_map(|(_, c)| match &c.desc {
                ComponentDesc::Tilemap(map)
                    if c.object == object && !self.lifetime.components.contains(&c.handle) =>
                {
                    Some(map)
                }
                _ => None,
            });
        Ok(match tilemap {
            Some(map) => Cow::Owned(map.collision_shapes()?),
            None => Cow::Borrowed(&[]),
        })
    }
    pub(super) fn physics_colliders(&self) -> Result<Vec<Collider>, WorldError> {
        let mut out = Vec::new();
        for (_, c) in self.components.iter(self.identity) {
            let ComponentDesc::Collider2d(desc) = &c.desc else {
                continue;
            };
            if self.lifetime.components.contains(&c.handle)
                || !desc.enabled
                || !self.active_in_hierarchy(c.object)?
            {
                continue;
            }
            let body = self.attached_body(c.object)?;
            let kind = match body.map(|c| &c.desc) {
                Some(ComponentDesc::Body2d(b)) if !b.enabled => continue,
                Some(ComponentDesc::Body2d(b)) => b.body_type,
                _ => BodyType::Static,
            };
            let plane = self.physics.settings.plane;
            let matrix = self.world_matrix(c.object)?;
            let layer = self.object(c.object)?.layer;
            for shape in self.collider_shapes(c.object, &desc.geometry)?.iter() {
                if out.len() >= 8192 {
                    return Err(PhysicsError::InvalidShape.into());
                }
                let shape = world_shape(shape, matrix, plane, desc.offset)?;
                out.push(Collider {
                    id: c.handle,
                    object: c.object,
                    owner: body.map_or(c.object, |b| b.object),
                    body: body.map(|b| b.handle),
                    kind,
                    shape,
                    trigger: desc.trigger,
                    layer,
                    mask: desc.layers
                        & self
                            .physics
                            .masks
                            .as_ref()
                            .map_or(u32::MAX, |m| m[layer as usize]),
                    material: self.collider_material(c.handle)?,
                    material_backed: desc.shared_material.is_some()
                        || body.is_some_and(|c| {
                            matches!(&c.desc,
                            ComponentDesc::Body2d(b) if b.shared_material.is_some())
                        }),
                });
            }
        }
        Ok(out)
    }
}
fn allowed(c: &Collider, f: QueryFilter) -> bool {
    f.layers & (1 << c.layer) != 0 && (f.include_triggers || !c.trigger)
}

fn world_shape(
    shape: &super::Shape2d,
    matrix: glam::Mat4,
    plane: super::PhysicsPlane,
    offset: Vec2,
) -> Result<Shape, WorldError> {
    let x = matrix.transform_vector3(plane.vector(Vec2::X));
    let y = matrix.transform_vector3(plane.vector(Vec2::Y));
    if (x - plane.vector(plane.project(x))).length() > 1e-4
        || (y - plane.vector(plane.project(y))).length() > 1e-4
        || plane.project(x).perp_dot(plane.project(y)).abs() < 1e-8
    {
        return Err(PhysicsError::InvalidTransform.into());
    }
    let map = |p: Vec2| plane.project(matrix.transform_point3(plane.vector(p + offset)));
    let shape = match shape.local_shape() {
        Shape::Circle { center, radius } => {
            if (x.length() - y.length()).abs() > 1e-4 || x.dot(y).abs() > 1e-4 {
                return Err(PhysicsError::InvalidTransform.into());
            }
            Shape::Circle {
                center: map(center),
                radius: radius * x.length(),
            }
        }
        Shape::Polygon(p) => Shape::Polygon(p.into_iter().map(map).collect()),
    };
    shape.validate()?;
    Ok(shape)
}
