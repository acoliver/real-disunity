use super::{
    UiDrawKind, UiDrawList, UiError, UiImage, UiNineSlice, UiRect, UiSliceCenter, UiSliceMode,
    UiTexture, check_color,
};
use glam::{UVec2, Vec2};

#[derive(Clone, Copy)]
struct Region {
    destination: [f32; 2],
    source: [f32; 2],
    tile_size: Option<f32>,
    center: bool,
}
impl Region {
    fn segments(self) -> impl Iterator<Item = Segment> {
        let count = if self.source[1] <= self.source[0] {
            0
        } else if self.tile_size.is_some() {
            u32::MAX
        } else {
            1
        };
        (0..count)
            .map(move |index| self.segment(index))
            .take_while(move |s| s.destination[0] < self.destination[1])
            .filter(|s| s.destination[1] > s.destination[0])
    }
    fn segment(self, index: u32) -> Segment {
        let mut segment = Segment {
            destination: self.destination,
            source: self.source,
            center: self.center,
        };
        if let Some(tile) = self.tile_size {
            let index = UVec2::splat(index).as_vec2().x;
            let start = self.destination[0] + index * tile;
            let end = (self.destination[0] + (index + 1.) * tile).min(self.destination[1]);
            segment.destination = [start, end];
            segment.source[1] =
                self.source[0] + (self.source[1] - self.source[0]) * ((end - start) / tile);
        }
        segment
    }
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
        let inner = if sum >= extent {
            let seam = scaled.x * (extent / sum);
            [seam; 2]
        } else {
            [scaled.x, (extent - scaled.y).max(scaled.x)]
        };
        let d = [0., inner[0], inner[1], extent];
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
            if region.destination[1] <= region.destination[0] {
                continue;
            }
            if region
                .tile_size
                .is_some_and(|tile| tile <= 0. || !tile.is_finite())
            {
                return Err(UiError::InvalidImage);
            }
            *count = region.segments().take(maximum + 1).count();
            if *count > maximum {
                return Err(UiError::DrawLimit);
            }
        }
        Ok(counts)
    }
    fn segments(&self, counts: [usize; 3]) -> Vec<Segment> {
        let mut result = Vec::with_capacity(counts.into_iter().sum());
        for (region, count) in self.regions.iter().zip(counts) {
            for mut segment in region.segments().take(count) {
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
        let border_xs: Vec<_> = xs.iter().copied().filter(|x| !x.center).collect();
        let mut draws = Vec::with_capacity(count);
        let extent = image.size.as_vec2();
        for y in ys {
            let row = if y.center && style.center == UiSliceCenter::Omit {
                &border_xs
            } else {
                &xs
            };
            for x in row {
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
                    Some(UiTexture::Image(image.id.0)),
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
