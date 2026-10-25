use super::{
    Collider, Motion, Motions, PhysicsError,
    geometry::{self, Shape},
};
use glam::Vec2;

pub(super) fn pose(
    collider: &Collider,
    motions: &Motions,
    time: f32,
) -> Result<Shape, PhysicsError> {
    let Some(m) = motions.get(&collider.owner) else {
        return Ok(collider.shape.clone());
    };
    let (sin, cos) = (m.angular * time).sin_cos();
    let map = |p: Vec2| {
        let r = p - m.position;
        Vec2::new(cos * r.x - sin * r.y, sin * r.x + cos * r.y) + m.position + m.velocity * time
    };
    let shape = match &collider.shape {
        Shape::Circle { center, radius } => Shape::Circle {
            center: map(*center),
            radius: *radius,
        },
        Shape::Polygon(points) => Shape::Polygon(points.iter().copied().map(map).collect()),
    };
    shape.validate()?;
    Ok(shape)
}

struct Rate {
    point: Vec2,
    speed: f64,
    acceleration: f64,
    speed_bound: f64,
}

fn rates(shape: &Shape, motion: Option<&Motion>, normal: Vec2, time: f32) -> (Vec<Rate>, f64) {
    let (points, radius): (&[Vec2], f64) = match shape {
        Shape::Circle { center, radius } => (std::slice::from_ref(center), f64::from(*radius)),
        Shape::Polygon(points) => (points, 0.0),
    };
    let values = points
        .iter()
        .map(|&point| {
            let mut rate = Rate {
                point,
                speed: 0.0,
                acceleration: 0.0,
                speed_bound: 0.0,
            };
            if let Some(m) = motion {
                let pivot = m.position + m.velocity * time;
                let rx = f64::from(point.x) - f64::from(pivot.x);
                let ry = f64::from(point.y) - f64::from(pivot.y);
                let omega = f64::from(m.angular);
                rate.speed = (f64::from(m.velocity.x) - ry * omega) * f64::from(normal.x)
                    + (f64::from(m.velocity.y) + rx * omega) * f64::from(normal.y);
                rate.acceleration = rx.hypot(ry) * omega * omega;
                rate.speed_bound = f64::from(m.velocity.x).hypot(f64::from(m.velocity.y))
                    + rx.hypot(ry) * omega.abs();
            }
            rate
        })
        .collect();
    (values, radius)
}

fn closing_time(gap: f64, closing: f64, acceleration: f64) -> f64 {
    if acceleration == 0.0 {
        if closing <= 0.0 {
            return f64::INFINITY;
        }
        return gap / closing;
    }
    let root = (closing * closing + 2.0 * acceleration * gap).sqrt();
    if closing >= 0.0 {
        2.0 * gap / (closing + root)
    } else {
        (root - closing) / acceleration
    }
}

// A separating plane may rotate with either body. Including its derivative
// avoids treating tangential motion along a rotating face as closing motion.
fn plane_interval(
    a: &[Rate],
    b: &[Rate],
    radius: f64,
    normal: Vec2,
    omega: f64,
    horizon: f64,
) -> f64 {
    let mut safe = f64::INFINITY;
    for a in a {
        for b in b {
            let x = f64::from(b.point.x) - f64::from(a.point.x);
            let y = f64::from(b.point.y) - f64::from(a.point.y);
            let gap = x * f64::from(normal.x) + y * f64::from(normal.y) - radius;
            if gap <= 1e-5 {
                return 0.0;
            }
            let closing =
                a.speed - b.speed - omega * (y * f64::from(normal.x) - x * f64::from(normal.y));
            let speed_bound = a.speed_bound + b.speed_bound;
            let acceleration = a.acceleration
                + b.acceleration
                + 2.0 * omega.abs() * speed_bound
                + omega * omega * (x.hypot(y) + speed_bound * horizon);
            safe = safe.min(closing_time(gap - 5e-6, closing, acceleration));
        }
    }
    safe
}

fn safe_interval(
    a: &Shape,
    b: &Shape,
    motions: (Option<&Motion>, Option<&Motion>),
    normal: Vec2,
    time: f32,
    horizon: f32,
) -> f64 {
    let (ma, mb) = motions;
    let (a, ra) = rates(a, ma, normal, time);
    let (b, rb) = rates(b, mb, normal, time);
    [
        0.0,
        ma.map_or(0.0, |m| f64::from(m.angular)),
        mb.map_or(0.0, |m| f64::from(m.angular)),
    ]
    .into_iter()
    .map(|omega| plane_interval(&a, &b, ra + rb, normal, omega, f64::from(horizon)))
    .fold(0.0, f64::max)
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "the conservative interval is bounded by the finite f32 horizon and reduced before conversion"
)]
pub(super) fn sweep(
    a: &Collider,
    b: &Collider,
    motions: &Motions,
    horizon: f32,
    budget: &mut u32,
) -> Result<Option<(f32, Vec2)>, PhysicsError> {
    let ma = motions.get(&a.owner);
    let mb = motions.get(&b.owner);
    let velocity = ma.map_or(Vec2::ZERO, |m| m.velocity) - mb.map_or(Vec2::ZERO, |m| m.velocity);
    if !velocity.length_squared().is_finite() {
        return Err(PhysicsError::NumericOverflow);
    }
    if ma.is_none_or(|m| m.angular == 0.0) && mb.is_none_or(|m| m.angular == 0.0) {
        let hit = geometry::sweep(&a.shape, &b.shape, velocity, horizon);
        if hit.is_some_and(|(t, n)| !t.is_finite() || !n.is_finite() || n.length_squared() == 0.0) {
            return Err(PhysicsError::NumericOverflow);
        }
        return Ok(hit);
    }
    let mut time = 0.0;
    while *budget > 0 {
        *budget -= 1;
        let sa = pose(a, motions, time)?;
        let sb = pose(b, motions, time)?;
        let (normal, gap) = geometry::separation(&sa, &sb);
        if !gap.is_finite() || !normal.is_finite() {
            return Err(PhysicsError::NumericOverflow);
        }
        if gap <= 1e-5 {
            return Ok(Some((time, normal)));
        }
        let safe = safe_interval(&sa, &sb, (ma, mb), normal, time, horizon - time);
        if safe == 0.0 {
            return Ok(Some((time, normal)));
        }
        if safe > f64::from(horizon - time) {
            return Ok(None);
        }
        let next = time + (safe * 0.9) as f32;
        if next <= time {
            return Err(PhysicsError::IterationLimit);
        }
        time = next;
    }
    Err(PhysicsError::IterationLimit)
}
