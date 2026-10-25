use super::{PhysicsError, geometry::Shape};
use glam::Vec2;

#[derive(Clone, Debug, PartialEq)]
pub enum Shape2d {
    Box { half_extents: Vec2 },
    Circle { radius: f32 },
    Polygon(Vec<Vec2>),
}
impl Shape2d {
    pub(crate) fn validate(&self) -> Result<(), PhysicsError> {
        match self {
            Self::Box { half_extents }
                if half_extents.is_finite() && half_extents.min_element() > 0.0 =>
            {
                self.local_shape().validate()
            }
            Self::Circle { radius } if radius.is_finite() && *radius > 0.0 => Ok(()),
            Self::Polygon(p) => validate_polygon(p),
            _ => Err(PhysicsError::InvalidShape),
        }
    }
    pub(super) fn local_shape(&self) -> Shape {
        match self {
            Self::Box { half_extents: h } => Shape::Polygon(vec![
                Vec2::new(-h.x, -h.y),
                Vec2::new(h.x, -h.y),
                *h,
                Vec2::new(-h.x, h.y),
            ]),
            Self::Circle { radius } => Shape::Circle {
                center: Vec2::ZERO,
                radius: *radius,
            },
            Self::Polygon(p) => Shape::Polygon(p.clone()),
        }
    }
}
fn validate_polygon(p: &[Vec2]) -> Result<(), PhysicsError> {
    if p.len() < 3 || p.len() > 128 || p.iter().any(|v| !v.is_finite()) {
        return Err(PhysicsError::InvalidShape);
    }
    let mut sign = 0.0_f32;
    for i in 0..p.len() {
        let edge = p[(i + 1) % p.len()] - p[i];
        if edge.length_squared() < 1e-12 {
            return Err(PhysicsError::InvalidShape);
        }
        for j in 0..p.len() {
            if j == i || j == (i + 1) % p.len() {
                continue;
            }
            let cross = edge.perp_dot(p[j] - p[i]);
            if cross.abs() < 1e-8 {
                return Err(PhysicsError::InvalidShape);
            }
            if sign == 0.0 {
                sign = cross.signum();
            }
            if cross * sign <= 0.0 {
                return Err(PhysicsError::InvalidShape);
            }
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq)]
pub struct Collider2d {
    pub shapes: Vec<Shape2d>,
    pub offset: Vec2,
    pub enabled: bool,
    pub shared_material: Option<super::PhysicsMaterialHandle>,
    pub trigger: bool,
    pub layers: u32,
}
impl Default for Collider2d {
    fn default() -> Self {
        Self {
            shapes: vec![Shape2d::Box {
                half_extents: Vec2::splat(0.5),
            }],
            offset: Vec2::ZERO,
            enabled: true,
            shared_material: None,
            trigger: false,
            layers: u32::MAX,
        }
    }
}
impl Collider2d {
    pub(crate) fn validate(&self) -> Result<(), PhysicsError> {
        if !self.offset.is_finite() || self.shapes.len() > 4096 {
            return Err(PhysicsError::InvalidShape);
        }
        for shape in &self.shapes {
            shape.validate()?;
        }
        Ok(())
    }
}
