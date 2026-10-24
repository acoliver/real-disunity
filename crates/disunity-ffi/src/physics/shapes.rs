use super::*;
use crate::components::Component;
impl DisunityWorld {
    pub(super) fn colliders(&self) -> Result<Vec<Collider>, String> {
        let mut out = Vec::new();
        for c in self.components.iter().flatten() {
            if !c.enabled || !self.active(c.object) || !is_collider(&c.fields) {
                continue;
            }
            let body = self.attached_body(c.object);
            if body
                .is_some_and(|c| !c.enabled || matches!(&c.fields, Fields::Body(b) if !b.simulated))
            {
                continue;
            }
            for (shape, trigger) in self.collider_shapes(c)? {
                if out.len() >= 8192 {
                    return Err("world collider shape budget exceeded".into());
                }
                out.push(self.collider(c, body, shape, trigger));
            }
        }
        Ok(out)
    }
    fn collider(
        &self,
        c: &Component,
        body: Option<&Component>,
        shape: Shape,
        trigger: bool,
    ) -> Collider {
        let material = c
            .fields
            .material()
            .or_else(|| body.and_then(|c| c.fields.material()));
        let restitution = body.map_or(1.0, |c| match &c.fields {
            Fields::Body(b) => b.restitution,
            _ => 1.0,
        });
        Collider {
            id: c.handle,
            object: c.object,
            owner: body.map_or(c.object, |c| c.object),
            body: body.map(|c| (c.object, c.handle)),
            body_type: body.map_or(BodyType::Static, |c| match &c.fields {
                Fields::Body(b) => b.body_type,
                _ => unreachable!(),
            }),
            shape,
            trigger,
            layer: self.metadata[&c.object].layer,
            mask: self.physics.layer_masks[self.metadata[&c.object].layer as usize],
            restitution: material.map_or(restitution, |m| m.bounciness),
            friction: material.map_or(0.0, |m| m.friction),
        }
    }
    fn collider_shapes(&self, c: &Component) -> Result<Vec<(Shape, bool)>, String> {
        if let Fields::Tiles(tiles) = &c.fields {
            let (p, r, s) = self.world_trs(c.object)?;
            planar(r)?;
            if s.x.abs() < 1e-8 || s.y.abs() < 1e-8 {
                return Err("tile collider requires nonzero XY world scale".into());
            }
            return Ok(tiles
                .polygons
                .iter()
                .map(|points| {
                    (
                        Shape::Polygon(
                            points
                                .iter()
                                .map(|v| (p + r * (s * Vec3::new(v[0], v[1], 0.0))).truncate())
                                .collect(),
                        ),
                        tiles.is_trigger,
                    )
                })
                .collect());
        }
        Ok(vec![self.collider_shape(c)?])
    }
    fn collider_shape(&self, c: &Component) -> Result<(Shape, bool), String> {
        let (p, r, s) = self.world_trs(c.object)?;
        planar(r)?;
        let map = |v: [f32; 2]| (p + r * (s * Vec3::new(v[0], v[1], 0.0))).truncate();
        let (shape, trigger) = match &c.fields {
            Fields::Circle(c) => {
                if (s.x.abs() - s.y.abs()).abs() > 1e-4 {
                    return Err("circle collider requires uniform XY world scale".into());
                }
                (
                    Shape::Circle {
                        center: map(c.offset),
                        radius: c.radius * s.x.abs(),
                    },
                    c.is_trigger,
                )
            }
            Fields::Box(b) => {
                let half = Vec2::from(b.size) * 0.5;
                let o = Vec2::from(b.offset);
                (
                    Shape::Polygon(
                        [
                            [-half.x, -half.y],
                            [half.x, -half.y],
                            [half.x, half.y],
                            [-half.x, half.y],
                        ]
                        .map(|v| map((Vec2::from(v) + o).to_array()))
                        .to_vec(),
                    ),
                    b.is_trigger,
                )
            }
            Fields::Polygon(b) => (
                Shape::Polygon(
                    b.points
                        .iter()
                        .map(|v| map((Vec2::from(*v) + Vec2::from(b.offset)).to_array()))
                        .collect(),
                ),
                b.is_trigger,
            ),
            _ => unreachable!(),
        };
        Ok((shape, trigger))
    }
}
fn is_collider(fields: &Fields) -> bool {
    matches!(
        fields,
        Fields::Circle(_) | Fields::Box(_) | Fields::Polygon(_) | Fields::Tiles(_)
    )
}
fn planar(r: glam::Quat) -> Result<(), String> {
    let x = r * Vec3::X;
    let y = r * Vec3::Y;
    if x.z.abs() > 1e-4 || y.z.abs() > 1e-4 {
        return Err("2D colliders require planar world rotation".into());
    }
    Ok(())
}
