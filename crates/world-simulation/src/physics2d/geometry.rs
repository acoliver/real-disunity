use super::PhysicsError;
use glam::Vec2;

#[derive(Clone, Debug)]
pub(crate) enum Shape {
    Circle { center: Vec2, radius: f32 },
    Polygon(Vec<Vec2>),
}
impl Shape {
    pub(super) fn validate(&self) -> Result<(), PhysicsError> {
        match self {
            Self::Circle { center, radius } => {
                if !center.length_squared().is_finite() || !(radius * radius).is_finite() {
                    return Err(PhysicsError::NumericOverflow);
                }
                if *radius <= 0.0 || radius * radius == 0.0 {
                    return Err(PhysicsError::InvalidShape);
                }
            }
            Self::Polygon(points) => {
                if points.iter().any(|p| !p.length_squared().is_finite()) {
                    return Err(PhysicsError::NumericOverflow);
                }
                let mut sign = 0.0;
                for i in 0..points.len() {
                    let edge = points[(i + 1) % points.len()] - points[i];
                    let squared = edge.length_squared();
                    if !squared.is_finite() {
                        return Err(PhysicsError::NumericOverflow);
                    }
                    if squared == 0.0 || edge.try_normalize().is_none() {
                        return Err(PhysicsError::InvalidShape);
                    }
                    let next = points[(i + 2) % points.len()] - points[(i + 1) % points.len()];
                    let cross = f64::from(edge.x) * f64::from(next.y)
                        - f64::from(edge.y) * f64::from(next.x);
                    if cross == 0.0 || (sign != 0.0 && cross * sign <= 0.0) {
                        return Err(PhysicsError::InvalidShape);
                    }
                    sign = cross.signum();
                }
            }
        }
        Ok(())
    }
    pub fn center(&self) -> Vec2 {
        match self {
            Self::Circle { center, .. } => *center,
            Self::Polygon(p) => {
                p.iter().copied().sum::<Vec2>()
                    / f32::from(u16::try_from(p.len()).expect("bounded polygon"))
            }
        }
    }
    pub fn translated(&self, v: Vec2) -> Self {
        match self {
            Self::Circle { center, radius } => Self::Circle {
                center: *center + v,
                radius: *radius,
            },
            Self::Polygon(p) => Self::Polygon(p.iter().map(|p| *p + v).collect()),
        }
    }
    fn project(&self, n: Vec2) -> (f32, f32) {
        match self {
            Self::Circle { center, radius } => {
                let d = center.dot(n);
                (d - radius, d + radius)
            }
            Self::Polygon(p) => p
                .iter()
                .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), p| {
                    let d = p.dot(n);
                    (lo.min(d), hi.max(d))
                }),
        }
    }
    fn axes(&self, out: &mut Vec<Vec2>) {
        if let Self::Polygon(p) = self {
            for i in 0..p.len() {
                out.push(
                    (p[(i + 1) % p.len()] - p[i])
                        .perp()
                        .try_normalize()
                        .expect("validated polygon edge"),
                );
            }
        }
    }
}
// Separating-axis contact normal points from a toward b.
pub(crate) fn overlap(a: &Shape, b: &Shape) -> Option<(Vec2, f32)> {
    let mut axes = Vec::new();
    a.axes(&mut axes);
    b.axes(&mut axes);
    match (a, b) {
        (Shape::Circle { center: a, .. }, Shape::Circle { center: b, .. }) => {
            axes.push((*b - *a).try_normalize().unwrap_or(Vec2::X));
        }
        (Shape::Circle { center, .. }, Shape::Polygon(p))
        | (Shape::Polygon(p), Shape::Circle { center, .. }) => {
            let v = p
                .iter()
                .min_by(|a, b| {
                    a.distance_squared(*center)
                        .total_cmp(&b.distance_squared(*center))
                })
                .unwrap();
            axes.push((*v - *center).try_normalize().unwrap_or(Vec2::X));
        }
        _ => {}
    }
    let mut depth = f32::INFINITY;
    let mut normal = Vec2::X;
    for mut n in axes {
        let (alo, ahi) = a.project(n);
        let (blo, bhi) = b.project(n);
        let d = (ahi - blo).min(bhi - alo);
        if d < -1e-5 {
            return None;
        }
        if d < depth {
            depth = d;
            if (b.center() - a.center()).dot(n) < 0.0 {
                n = -n;
            }
            normal = n;
        }
    }
    Some((normal, depth.max(0.0)))
}
pub(super) fn separation(a: &Shape, b: &Shape) -> (Vec2, f32) {
    let mut axes = Vec::new();
    a.axes(&mut axes);
    b.axes(&mut axes);
    match (a, b) {
        (Shape::Circle { center: a, .. }, Shape::Circle { center: b, .. }) => {
            if let Some(n) = (*b - *a).try_normalize() {
                axes.push(n);
            } else {
                axes.push(Vec2::X);
            }
        }
        (Shape::Circle { center, .. }, Shape::Polygon(p))
        | (Shape::Polygon(p), Shape::Circle { center, .. }) => {
            let closest = p
                .iter()
                .min_by(|a, b| {
                    a.distance_squared(*center)
                        .total_cmp(&b.distance_squared(*center))
                })
                .expect("nonempty polygon");
            if let Some(n) = (*closest - *center).try_normalize() {
                axes.push(n);
            }
        }
        _ => {}
    }
    let mut result = (Vec2::ZERO, f32::NEG_INFINITY);
    for n in axes {
        let (alo, ahi) = a.project(n);
        let (blo, bhi) = b.project(n);
        for (normal, gap) in [(n, blo - ahi), (-n, alo - bhi)] {
            if gap > result.1 {
                result = (normal, gap);
            }
        }
    }
    result
}
#[expect(
    clippy::cast_possible_truncation,
    reason = "the root is bounded by the finite f32 query horizon before conversion"
)]
fn ray_circle(origin: Vec2, velocity: Vec2, center: Vec2, radius: f32, max: f32) -> Option<f32> {
    // f64 contains every product of finite f32 inputs. The cross-product form
    // avoids cancellation between large, nearly equal head-on/grazing terms.
    let x = f64::from(origin.x) - f64::from(center.x);
    let y = f64::from(origin.y) - f64::from(center.y);
    let vx = f64::from(velocity.x);
    let vy = f64::from(velocity.y);
    let quadratic = vx * vx + vy * vy;
    if quadratic == 0.0 {
        return None;
    }
    let linear = x * vx + y * vy;
    let radius = f64::from(radius);
    let constant = x * x + y * y - radius * radius;
    let cross = x * vy - y * vx;
    let discriminant = quadratic * radius * radius - cross * cross;
    if discriminant < 0.0 {
        return None;
    }
    let root = discriminant.sqrt();
    let q = -linear - root.copysign(linear);
    let time = if q == 0.0 {
        0.0
    } else {
        (q / quadratic).min(constant / q)
    };
    if time >= 0.0 && time <= f64::from(max) {
        Some(time as f32)
    } else {
        None
    }
}
fn circle_polygon(
    center: Vec2,
    radius: f32,
    poly: &[Vec2],
    velocity: Vec2,
    max: f32,
) -> Option<(f32, Vec2)> {
    let centroid = poly.iter().copied().sum::<Vec2>()
        / f32::from(u16::try_from(poly.len()).expect("bounded polygon"));
    let mut hit: Option<(f32, Vec2)> = None;
    let mut record = |time: f32, normal: Vec2| {
        if time >= 0.0
            && time <= max
            && velocity.dot(normal) > 1e-7
            && hit.is_none_or(|h| time < h.0)
        {
            hit = Some((time, normal));
        }
    };
    for i in 0..poly.len() {
        let quadratic = poly[i];
        let linear = poly[(i + 1) % poly.len()];
        let edge = linear - quadratic;
        let mut outward = edge.perp().normalize();
        if outward.dot(centroid - quadratic) > 0.0 {
            outward = -outward;
        }
        let speed = velocity.dot(outward);
        if speed < -1e-8 {
            let time = (radius - (center - quadratic).dot(outward)) / speed;
            let offset = center + velocity * time - outward * radius;
            let along = (offset - quadratic).dot(edge) / edge.length_squared();
            if (0.0..=1.0).contains(&along) {
                record(time, -outward);
            }
        }
        if let Some(time) = ray_circle(center, velocity, quadratic, radius, max) {
            let normal = (quadratic - (center + velocity * time)).normalize_or_zero();
            record(time, normal);
        }
    }
    hit
}
pub(crate) fn sweep(
    a: &Shape,
    b: &Shape,
    relative_velocity: Vec2,
    max: f32,
) -> Option<(f32, Vec2)> {
    if let Some((n, depth)) = overlap(a, b) {
        if depth > 1e-5 || relative_velocity.dot(n) > 1e-7 {
            return Some((0.0, n));
        }
        return None;
    }
    match (a, b) {
        (
            Shape::Circle {
                center: a,
                radius: ra,
            },
            Shape::Circle {
                center: b,
                radius: rb,
            },
        ) => ray_circle(*a, relative_velocity, *b, ra + rb, max)
            .map(|t| (t, (*b - (*a + relative_velocity * t)).normalize_or_zero())),
        (Shape::Circle { center, radius }, Shape::Polygon(p)) => {
            circle_polygon(*center, *radius, p, relative_velocity, max)
        }
        (Shape::Polygon(p), Shape::Circle { center, radius }) => {
            circle_polygon(*center, *radius, p, -relative_velocity, max).map(|(t, n)| (t, -n))
        }
        (Shape::Polygon(_), Shape::Polygon(_)) => polygon_sweep(a, b, relative_velocity, max),
    }
}
pub(crate) fn raycast(
    shape: &Shape,
    origin: Vec2,
    direction: Vec2,
    distance: f32,
) -> Option<(f32, Vec2)> {
    let point = Shape::Circle {
        center: origin,
        radius: 0.0,
    };
    if overlap(&point, shape).is_some() {
        return Some((0.0, -direction));
    }
    sweep(&point, shape, direction, distance).map(|(t, n)| (t, -n))
}

