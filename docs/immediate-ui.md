# Drawing an immediate overlay

Enable the `worldrunner` facade's `ui` feature to build a CPU draw list. Keep a
`FontBook` across frames, register your owned TrueType bytes once, and pass it to
`UiDrawList`. The list produces the same `UiPresentation` that the retained UI
compositor accepts. It does not create a world, window or renderer.

```rust
use worldrunner::glam::{UVec2, Vec2};
use worldrunner::ui::{
    FontBook, UiBorder, UiDrawLimits, UiDrawList, UiError, UiOrder, UiRect,
};

fn panel(fonts: &FontBook) -> Result<worldrunner::ui::presentation::UiPresentation, UiError> {
    let mut list = UiDrawList::new(
        UVec2::new(800, 600), fonts, UiOrder::default(), UiDrawLimits::default(),
    )?;
    list.border_rect(UiRect::new(20., 20., 240., 100.), UiBorder {
        width: Vec2::new(2., 3.),
        color: [1., 1., 1., 0.8],
        fill: Some([0.05, 0.05, 0.05, 0.9]),
    })?;
    list.push_clip(UiRect::new(22., 23., 236., 94.))?;
    list.solid_rect(UiRect::new(30., 30., 40., 20.), [0.2, 0.6, 1., 1.])?;
    list.pop_clip()?;
    list.finish()
}
```

## Coordinates, colour and order

Rectangles use physical pixels from the viewport's top-left, with positive Y
down. Negative dimensions and nonfinite inputs fail. Zero-area destinations and
empty clips draw nothing. `push_clip` intersects with the active clip;
`push_clip_replace` pushes a rectangle bounded only by the viewport. `pop_clip`
restores the previous clip for either operation. Rectangles remain floats and
round inward only when emitted as integer scissors. They do not cut or distort
rotated quads.

Colours are linear straight-alpha RGBA in 0..=1. `set_color` and `set_alpha`
replace the current modifier values. `push_modifier` saves the current values and
multiplies them by a `UiColorModifier`; `pop_modifier` restores the saved values.
Clip and modifier stacks must be balanced at `finish`. Root entries cannot be
popped. Border strips meet without overlapping translucent corners.

Every emitted quad gets a distinct ascending hierarchy number, in submission
order. `UiOrder` fixes the layer, order and first hierarchy number. Use
`draw_range().end` as the next contribution's `first_hierarchy`, or choose a
different layer/order. `UiPresentation::append` remaps glyph texture identities
but does not rebase painter order. No texture sorting happens in the list.

## Images and frames

`UiImage::new(UiImageId(id), pixel_size)` describes an immutable resource
generation. The caller resolves `id` to a `SceneTexture` in the image map passed
to `UiOverlay::from_presentation`. A missing image is an error. Change the image
identity when its resource generation changes, and build a replacement snapshot.
The list performs no loading or decoding.

`image` accepts a destination and `UiImageOptions`. An optional source rectangle
is measured in top-left texture pixels. Four colours attach to destination
bottom-left, bottom-right, top-right and top-left. Flips reverse sampling only.
Clockwise rotation is in radians about a normalized destination pivot, which
must lie in 0..=1 on each axis. Corner colours interpolate across the quad's two
triangles, rather than as a bilinear field.

`nine_slice` accepts source insets in left/top/right/bottom order, positive X/Y
border scale, Stretch or Tile, and Draw or Omit for the centre. Oversized opposing
borders compress proportionally and meet at one shared seam, leaving exactly
zero centre extent. Tiled edges and centres repeat their source patch; the last
partial tile crops both geometry and UVs. Zero-length tiles emit no draw and
consume no vertex budget. Omitting the centre expands only border combinations.
Flipping mirrors the whole expanded frame, including asymmetric borders and
partial-tile phase.
Texture sampling should normally use Bilinear/Clamp and source patches need
filtering gutters. Encoded BC7 sRGB images use the existing
`SceneTexture::Encoded` DDS contract, not a new texel format.

## Measure and draw text

Construct a `TextRun` with a registered `FontId`. `FontBook::measure(run, size)`
returns maximum line advance, full content height and line metrics without
building an atlas. Wrapping follows the run and size.x; vertical truncation is
ignored for measurement. `list.measure_text(run, None)` disables wrapping, while
`Some(width)` enables it. Width includes unconsumed spaces, not just visible ink.
Blank and trailing lines contribute full font metric height. Empty input has no
lines and zero width/height.

