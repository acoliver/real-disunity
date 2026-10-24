use serde_json::Value;

pub(crate) fn physics_shapes(value: &Value, ppu: f64) -> Result<Vec<Vec<[f64; 2]>>, String> {
    if value.is_null() {
        return Ok(Vec::new());
    }
    let paths = value
        .as_array()
        .ok_or("sprite physicsShape must be an array")?;
    if paths.len() > 4096 {
        return Err("sprite physicsShape budget exceeds 4096".into());
    }
    let mut result = Vec::new();
    for path in paths {
        let points = path
            .as_array()
            .ok_or("physicsShape outline must be an array")?;
        if points.len() > 129 {
            return Err("physicsShape outline exceeds 128 vertices".into());
        }
        let mut polygon: Vec<_> = points
            .iter()
            .map(|p| {
                let x = p["x"]
                    .as_f64()
                    .filter(|n| n.is_finite())
                    .ok_or("physicsShape x must be finite")?;
                let y = p["y"]
                    .as_f64()
                    .filter(|n| n.is_finite())
                    .ok_or("physicsShape y must be finite")?;
                Ok([x / ppu, y / ppu])
            })
            .collect::<Result<_, String>>()?;
        if polygon.first() == polygon.last() {
            polygon.pop();
        }
        result.extend(decompose(polygon)?);
        if result.len() > 4096 {
            return Err("sprite convex shape budget exceeds 4096".into());
        }
    }
    Ok(result)
}
fn cross(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}
fn area(points: &[[f64; 2]]) -> f64 {
    points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
        .map(|(a, b)| a[0] * b[1] - a[1] * b[0])
        .sum::<f64>()
}
fn decompose(mut points: Vec<[f64; 2]>) -> Result<Vec<Vec<[f64; 2]>>, String> {
    if points.len() < 3 || points.len() > 128 || area(&points).abs() < 1e-10 {
        return Err("physicsShape must have 3..128 vertices and nonzero area".into());
    }
    validate_simple(&points)?;
    if area(&points) < 0. {
        points.reverse();
    }
    remove_collinear(&mut points);
    if (0..points.len()).all(|i| corner(&points, i) > 1e-10) {
        return Ok(vec![points]);
    }
    let mut pieces = Vec::new();
    while points.len() > 3 {
        let i = (0..points.len())
            .find(|&i| is_ear(&points, i))
            .ok_or("physicsShape cannot be decomposed into convex pieces")?;
        let n = points.len();
        pieces.push(vec![
            points[(i + n - 1) % n],
            points[i],
            points[(i + 1) % n],
        ]);
        points.remove(i);
    }
    pieces.push(points);
    Ok(pieces)
}
fn corner(points: &[[f64; 2]], i: usize) -> f64 {
    let n = points.len();
    cross(points[(i + n - 1) % n], points[i], points[(i + 1) % n])
}
fn remove_collinear(points: &mut Vec<[f64; 2]>) {
    while points.len() > 3 {
        let Some(i) = (0..points.len()).find(|&i| corner(points, i).abs() < 1e-10) else {
            break;
        };
        points.remove(i);
    }
}
fn is_ear(points: &[[f64; 2]], i: usize) -> bool {
    if corner(points, i) <= 1e-10 {
        return false;
    }
    let n = points.len();
    let before = (i + n - 1) % n;
    let after = (i + 1) % n;
    !points.iter().enumerate().any(|(j, &p)| {
        j != before
            && j != i
            && j != after
            && cross(points[before], points[i], p) >= -1e-10
            && cross(points[i], points[after], p) >= -1e-10
            && cross(points[after], points[before], p) >= -1e-10
    })
}
fn validate_simple(points: &[[f64; 2]]) -> Result<(), String> {
    let n = points.len();
    for i in 0..n {
        if (0..2).all(|axis| (points[i][axis] - points[(i + 1) % n][axis]).abs() < 1e-10) {
            return Err("physicsShape has repeated vertices".into());
        }
        for j in i + 1..n {
            if j == i + 1 || (i == 0 && j == n - 1) {
                continue;
            }
            if intersects(
                points[i],
                points[(i + 1) % n],
                points[j],
                points[(j + 1) % n],
            ) {
                return Err("physicsShape outline self-intersects".into());
            }
        }
    }
    Ok(())
}
fn intersects(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> bool {
    if (0..2).any(|i| a[i].max(b[i]) < c[i].min(d[i]) || c[i].max(d[i]) < a[i].min(b[i])) {
        return false;
    }
    cross(a, b, c) * cross(a, b, d) <= 0. && cross(c, d, a) * cross(c, d, b) <= 0.
}
