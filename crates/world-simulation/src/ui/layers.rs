//! Runs of UI draws that a game's shader draws, and their places in the layer atlas.
use super::material::UiMaterial;
use super::presentation::{UiDraw, UiPresentation};
use super::texture::UiTexture;
use glam::{UVec2, Vec2};
use std::ops::Range;

/// Consecutive draws of one material and one image inside a run. The program reads the image as
/// the draw's main texture, so a draw of another image starts another span.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiSpan {
    pub material: UiMaterial,
    pub texture: Option<UiTexture>,
    /// Indices into `UiPresentation::draws`.
    pub draws: Range<usize>,
}

/// Consecutive draws with a game material and one clip rectangle. The program draws them into
/// `atlas`, and the overlay shows that rectangle at the sort position of the first draw.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiRun {
    /// Indices into `UiPresentation::draws`.
    pub draws: Range<usize>,
    /// The lower-left pixel clip rectangle every draw of the run shares: x, y, width, height.
    pub clip: [u32; 4],
    /// The part of the run that shows, in lower-left viewport pixels: x, y, width, height.
    pub bounds: [u32; 4],
    /// Where the run sits in the atlas, in lower-left atlas pixels. It has the size of `bounds`.
    pub atlas: [u32; 4],
    pub spans: Vec<UiSpan>,
}

/// The runs of a presentation. Every draw with a material is in a run, in `too_large`, or has
/// nothing visible and is dropped.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UiLayers {
    /// The size of the atlas the runs are packed into, in pixels.
    pub atlas_size: [u32; 2],
    pub runs: Vec<UiRun>,
    /// The draws of each run whose rectangle does not fit the atlas.
    pub too_large: Vec<Range<usize>>,
}

impl UiPresentation {
    /// Groups the draws that a game's shader draws into runs inside `viewport` and packs them
    /// into an atlas of `atlas` pixels. A run with nothing visible in the viewport is dropped.
    #[must_use]
    pub fn layers(&self, viewport: [u32; 2], atlas: [u32; 2]) -> UiLayers {
        let mut layers = UiLayers {
            atlas_size: atlas,
            ..UiLayers::default()
        };
        let mut shelves = Shelves::new(atlas);
        for draws in run_ranges(&self.draws) {
            let clip = self.draws[draws.start].clip_rect;
            let Some(bounds) = visible_bounds(&self.draws[draws.clone()], clip, viewport) else {
                continue;
            };
            let Some(place) = shelves.place([bounds[2], bounds[3]]) else {
                layers.too_large.push(draws);
                continue;
            };
            layers.runs.push(UiRun {
                spans: spans(&self.draws, &draws),
                draws,
                clip,
                bounds,
                atlas: place,
            });
        }
        layers
    }
}

/// The ranges of consecutive draws that have a material and one clip rectangle.
fn run_ranges(draws: &[UiDraw]) -> Vec<Range<usize>> {
    let mut runs: Vec<Range<usize>> = vec![];
    for (index, draw) in draws.iter().enumerate() {
        if draw.material.is_none() {
            continue;
        }
        match runs.last_mut() {
            Some(run) if run.end == index && draws[run.start].clip_rect == draw.clip_rect => {
                run.end += 1;
            }
            _ => runs.push(index..index + 1),
        }
    }
    runs
}

/// The consecutive draws of one material and image in `run`.
fn spans(draws: &[UiDraw], run: &Range<usize>) -> Vec<UiSpan> {
    let mut spans: Vec<UiSpan> = vec![];
    for index in run.clone() {
        let material = draws[index].material.expect("a run holds material draws");
        let texture = draws[index].texture;
        match spans.last_mut() {
            Some(span) if span.material == material && span.texture == texture => {
                span.draws.end += 1;
            }
            _ => spans.push(UiSpan {
                material,
                texture,
                draws: index..index + 1,
            }),
        }
    }
    spans
}

/// The whole-pixel rectangle `draws` cover inside the viewport and `clip`, or none when no pixel
/// of it shows.
fn visible_bounds(draws: &[UiDraw], clip: [u32; 4], viewport: [u32; 2]) -> Option<[u32; 4]> {
    let corners = draws.iter().flat_map(|draw| draw.corners);
    let low = corners
        .clone()
        .fold(Vec2::INFINITY, |low, c| low.min(Vec2::new(c[0], c[1])));
    let high = corners.fold(Vec2::NEG_INFINITY, |high, c| {
        high.max(Vec2::new(c[0], c[1]))
    });
    let clip_low = UVec2::new(clip[0], clip[1]).as_vec2();
    let clip_high = clip_low + UVec2::new(clip[2], clip[3]).as_vec2();
    let low = low.floor().max(clip_low);
    let high = high
        .ceil()
        .min(clip_high)
        .min(UVec2::from(viewport).as_vec2());
    if !high.cmpgt(low).all() {
        return None;
    }
    let (origin, size) = (low.as_uvec2(), (high - low).as_uvec2());
    Some([origin.x, origin.y, size.x, size.y])
}

/// Next-fit shelf packing: runs fill a row left to right, and a run that does not fit the row
/// starts the next one. The same runs in the same order always land in the same places.
struct Shelves {
    size: [u32; 2],
    x: u32,
    y: u32,
    height: u32,
}

impl Shelves {
    fn new(size: [u32; 2]) -> Self {
        Self {
            size,
            x: 0,
            y: 0,
            height: 0,
        }
    }

    fn place(&mut self, [width, height]: [u32; 2]) -> Option<[u32; 4]> {
        if width > self.size[0] || height > self.size[1] {
            return None;
        }
        if self.x + width > self.size[0] {
            self.y += self.height;
            self.x = 0;
            self.height = 0;
        }
        if self.y + height > self.size[1] {
            return None;
        }
        let at = [self.x, self.y, width, height];
        self.x += width;
        self.height = self.height.max(height);
        Some(at)
    }
}
