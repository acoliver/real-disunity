# Native UI layout and text

Enable `worldrunner`'s `ui` feature for CPU-only UI and render planning, or enable `ui` directly on `worldrunner-world-simulation` and `worldrunner-render-plan`. The feature does not require a GPU or managed runtime. With default features disabled, import components and fonts from `worldrunner::ui`, extraction and `UiRenderPlan` from `worldrunner::ui::plan::ui`, and `SceneTexture` from `worldrunner::ui::plan`. `World` owns Canvas, RectTransform and Text component state. `FontBook` owns font resources supplied as byte vectors, without filesystem access or an implicit default font.

## Layout contract

`World::ui_layout(viewport)` returns detached rectangle observations. Viewport dimensions must each be in 1..16384. Rectangle local coordinates are X-right and Y-up. Anchors and pivots are normalized to 0..1. The resolved size is the parent rectangle's size multiplied by the anchor span, plus `size_delta`. The pivot's position is the interpolated anchor point plus `anchored_position`. `RectTransform::with_offsets` converts offset pairs into these fields.

RectTransform owns local X/Y translation through its anchored position. LocalTransform supplies depth, rotation and scale. Ancestor matrices multiply in full, including shear produced by rotated nonuniform scale. A non-rectangle ancestor contributes its transform without supplying an anchor rectangle.

Screen-space overlay Canvas must be an object root. Layout derives its size, translation, rotation and scale from viewport, pivot and Canvas scale settings. Camera-space Canvas also must be a root. Its effective matrix is derived from a referenced camera object's rigid world transform, explicit orthographic half-height and positive plane distance. Camera scale and a camera inside a Canvas are unsupported and return an error. The camera's local +X/+Y axes span the plane and +Z points toward the Canvas plane; rotate this basis into the native world frame as needed.

Both screen modes ignore the serialized root transform for layout, including zero scale. Serialized values remain unchanged. Ordinary world transforms still describe those serialized values. World-space Canvas uses RectTransform positioning and authored rotation/scale, including zero scale. Descendant transforms never receive a zero-scale repair. A disabled Canvas or inactive object ancestor suppresses output for the subtree.

Constant scaling sets pixels per logical unit. Viewport matching uses the geometric interpolation of width and height ratios against a positive reference resolution. Recompute layout after viewport changes; there is no persistent layout cache to invalidate.

## Text contract

Register owned TrueType SFNT bytes with `FontBook::register`, then use its typed `FontId` in a `TextRun`. Registration validates horizontal metrics, positive font height, character mapping, glyph offsets and nonempty outlines before publishing the font. This subset requires `glyf` and `loca` outlines; CFF, bitmap-only and other representations return `InvalidFont`. Authored zero advances and empty glyphs remain valid. Font identities are process-unique and never reused between books. Missing fonts, invalid bytes, missing glyphs and resource limits are distinct errors. There is no substitution for missing content.

The supported character set is printable ASCII, LF and CR. CR is ignored; LF starts a line. Other controls and all non-ASCII input return `UnsupportedShaping`. This implementation does not promise ligatures, bidirectional layout, combining marks or script shaping. Wrapping is by character, with font kerning applied within each line. Font size uses `ab_glyph` pixel-height scaling. Font ascent, descent, advance and line gap determine placement; line spacing multiplies the font height plus gap. Alignment applies to each line horizontally and to the entire block vertically. Empty strings produce no lines, glyphs or texture bytes.

Glyph output provides baseline origins and outward-rounded raster bounds separately. Local text coordinates start at the rectangle's bottom-left. With overflow disabled, raster bounds are clipped inward to whole pixels inside the rectangle. Rasterizer winding coverage is saturated to 1 before rounding to U8 and packed deterministically into a shelf atlas with a fixed configured row width. Atlas rows start at the top. Font storage, character count, atlas dimensions and atlas pixels are bounded. Render extraction applies character and pixel budgets across the complete frame as well as per text run.

`render_plan::ui::extract_ui` returns typed glyph quads and existing `SceneTexture::Texels` descriptors using linear R8 coverage. Quads carry matched UV orientation, tint, Canvas identity and sorting values. Text contributions are ordered by sorting layer, order and object identity. Overlay positions are viewport pixels; other modes produce native world positions. GPU upload, alpha sampling, composition and resource lifetime belong to the existing scene renderer, not this module.

## Font dependencies and fixtures

The optional rasterizer is `ab_glyph` 0.2.32, licensed Apache-2.0. Its font stack uses Apache-2.0 `ab_glyph_rasterizer` and `owned_ttf_parser`, and MIT OR Apache-2.0 `ttf-parser`. Tests generate a small SFNT font from synthetic triangle outlines and fixed metrics in Rust. No external font data or distribution notices are bundled.
