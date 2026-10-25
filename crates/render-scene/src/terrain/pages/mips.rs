//! Area-box reduction over the complete padded domain, including odd edge bins.

use super::composition::{CompositionError, MaterialImage, MaterialTexel, pack_normal};
use glam::DVec3;

/// Builds every hardware-sized level through 1x1.
///
/// If weighted directions cancel exactly, reduction preserves the first
/// contributing direction in row-major order. This explicit antipodal tie rule
/// is shared with the GPU reducer, including prepared GPU inputs. It chooses a
/// source direction rather than inventing a surface normal.
///
/// Each destination cell integrates its exact source-area footprint. Odd edges
/// are neither dropped nor duplicated. UVs retain the base padded domain at all
/// levels; settings restrict sampling to mips that still contain a full gutter.
pub fn build_mips(base: MaterialImage) -> Result<Vec<MaterialImage>, CompositionError> {
    let mut levels = vec![base];
    while levels.last().expect("base level exists").extent() != [1, 1] {
        let source = levels.last().expect("base level exists");
        let extent = source.extent().map(|value| (value / 2).max(1));
        let mut texels = Vec::new();
        for y in 0..extent[1] {
            for x in 0..extent[0] {
                texels.push(reduce_texel(source, extent, [x, y]));
            }
        }
        levels.push(MaterialImage::new(extent, texels)?);
    }
    Ok(levels)
}

fn overlap(source: u32, index: u32) -> (u64, u64) {
    (
        u64::from(index) * u64::from(source),
        u64::from(index + 1) * u64::from(source),
    )
}

fn cell_weight(cell: u32, scale: u32, bounds: (u64, u64)) -> u64 {
    let start = u64::from(cell) * u64::from(scale);
    let end = start + u64::from(scale);
    end.min(bounds.1) - start.max(bounds.0)
}

fn reduce_texel(source: &MaterialImage, extent: [u32; 2], at: [u32; 2]) -> MaterialTexel {
    let [width, height] = source.extent();
    let bx = overlap(width, at[0]);
    let by = overlap(height, at[1]);
    let mut accumulation = Accumulation::new();
    for y in (by.0 / u64::from(extent[1]))..by.1.div_ceil(u64::from(extent[1])) {
        for x in (bx.0 / u64::from(extent[0]))..bx.1.div_ceil(u64::from(extent[0])) {
            let x = u32::try_from(x).expect("bounded by source width");
            let y = u32::try_from(y).expect("bounded by source height");
            let weight = cell_weight(x, extent[0], bx) * cell_weight(y, extent[1], by);
            let index = usize::try_from(u64::from(y) * u64::from(width) + u64::from(x))
                .expect("validated image");
            accumulation.add(source.texels()[index], weight);
        }
    }
    accumulation.finish(u64::from(width) * u64::from(height))
}

struct Accumulation {
    channels: [u128; 5],
    normal: DVec3,
    first_normal: Option<DVec3>,
}

impl Accumulation {
    fn new() -> Self {
        Self {
            channels: [0; 5],
            normal: DVec3::ZERO,
            first_normal: None,
        }
    }

    #[allow(
        clippy::cast_precision_loss,
        reason = "area weights convert to floating point only for vector averaging"
    )]
    fn add(&mut self, texel: MaterialTexel, weight: u64) {
        for (sum, value) in self
            .channels
            .iter_mut()
            .zip(texel.color.into_iter().chain([texel.normal_roughness[3]]))
        {
            *sum += u128::from(value) * u128::from(weight);
        }
        let direction = DVec3::from_array(std::array::from_fn(|axis| {
            f64::from(texel.normal_roughness[axis]) * 2.0 - 255.0
        }));
        let direction = direction.normalize();
        self.first_normal.get_or_insert(direction);
        self.normal += direction * weight as f64;
    }

    fn finish(self, weight: u64) -> MaterialTexel {
        let normal = self
            .normal
            .try_normalize()
            .or(self.first_normal)
            .expect("nonempty reduction footprint")
            .as_vec3();
        let weight = u128::from(weight);
        let values = self
            .channels
            .map(|sum| u8::try_from((sum + weight / 2) / weight).expect("weighted UNORM mean"));
        MaterialTexel {
            color: [values[0], values[1], values[2], values[3]],
            normal_roughness: [
                pack_normal(normal.x),
                pack_normal(normal.y),
                pack_normal(normal.z),
                values[4],
            ],
        }
    }
}
