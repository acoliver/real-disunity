use glam::Vec2;

#[derive(Clone, Debug)]
pub(crate) enum Shape {
    Circle { center: Vec2, radius: f32 },
    Polygon(Vec<Vec2>),
}
impl Shape {
    pub fn center(&self) -> Vec2 {
        match self {
            Self::Circle { center, .. } => *center,
            Self::Polygon(p) => p.iter().copied().sum::<Vec2>() / p.len() as f32,
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
                out.push((p[(i + 1) % p.len()] - p[i]).perp().normalize());
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
            axes.push((*b - *a).try_normalize().unwrap_or(Vec2::X))
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
fn ray_circle(origin: Vec2, velocity: Vec2, center: Vec2, radius: f32, max: f32) -> Option<f32> {
    let p = origin - center;
    let a = velocity.length_squared();
    if a < 1e-16 {
        return None;
    }
    let b = p.dot(velocity);
    let c = p.length_squared() - radius * radius;
    let d = b * b - a * c;
    if d < 0.0 {
        return None;
    }
    let t = (-b - d.sqrt()) / a;
    if t >= -1e-6 && t <= max {
        Some(t.max(0.0))
    } else {
        None
    }
}
fn circle_polygon(
    center: Vec2,
    radius: f32,
    poly: &[Vec2],
    v: Vec2,
    max: f32,
) -> Option<(f32, Vec2)> {
    let centroid = poly.iter().copied().sum::<Vec2>() / poly.len() as f32;
    let mut hit: Option<(f32, Vec2)> = None;
    let mut record = |t: f32, n: Vec2| {
        if t >= 0.0 && t <= max && v.dot(n) > 1e-7 && hit.is_none_or(|h| t < h.0) {
            hit = Some((t, n));
        }
    };
    for i in 0..poly.len() {
        let a = poly[i];
        let b = poly[(i + 1) % poly.len()];
        let edge = b - a;
        let mut outward = edge.perp().normalize();
        if outward.dot(centroid - a) > 0.0 {
            outward = -outward;
        }
        let speed = v.dot(outward);
        if speed < -1e-8 {
            let t = (radius - (center - a).dot(outward)) / speed;
            let p = center + v * t - outward * radius;
            let along = (p - a).dot(edge) / edge.length_squared();
            if (0.0..=1.0).contains(&along) {
                record(t, -outward);
            }
        }
        if let Some(t) = ray_circle(center, v, a, radius, max) {
            let n = (a - (center + v * t)).normalize_or_zero();
            record(t, n);
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
