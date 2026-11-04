# Native UI layout and text

Enable `worldrunner`'s `ui` feature for CPU-only UI and render planning, or enable `ui` directly on `worldrunner-world-simulation` and `worldrunner-render-plan`. The feature does not require a GPU or managed runtime. With default features disabled, import components and fonts from `worldrunner::ui`, and extraction and `UiRenderPlan` from `worldrunner::ui::plan::ui`. `worldrunner_world_simulation::ui::contribution` owns the contribution types and extraction function; `render_plan::ui` re-exports them. Text contributions carry `worldrunner_world_simulation::presentation::CoverageTexture`. `World` owns Canvas, RectTransform and Text component state. `FontBook` owns font resources supplied as byte vectors, without filesystem access or an implicit default font.

## Layout contract

`World::ui_layout(viewport)` returns detached rectangle observations. Viewport dimensions must each be in 1..16384. Rectangle local coordinates are X-right and Y-up. Anchors and pivots are normalized to 0..1. The resolved size is the parent rectangle's size multiplied by the anchor span, plus `size_delta`. The pivot's position is the interpolated anchor point plus `anchored_position`. `RectTransform::with_offsets` converts offset pairs into these fields.

RectTransform owns local X/Y translation through its anchored position. LocalTransform supplies depth, rotation and scale. Ancestor matrices multiply in full, including shear produced by rotated nonuniform scale. A non-rectangle ancestor contributes its transform without supplying an anchor rectangle.

Screen-space overlay Canvas must be an object root. Layout derives its size, translation, rotation and scale from viewport, pivot and Canvas scale settings. Camera-space Canvas also must be a root. Its effective matrix is derived from a referenced camera object's rigid world transform, explicit orthographic half-height and positive plane distance. Camera scale and a camera inside a Canvas are unsupported and return an error. The camera's local +X/+Y axes span the plane and +Z points toward the Canvas plane; rotate this basis into the native world frame as needed.

Both screen modes ignore the serialized root transform for layout, including zero scale. Serialized values remain unchanged. Ordinary world transforms still describe those serialized values. World-space Canvas uses RectTransform positioning and authored rotation/scale, including zero scale. Descendant transforms never receive a zero-scale repair. A disabled Canvas or inactive object ancestor suppresses output for the subtree.

Constant scaling sets pixels per logical unit. Viewport matching uses the geometric interpolation of width and height ratios against a positive reference resolution. Recompute layout after viewport changes; there is no persistent layout cache to invalidate.

## Native pointer routing and controls

`InputAction::Pointer` carries physical host-pixel positions measured from the top left,
button edges, unit-preserving wheel observations and explicit cancellation. `InputView::pointer`
projects held/down/up buttons, position, separate line/pixel wheel sums and ordered events for
each fixed or dynamic frame. Edges, sums and events clear on that phase's next projection.
Nonfinite positions/deltas are rejected atomically by `World::push_input`. Focus loss projects
cancellation, clears held buttons and invalidates the position. Input while unfocused is ignored.

`RetainedInput::simulation_action` converts the existing window observations into this contract.
Queue the result with the host event timestamp, then route the dynamic view's ordered pointer
events once per frame. Alternatively route each host event directly; do not use both routes.
Host coordinates and viewport extents are already physical pixels on scaled displays. Do not
apply the OS scale a second time. The retained host emits `PointerCancelled` before synthetic
key/button releases on focus loss. Key bindings remain caller policy.

Keep one `UiRouter` for the interaction session and register controls by existing `ObjectHandle`
in `controls`. `UiControl::button` produces `UiAction::Activated`; `UiControl::choice` produces
`ChoiceSelected { group, index }` and updates `selected(group)`. No game callback is invoked.
Pass `pointer` a freshly resolved `World::ui_layout` for the current physical viewport. The router
converts top-left host positions to lower-left layout pixels once and tests the screen-space
affine rectangle, including rotation, shear and Canvas scale. World-space ray picking is outside
this contract. Registered controls supply hit surfaces; an unregistered rectangle or ordinary
CanvasGroup does not consume input. Groups modify descendant interaction without adding a surface.

`RectLayout::draw_order()` provides Canvas layer/order followed by `hierarchy_order`, a preorder
with siblings in `World::children` order. The router uses this tuple; rendering consumers must
use the same tuple to align visible overlap with hit testing. The last matching surface wins. The router applies enabled CanvasGroup interaction flags
from child to parent, stopping at `ignore_parent_groups`. A disabled control or a non-interactable
group cannot activate, but still consumes hits when blocking raycasts. Inactive objects and
disabled canvases have no resolved targets. Group alpha does not control interaction.

