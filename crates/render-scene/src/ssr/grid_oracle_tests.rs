use super::*;

impl Ray {
    fn position(self, axis: f64, primary: usize) -> [f64; 3] {
        let t = (axis - self.a[primary]) / (self.b[primary] - self.a[primary]);
        [
            self.a[0] + t * (self.b[0] - self.a[0]),
            self.a[1] + t * (self.b[1] - self.a[1]),
            (self.weighted[0] + t * (self.weighted[1] - self.weighted[0]))
                / (self.reciprocal[0] + t * (self.reciprocal[1] - self.reciprocal[0])),
        ]
    }
    fn end(self) -> f64 {
        let mut end = 1.0f64;
        for (axis, delta) in [self.b[0] - self.a[0], self.b[1] - self.a[1]]
            .into_iter()
            .enumerate()
        {
            if delta != 0.0 {
                let boundary = if delta > 0.0 {
                    f64::from(self.side) / f64::from(1u32 << self.base)
                } else {
                    0.0
                };
                end = end.min((boundary - self.a[axis]) / delta);
            }
        }
        end
    }
}
fn fetch(depths: &[Vec<f64>], level: usize, point: [f64; 3]) -> f64 {
    let width = f64::from(1u32 << level);
    let side = i32::try_from(depths[level].len().isqrt()).unwrap();
    let coordinate = (glam::DVec2::new(point[0], point[1]) / width)
        .as_ivec2()
        .clamp(glam::IVec2::ZERO, glam::IVec2::splat(side - 1));
    depths[level][usize::try_from(coordinate.y * side + coordinate.x).unwrap()]
}
// Parameter-space oracle: reconstruct each endpoint independently in double precision.
pub(super) fn oracle(ray: Ray, depths: &[Vec<f64>]) -> [f64; 8] {
    if ray.base > 0 {
        return refined_oracle(ray, depths);
    }
    let primary = usize::from((ray.b[1] - ray.a[1]).abs() > (ray.b[0] - ray.a[0]).abs());
    let delta = ray.b[primary] - ray.a[primary];
    let direction = delta.signum();
    let offset = f64::from(u8::from(direction > 0.0));
    let stop = ((ray.a[primary] + ray.end() * delta) * direction).floor() + offset;
    let mut axis = (ray.a[primary] + ray.skip * direction).floor() + offset;
    let (mut level, mut levels, mut steps) = (0usize, 0u32, 0u32);
    let mut allowance = ray.behind;
    while axis * direction < stop && steps < ray.budget {
        steps += 1;
        levels = (levels * 5 + u32::try_from(level).unwrap() + 1) % 1021;
        let width = f64::from(1u32 << level);
        let next = ((((axis + direction * 0.5) / width).floor() + offset) * width * direction)
            .min(stop)
            * direction;
        let mut start = ray.position(axis, primary);
        let mut finish = ray.position(next, primary);
        start[primary] += direction * 0.001;
        finish[primary] -= direction * 0.001;
        let surface = fetch(depths, level, start).min(fetch(depths, level, finish));
        if surface < start[2].max(finish[2]) {
            if level > 0 {
                level -= 1;
                continue;
            }
            if start[2].min(finish[2]) - 0.25 <= surface {
                return oracle_hit(
                    ray,
                    ray.position(axis + direction * 0.5, primary),
                    surface,
                    levels,
                    steps,
                );
            }
            if allowance == 0 {
                break;
            }
            allowance -= 1;
        } else {
            level = (level + 1).min(usize::try_from(ray.levels - 1).unwrap());
        }
        axis = next;
    }
    oracle_miss(levels, steps)
}

