use super::{
    UiDrawKind, UiDrawList, UiError, UiImage, UiNineSlice, UiRect, UiSliceCenter, UiSliceMode,
    check_color,
};
use glam::{DVec2, Vec2};

#[derive(Clone, Copy)]
struct Region {
    destination: [f32; 2],
    source: [f32; 2],
    tile_size: Option<f32>,
    center: bool,
}
#[derive(Clone, Copy)]
struct Segment {
    destination: [f32; 2],
    source: [f32; 2],
    center: bool,
}
struct Axis {
    regions: [Region; 3],
    extent: f32,
    flip: bool,
}
impl Axis {
    fn new(
        source: [f32; 2],
        extent: f32,
        borders: [f32; 2],
        scale: f32,
        tile: bool,
        flip: bool,
    ) -> Result<Self, UiError> {
        let scaled = Vec2::from_array(borders) * scale;
        let sum = scaled.element_sum();
        if !scaled.is_finite() || !sum.is_finite() {
            return Err(UiError::InvalidImage);
        }
        let scaled = if sum > extent {
            scaled * (extent / sum)
        } else {
            scaled
        };
        let d = [0., scaled.x, (extent - scaled.y).max(scaled.x), extent];
        let s = [
            source[0],
            source[0] + borders[0],
            source[1] - borders[1],
            source[1],
        ];
        let regions = std::array::from_fn(|i| Region {
            destination: [d[i], d[i + 1]],
            source: [s[i], s[i + 1]],
            center: i == 1,
            tile_size: (tile && i == 1).then_some((s[i + 1] - s[i]) * scale),
        });
        Ok(Self {
            regions,
            extent,
            flip,
        })
    }
    fn has_borders(&self) -> bool {
        [self.regions[0], self.regions[2]]
            .iter()
            .any(|r| r.destination[1] > r.destination[0] && r.source[1] > r.source[0])
    }
    fn counts(&self, maximum: usize) -> Result<[usize; 3], UiError> {
        let mut counts = [0; 3];
        for (count, region) in counts.iter_mut().zip(self.regions) {
            let length = region.destination[1] - region.destination[0];
            if length <= 0. {
                continue;
            }
            let copies = match region.tile_size {
                Some(tile) => {
                    if tile <= 0. || !tile.is_finite() {
                        return Err(UiError::InvalidImage);
                    }
                    (f64::from(length) / f64::from(tile)).ceil()
                }
                None if region.source[1] <= region.source[0] => continue,
                None => 1.,
            };
            let bound = u32::try_from(maximum).expect("bounded quad budget");
            if copies > f64::from(bound) {
                return Err(UiError::DrawLimit);
            }
            *count = usize::try_from(DVec2::splat(copies).as_uvec2().x).expect("bounded copies");
        }
        Ok(counts)
    }
    fn segments(&self, counts: [usize; 3]) -> Vec<Segment> {
        let mut result = Vec::with_capacity(counts.into_iter().sum());
        for (region, count) in self.regions.iter().zip(counts) {
            for index in 0..count {
                let mut segment = Segment {
                    destination: region.destination,
                    source: region.source,
                    center: region.center,
                };
                if let Some(tile) = region.tile_size {
                    let index = f32::from(u16::try_from(index).expect("bounded tiles"));
                    let start = region.destination[0] + index * tile;
                    let end =
                        (region.destination[0] + (index + 1.) * tile).min(region.destination[1]);
                    segment.destination = [start, end];
                    segment.source[1] = region.source[0]
                        + (region.source[1] - region.source[0]) * ((end - start) / tile);
                }
                if self.flip {
                    segment.destination = [
                        self.extent - segment.destination[1],
                        self.extent - segment.destination[0],
                    ];
                    segment.source.swap(0, 1);
                }
                result.push(segment);
            }
        }
        if self.flip {
            result.reverse();
        }
        result
    }
}

impl UiDrawList<'_> {
    pub fn nine_slice(
        &mut self,
        image: UiImage,
        destination: UiRect,
        style: UiNineSlice,
    ) -> Result<(), UiError> {
        destination.validate()?;
        let source = image.source(style.source)?;
        check_color(style.color)?;
        let i = style.insets;
        if [i.left, i.top, i.right, i.bottom]
            .into_iter()
            .any(|v| !v.is_finite() || v < 0.)
            || i.left + i.right > source.size.x
            || i.top + i.bottom > source.size.y
            || !style.scale.is_finite()
            || style.scale.min_element() <= 0.
        {
            return Err(UiError::InvalidImage);
        }
        if self.hidden(destination) {
            return Ok(());
        }
        let (x, y) = slice_axes(source, destination, style)?;
        let max = self.limits.max_vertices / 6;
        let (cx, cy) = (x.counts(max)?, y.counts(max)?);
        let mut count = cx
            .iter()
            .sum::<usize>()
            .checked_mul(cy.iter().sum())
            .ok_or(UiError::DrawLimit)?;
        if style.center == UiSliceCenter::Omit {
            count -= cx[1] * cy[1];
        }
        self.check_count(count)?;
        let (xs, ys) = (x.segments(cx), y.segments(cy));
        let mut draws = Vec::with_capacity(count);
        let extent = image.size.as_vec2();
        for y in ys {
            for x in &xs {
                if x.center && y.center && style.center == UiSliceCenter::Omit {
                    continue;
                }
                let rect = UiRect {
                    origin: destination.origin + Vec2::new(x.destination[0], y.destination[0]),
                    size: Vec2::new(
                        x.destination[1] - x.destination[0],
                        y.destination[1] - y.destination[0],
                    ),
                };
                let uv = [
                    x.source[0] / extent.x,
                    y.source[0] / extent.y,
                    x.source[1] / extent.x,
                    y.source[1] / extent.y,
                ];
                draws.push(self.quad(
                    rect.points(),
                    uv,
                    style.color,
                    Some(image.id.0),
                    UiDrawKind::Graphic,
                )?);
            }
        }
        self.commit(draws)
    }
}

fn slice_axes(
    source: UiRect,
    destination: UiRect,
    style: UiNineSlice,
) -> Result<(Axis, Axis), UiError> {
    let i = style.insets;
    let tile = style.mode == UiSliceMode::Tile;
    let mut x = Axis::new(
        [source.origin.x, source.origin.x + source.size.x],
        destination.size.x,
        [i.left, i.right],
        style.scale.x,
        tile,
        style.flip.x(),
    )?;
    let mut y = Axis::new(
        [source.origin.y, source.origin.y + source.size.y],
        destination.size.y,
        [i.top, i.bottom],
        style.scale.y,
        tile,
        style.flip.y(),
    )?;
    if style.center == UiSliceCenter::Omit {
        let (x_border, y_border) = (x.has_borders(), y.has_borders());
        // A centre axis is only expanded when a perpendicular border samples it.
        if !y_border {
            x.regions[1].destination = [0.; 2];
        }
        if !x_border {
            y.regions[1].destination = [0.; 2];
        }
    }
    Ok((x, y))
}