Each line provides `source_range` and `next_byte` in the original UTF-8 input.
Ranges exclude LF and consumed wrap spaces; ignored CR can remain inside a
range. `next_byte` skips a separator or equals input length. Use
`LineMetrics::source` or `TextRun::substring` for checked slicing. The latter
clips and rebases structured spans and rejects partial UTF-8 characters.

`text` accepts a `UiTextPlacement` and `UiTextEffect::{None, Shadow, Outline, Glow}`.
`UiTextPlacement::new(destination)` uses the destination size for layout. Set
`layout_size` explicitly to stretch one layout into another rectangle, including
nonuniform stretching. Rotation follows the image convention. Glyph ink can
extend outside this box; only the list's clip stack bounds it. The run's overflow
settings still determine which lines are laid out. Shadow offset is in final
screen axes after stretching and rotation. All shadow glyphs precede every fill
glyph. Shadow RGB uses its own colour and global tint; its alpha also inherits
run/span alpha.

`Outline { color, size }` draws an outward band; `Glow { color, size }` draws a
smooth falloff outside the edge. Size is a finite nonnegative physical-pixel
radius. Zero uses ordinary coverage. Both effects reuse the font book's bounded
4x signed-distance rasterization and the same atlas as their fill. Each glyph is
one padded quad: the shader combines effect and foreground masks before one
straight-alpha blend, placing the effect beneath that glyph's fill without an
offset-sampling pass. Glyphs remain in text order, so a later glyph's halo can
cover an earlier glyph. Global tint multiplies effect RGB independently of run
and span RGB; effect alpha inherits run, span and global alpha once.

Distance decoding is positive inside the glyph. For screen distance D, fill is
`smoothstep(-0.5, 0.5, D)`. Outline subtracts that mask from
`smoothstep(-size-0.5, -size+0.5, D)`. Glow uses
`smoothstep(-size, 0, D) * (1-fill)`. The one-pixel antialias transition is in
screen pixels. R8 quantization and the 4x raster approximate the font contour.
Geometric UV derivatives set distance scale, including at flat samples and
medial axes. Rotation and uniform scaling preserve physical radius. Nonuniform
stretch uses the smaller placement scale: bands can be wider along the stretched
direction than an exact transformed contour offset. No sampled-distance gradient
normalization or blur kernel is used.

Padding is `ceil((size+2)/min_scale)+1` atlas pixels and is also the encoded spread.
Sampling gutters are separate. Padded glyph sides are at most 256 pixels, with
bounded scratch of at most 30,474,240 payload bytes per glyph. Excessive radius,
minification, glyph size or scratch demand returns `DistanceFieldLimit`; invalid
size returns `InvalidTextEffect`. Atlas and frame budgets retain their own named
errors. Failed effects never downgrade to coverage. Measurement and line metrics
exclude padding and effects.

Printable ASCII, Latin-1 and LF/CR are supported. Missing glyphs and unsupported
shaping fail explicitly. There is no font substitution, markup interpretation,
bidirectional text or ligature shaping. Horizontal GPOS pair advances and legacy
kerning share the font book's line placement.

## Submit and retain

Convert the finished presentation with `UiOverlay::from_presentation`, supplying
the caller's image map, then publish `RetainedUpdate::Ui(Arc<UiOverlay>)`. It is
composited after tone mapping through the existing retained renderer. Publish
replacements only when content, images or viewport change; an empty overlay
clears the previous one.

Finished presentations hold shared glyph bytes without borrowing the font book.
Cache eviction or dropping the font book cannot invalidate them. The overlay
conversion clones image snapshots, including their boxed CPU bytes, and glyph
upload expands R8 coverage to RGBA. Image snapshots are not zero-copy.

`UiDrawLimits` bounds each stack, expanded vertices, submitted text characters
and unique referenced atlas pixels, further restricted by `UiLimits`. Six
vertices are charged per quad, including every frame patch, shadow and distance glyph. Failed
operations leave the list unchanged, though a successful intermediate text
layout may warm the font cache. Combined presentations need a caller-owned
combined character/atlas budget; append independently enforces the vertex limit.