A primary press captures an eligible hit. Release activates once only if the same control is
still eligible and is hit; release outside remains consumed without activation. Cancellation
clears capture, pointer position, keyboard focus and held navigation keys. Read `UiResponse::consumed`
for every routed event and withhold consumed events from board/camera/scene interaction.

Register a Canvas or group object's rectangle in `modals` for modal behavior. The highest sorted
visible modal root consumes all pointer input, even outside its rectangle, and restricts pointer
and keyboard targets to its subtree. Removing or hiding it removes the restriction. Map host
keys to `UiKey::{Tab, Shift, Enter, Space}` and call `key` with press and repeat observations.
Tab/shift-Tab cycles enabled interactable controls in target order; Enter/Space activates on the
first key-down. Held/repeated key-downs never activate twice. Focus is revalidated at each key
event. Nested-modal focus restoration and visual focus styling are separate UI work.

## Text contract

Register owned TrueType SFNT bytes with `FontBook::register`, then use its typed `FontId` in a `TextRun`. Registration validates horizontal metrics, positive font height, character mapping, glyph offsets and nonempty outlines before publishing the font. This subset requires `glyf` and `loca` outlines; CFF, bitmap-only and other representations return `InvalidFont`. Authored zero advances and empty glyphs remain valid. Font identities are process-unique and never reused between books. Missing fonts, invalid bytes, missing glyphs and resource limits are distinct errors. There is no substitution for missing content.

The supported character set is printable ASCII, LF and CR. CR is ignored; LF starts a line. Other controls and all non-ASCII input return `UnsupportedShaping`. This implementation does not promise ligatures, bidirectional layout, combining marks or script shaping. With `HorizontalOverflow::Wrap`, a line that grows past the rectangle's width breaks before its last word and the space at the break is consumed; a word wider than the line breaks between characters (`ui/wrap.rs`). `HorizontalOverflow::Overflow` keeps each line whole. Font kerning applies within each line. Font size is pixels per em, as in Unity, converted to `ab_glyph`'s ascent-minus-descent scale through the font's units per em. Font ascent, descent, advance and line gap determine placement; line spacing multiplies the font height plus gap. Alignment applies to each line horizontally and to the entire block vertically. Empty strings produce no lines, glyphs or texture bytes.

Glyph output provides baseline origins and outward-rounded raster bounds separately. Local text coordinates start at the rectangle's bottom-left. `VerticalOverflow::Truncate` drops lines whose extent passes the rectangle's height; `VerticalOverflow::Overflow` keeps them. Glyphs are never clipped, so a kept line draws whole even outside the rectangle. With `rich_text` set, as by default, text containing `<` is rejected with `UnsupportedRichText`, because markup is not rendered. Rasterizer winding coverage is saturated to 1 before rounding to U8 and packed deterministically into a shelf atlas with a fixed configured row width. Atlas rows start at the top. Font storage, character count, atlas dimensions and atlas pixels are bounded. Render extraction applies character and pixel budgets across the complete frame as well as per text run.

`render_plan::ui::extract_ui` returns typed glyph quads and `CoverageTexture` values using linear R8 coverage. Quads carry matched UV orientation, tint, Canvas identity and sorting values. Text contributions are ordered by sorting layer, order and object identity. Overlay positions are viewport pixels; other modes produce native world positions. GPU upload, alpha sampling, composition and resource lifetime belong to the existing scene renderer, not this module.

Retained 3D hosts accept overlay text through `render_scene::simulation::UiOverlay`
and `scene_viewer::RetainedUpdate::Ui`. `SceneGpu` owns its dynamic quad resources;
the compositor blends them over the display output after all scene post-processing.
UI replacements advance retained generations without rebuilding scene resources.
World/camera canvases, graphic rectangles and additional text clipping are outside
this retained overlay contract. See the [host API](../../crates/scene-viewer/README.md#retained-screen-space-text).

## Font dependencies and fixtures

The optional rasterizer is `ab_glyph` 0.2.32, licensed Apache-2.0. Its font stack uses Apache-2.0 `ab_glyph_rasterizer` and `owned_ttf_parser`, and MIT OR Apache-2.0 `ttf-parser`. Tests generate a small SFNT font from synthetic triangle outlines and fixed metrics in Rust. No external font data or distribution notices are bundled.
