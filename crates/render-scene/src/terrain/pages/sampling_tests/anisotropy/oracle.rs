use super::{Fixture, MaterialImage, PageKey};
use glam::{DVec2, DVec3, DVec4};

// Diagonal derivatives have analytically known principal axes. This f64 oracle
// searches resident rectangles independently of the compact shader quadtree.
pub(super) fn sample(fixture: &Fixture, point: [f32; 4], level: u16) -> (f64, Vec<PageKey>, DVec3) {
    let center = DVec2::new(f64::from(point[0]), f64::from(point[1]));
    let scale = f64::from(1_u32 << resident(fixture, center, 0.0).0.depth());
    let size = DVec2::new(32.0, 16.0) * scale;
    let derivatives = DVec2::new(f64::from(point[2]), f64::from(point[3])) * size;
    let major = derivatives.abs().max_element();
    let minor = derivatives.abs().min_element();
    let mut count = 1_u16;
    while major > 1.0 && count < level && f64::from(count) < major / minor {
        count += 1;
    }
    let direction = if derivatives.x.abs() >= derivatives.y.abs() {
        DVec2::X
    } else {
        DVec2::Y
    } * major
        / size;
    let footprint = (major / f64::from(count)).max(minor) / scale;
    let mut keys = Vec::new();
    let mut sum = 0.0;
    let mut normal = DVec3::ZERO;
    let mut first_normal = DVec3::ZERO;
    for tap in 0..count {
        let offset = (f64::from(tap) + 0.5) / f64::from(count) - 0.5;
        let global = (center + direction * offset).clamp(DVec2::ZERO, DVec2::ONE);
        let (key, value) = read(fixture, global, footprint);
        sum += value.w;
        let decoded = value.truncate() * 2.0 - DVec3::splat(255.0);
        // Scale first so opposite diagonal directions share identical normalization rounding.
        let direction = (decoded / decoded.abs().max_element()).normalize();
        if tap == 0 {
            first_normal = direction;
        }
        normal += direction;
        keys.push(key);
    }
    let direction = normal.try_normalize().unwrap_or(first_normal);
    (sum / f64::from(count), keys, direction)
}

fn resident(fixture: &Fixture, global: DVec2, footprint: f64) -> &(PageKey, Vec<MaterialImage>) {
    fixture
        .images
        .iter()
        .rev()
        .find(|(key, _)| {
            let scale = f64::from(1_u32 << key.depth());
            let local = global * scale - DVec2::from_array(key.coordinate().map(f64::from));
            local.cmpge(DVec2::ZERO).all()
                && local.cmple(DVec2::ONE).all()
                && (key.depth() == 0
                    || footprint * scale
                        <= 2.0_f64.powf(f64::from(fixture.cache.settings().maximum_sampling_mip())))
        })
        .unwrap()
}

fn read(fixture: &Fixture, global: DVec2, footprint: f64) -> (PageKey, DVec4) {
    let (key, mips) = resident(fixture, global, footprint);
    let scale = f64::from(1_u32 << key.depth());
    let max_mip = fixture.cache.settings().maximum_sampling_mip();
    let lod = (footprint * scale).max(1.0).log2().min(f64::from(max_mip));
    let low = (0..=max_mip).rev().find(|m| f64::from(*m) <= lod).unwrap();
    let high = (low + 1).min(max_mip);
    let local = global * scale - DVec2::from_array(key.coordinate().map(f64::from));
    let page_size = DVec2::new(32.0, 16.0);
    let uv = (local * page_size + DVec2::splat(4.0)) / (page_size + DVec2::splat(8.0));
    let a = bilinear(&mips[usize::try_from(low).unwrap()], uv);
    let b = bilinear(&mips[usize::try_from(high).unwrap()], uv);
    (*key, a + (b - a) * lod.fract())
}

fn bilinear(image: &MaterialImage, uv: DVec2) -> DVec4 {
    let extent = image.extent();
    let position = uv * DVec2::from_array(extent.map(f64::from)) - DVec2::splat(0.5);
    let lower: [i32; 2] = std::array::from_fn(|axis| {
        (-1..i32::try_from(extent[axis]).unwrap())
            .rev()
            .find(|v| f64::from(*v) <= position[axis])
            .unwrap()
    });
    let fraction = position - position.floor();
    let mut sum = DVec4::ZERO;
    for y in 0..2 {
        for x in 0..2 {
            let at: [u32; 2] = std::array::from_fn(|axis| {
                u32::try_from(
                    (lower[axis] + [x, y][axis]).clamp(0, i32::try_from(extent[axis] - 1).unwrap()),
                )
                .unwrap()
            });
            let texel = image.texels()[usize::try_from(at[1] * extent[0] + at[0]).unwrap()];
            let value = DVec4::new(
                f64::from(texel.normal_roughness[0]),
                f64::from(texel.normal_roughness[1]),
                f64::from(texel.normal_roughness[2]),
                f64::from(texel.color[0]),
            );
            let weight = |axis, side| {
                if side == 0 {
                    1.0 - fraction[axis]
                } else {
                    fraction[axis]
                }
            };
            sum += value * weight(0, x) * weight(1, y);
        }
    }
    sum
}