fn oracle_hit(ray: Ray, hit: [f64; 3], surface: f64, levels: u32, steps: u32) -> [f64; 8] {
    let scale = f64::from(1u32 << ray.base);
    if hit[..2]
        .iter()
        .any(|&v| v < 0.0 || v * scale >= f64::from(ray.side))
    {
        return oracle_miss(levels, steps);
    }
    [
        hit[0],
        hit[1],
        surface,
        f64::from(levels),
        f64::from(steps),
        1.0,
        (hit[0] * scale).trunc(),
        (hit[1] * scale).trunc(),
    ]
}

fn oracle_miss(levels: u32, steps: u32) -> [f64; 8] {
    [
        0.0,
        0.0,
        -1.0,
        f64::from(levels),
        f64::from(steps),
        0.0,
        0.0,
        0.0,
    ]
}

fn refined_oracle(ray: Ray, depths: &[Vec<f64>]) -> [f64; 8] {
    let primary = usize::from((ray.b[1] - ray.a[1]).abs() > (ray.b[0] - ray.a[0]).abs());
    let delta = ray.b[primary] - ray.a[primary];
    let direction = delta.signum();
    let offset = f64::from(u8::from(direction > 0.0));
    let stop = ((ray.a[primary] + ray.end() * delta) * direction * 2.0)
        .floor()
        .midpoint(offset);
    let mut axis = ((ray.a[primary] + ray.skip * direction) * 2.0)
        .floor()
        .midpoint(offset);
    let (mut level, mut levels, mut steps) = (0i32, 0u32, 0u32);
    let mut allowance = ray.behind;
    while axis * direction < stop && steps < ray.budget {
        steps += 1;
        levels = (levels * 5 + u32::try_from(level + 1).unwrap()) % 1021;
        let width = 2.0f64.powi(level);
        let mut next = (((axis * direction / width).floor() + 1.0) * width).min(stop) * direction;
        if level < 0 {
            next = child_axis(ray, primary, axis, next);
        }
        let start = ray.position(axis, primary);
        let finish = ray.position(next, primary);
        let middle = ray.position(axis.midpoint(next), primary);
        let surface = refined_surface(ray, depths, level, [axis, next], primary);
        if surface < start[2].max(finish[2]) {
            if level > -1 {
                level -= 1;
                continue;
            }
            if start[2].min(finish[2]) - 0.25 <= surface {
                return oracle_hit(ray, middle, surface, levels, steps);
            }
            if allowance == 0 {
                break;
            }
            allowance -= 1;
        } else {
            level = (level + 1).min(i32::try_from(ray.levels - 1).unwrap());
        }
        axis = next;
    }
    oracle_miss(levels, steps)
}

fn refined_surface(
    ray: Ray,
    depths: &[Vec<f64>],
    level: i32,
    axes: [f64; 2],
    primary: usize,
) -> f64 {
    if level < 0 {
        return fetch(depths, 0, ray.position(axes[0].midpoint(axes[1]), primary));
    }
    let direction = (ray.b[primary] - ray.a[primary]).signum();
    let mut a = ray.position(axes[0], primary);
    let mut b = ray.position(axes[1], primary);
    a[primary] += direction * 0.001;
    b[primary] -= direction * 0.001;
    fetch(depths, usize::try_from(level).unwrap(), a).min(fetch(
        depths,
        usize::try_from(level).unwrap(),
        b,
    ))
}

fn child_axis(ray: Ray, primary: usize, axis: f64, next: f64) -> f64 {
    let primary_delta = ray.b[primary] - ray.a[primary];
    let minor_delta = ray.b[1 - primary] - ray.a[1 - primary];
    if minor_delta == 0.0 {
        return next;
    }
    let direction = primary_delta.signum();
    let mut closest = next;
    // Enumerate image-grid crossings in parameter space, independently of the GPU DDA.
    for edge in 0..=ray.side {
        let fraction = (f64::from(edge) * 0.5 - ray.a[1 - primary]) / minor_delta;
        let crossing = ray.a[primary] + fraction * primary_delta;
        if crossing * direction > axis * direction && crossing * direction < closest * direction {
            closest = crossing;
        }
    }
    closest
}