fn polygon_sweep(a: &Shape, b: &Shape, relative_velocity: Vec2, max: f32) -> Option<(f32, Vec2)> {
    let mut axes = Vec::new();
    a.axes(&mut axes);
    b.axes(&mut axes);
    let mut enter = 0.0_f32;
    let mut exit = max;
    let mut normal = Vec2::ZERO;
    for n in axes {
        let (alo, ahi) = a.project(n);
        let (blo, bhi) = b.project(n);
        let speed = relative_velocity.dot(n);
        if speed.abs() < 1e-8 {
            if ahi < blo || bhi < alo {
                return None;
            }
            continue;
        }
        let mut t0 = (blo - ahi) / speed;
        let mut t1 = (bhi - alo) / speed;
        let sign = if speed > 0.0 { 1.0 } else { -1.0 };
        if t0 > t1 {
            std::mem::swap(&mut t0, &mut t1);
        }
        if t0 > enter {
            enter = t0;
            normal = n * sign;
        }
        exit = exit.min(t1);
        if enter > exit {
            return None;
        }
    }
    if enter >= 0.0 && enter <= max && normal != Vec2::ZERO {
        Some((enter, normal))
    } else {
        None
    }
}

pub(crate) fn area_moment(shape: &Shape, origin: Vec2) -> (f32, f32) {
    match shape {
        Shape::Circle { center, radius } => {
            let area = std::f32::consts::PI * radius * radius;
            (
                area,
                area * (radius * radius * 0.5 + center.distance_squared(origin)),
            )
        }
        Shape::Polygon(points) => {
            let (mut area, mut moment) = (0.0, 0.0);
            for i in 0..points.len() {
                let a = points[i] - origin;
                let b = points[(i + 1) % points.len()] - origin;
                let cross = a.perp_dot(b);
                area += cross * 0.5;
                moment += cross * (a.dot(a) + a.dot(b) + b.dot(b)) / 12.0;
            }
            (area.abs(), moment.abs())
        }
    }
}
fn support_feature(shape: &Shape, n: Vec2) -> (f32, f32, f32) {
    let tangent = n.perp();
    match shape {
        Shape::Circle { center, radius } => (
            center.dot(n) + radius,
            center.dot(tangent),
            center.dot(tangent),
        ),
        Shape::Polygon(points) => {
            let plane = points
                .iter()
                .map(|p| p.dot(n))
                .fold(f32::NEG_INFINITY, f32::max);
            let (lo, hi) = points
                .iter()
                .filter(|p| (p.dot(n) - plane).abs() < 1e-4)
                .map(|p| p.dot(tangent))
                .fold((f32::INFINITY, f32::NEG_INFINITY), |(a, b), t| {
                    (a.min(t), b.max(t))
                });
            (plane, lo, hi)
        }
    }
}
pub(crate) fn contact_point(a: &Shape, b: &Shape, n: Vec2) -> Vec2 {
    let (ap, alo, ahi) = support_feature(a, n);
    let (bp, blo, bhi) = support_feature(b, -n);
    let lo = alo.max(-bhi);
    let hi = ahi.min(-blo);
    n * ((ap - bp) * 0.5) + n.perp() * ((lo + hi) * 0.5)
}
