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
navigation and activation keys to `UiKey` and call `key` with press and repeat observations.
Tab/shift-Tab cycles enabled interactable controls in target order; Enter/Space activates on the
first key-down. Held/repeated key-downs never activate twice. Focus is revalidated at each key
event. The router records each modal's opener when it first observes that modal. When the
modal is removed or hidden, the next routed event restores the opener if it remains eligible.
Nested closures restore each enclosing opener in turn. Cancellation clears this history.

## Menu groups, scrolling and selection

`UiLayoutRules` holds native layout registrations keyed by existing object handles. Supply it
to `World::ui_layout_with(viewport, rules)`. `UiRouter::layout` stores the same rules for an
interactive menu. These rules affect the existing rectangle resolution pass; they do not mutate
authored `RectTransform` components. `ui_extent` without rules continues to describe authored
rectangles. Remove registrations when destroying their objects; missing registered rectangles
return `MissingLayout`.

`UiLayoutGroup::Stack` lays out active direct rectangle children in hierarchy order, left to
right or top to bottom. Padding is `[left, right, top, bottom]`, and spacing separates adjacent
children only. Cross-axis alignment is start, center, end or stretch. Center alignment rounds
half-pixel spare space toward the start. Group positions and intrinsic child extents are whole
logical pixels; odd extents keep their fractional pivot translation so their edges stay whole.
Canvas scale and authored ancestor transforms still apply normally.

A child's default intrinsic extent is its nonnegative, rounded authored `size_delta`, independent
of its anchor span. `UiSize::Fixed` supplies an explicit extent. `UiSize::Content` measures a
nested group's children, spacing and padding recursively; a content-sized leaf without a group
returns `MissingLayout`. Text shaping is not an implicit sizing input. A caller can measure text
with its `FontBook` and supply the resulting fixed extent. `UiLayoutGroup::Grid` supplies cell
extent and two-axis spacing and fills a specified number of columns or rows before wrapping.
A zero column/row count returns `InvalidRect`. Groups accept at most 65535 active children.

Register `UiScrollView::new(content)` against a viewport rectangle in `rules.scrolls`. Content
must be a direct rectangle child. Its extent can be fixed or measured from a group. Its top-left
starts at the viewport's top-left; increasing the offset reveals content farther right/down.
`scroll_to(layout, viewport, offset)` rounds and clamps offsets to the nonnegative content extent
minus viewport extent. Resolution also bounds the effective offset, including after resize.
Call `scroll_to` with the new layout to normalize stored offsets after changing extents.

Scroll viewports require positive, axis-aligned screen-space overlay rectangles. Rotated,
sheared, mirrored, collapsed, camera-space or world-space viewports return `InvalidRect`;
ordinary non-scroll controls retain affine hit testing. Nested
viewport clips intersect and apply only to their content subtrees. Both text and graphics carry
the resulting clip into `UiRenderPlan`, intersected with any renderer clip. Pointer hit testing
uses the same bounds. Clipped controls remain keyboard targets so navigation can reveal them.
Wheel input over a viewport scrolls the highest ordered viewport under the pointer, including
empty viewport space. Pixel deltas convert to logical units using the viewport's screen scale;
line deltas use `UiScrollView::line_step` (20 logical pixels by default). Positive wheel deltas
move toward the start. A viewport consumes wheel input even at its boundary.

Register each list row with `UiControl::list_item(list, index)`. The list identity and unique
row indices are caller data. Pointer release, Enter/Space and Up/Down/Home/End all use the same
selection path, emit `UiAction::Selected { list, index }` and update `selected(list)`. Arrow
navigation follows ascending indices, skips disabled rows and stops at the ends. Navigation
accepts repeats; activation does not. Tab retains the ordinary control traversal. Keyboard
focus changes reveal the target through its enclosing scroll viewports, innermost first.

Register caller-authored `UiStyle` tints in `router.styles`, then call `style_plan(world, plan)`
once on a freshly extracted plan. It multiplies graphic vertex colours and text colour using
this priority: disabled, pressed, focused, hovered, selected, normal. Focus takes precedence over
hover so pointer position cannot conceal keyboard focus. The engine supplies no default colour
scheme; default tints are white. An enabled toggle uses the selected tint while on, subject
to the same state priority.

Resolve and extract again after routing events that can change an offset:

```rust,ignore
let layout = world.ui_layout_with(viewport, &router.layout)?;
let response = router.key(&world, &layout, UiKey::Down, true, false)?;
let layout = world.ui_layout_with(viewport, &router.layout)?;
let mut plan = contribution::extract_ui_layout(&world, &fonts, layout)?;
router.style_plan(&world, &mut plan)?;
```

The scrolled-list windowless overlay test checks exact RGBA bytes inside the viewport and
unchanged scene pixels outside it. A real-window resize check remains pending.

## Slider and toggle values

`UiControl::slider(UiSlider::new(min, max, step, value, axis)?)` registers a value control.
The range, initial value and optional step are `f64`. Construction returns
`WorldError::InvalidInput` for nonfinite values, `min >= max`, a nonfinite range span,
a nonpositive step or a step whose range-to-step ratio overflows. `None` gives continuous
pointer values. `Some(step)` snaps to the nearest step measured from `min`, with midpoint
ties toward the larger value. Values are clamped, and both range endpoints remain reachable
even when the span is not a multiple of the step. The initial value is clamped and snapped too.

The explicit `UiAxis` uses the track's local coordinates: horizontal increases rightward,
vertical increases upward. Affine transforms and Canvas scaling apply to both hit testing
and value mapping. A primary press on the track focuses and captures it and immediately
sets its value at that point. Captured movement continues outside the track and clamps at
the ends. Release ends capture without activating. Cancellation, loss of eligibility or a
modal change ends capture. Disabled controls still block hits under the ordinary raycast
rules, but neither focus nor change their values.

With a slider focused, Right/Up increases and Left/Down decreases by one step. Continuous
sliders use one percent of the range per arrow press. Home/End goes to the exact minimum
or maximum. Arrow and Home/End navigation accepts repeats; Enter/Space has no slider action.
`UiAction::ValueChanged { control, value }` is emitted only when the numeric value changes.
The router stores that current value; the game reads the action and owns persistence.
`router.slider(control)` returns the typed `UiSlider` observation. Its `value()`,
`normalized_value()` and `axis()` let the caller position a knob or size a fill child through
the existing rectangle and graphic APIs. The router does not author geometry or add a draw
path. Resolve and extract again after changing caller-authored rectangles.

`UiControl::toggle(on)` registers a boolean control. A primary press followed by release
inside toggles once, as does the first Space or Enter key-down while focused. Held and repeat
key-downs do not toggle again. It emits `UiAction::Toggled { control, on }` and stores the
new state in `UiControlKind::Toggle { on }`. Disabled toggles ignore activation and focus.
Both value controls share ordinary Tab order, CanvasGroup eligibility, modal restrictions
and caller-authored state tints with buttons and list items. The CPU tests in
`crates/world-simulation/tests/ui_values.rs` cover these input and contribution contracts.

## Text contract

Register owned TrueType SFNT bytes with `FontBook::register`, then use its typed `FontId` in a `TextRun`. Registration validates horizontal metrics, positive font height, character mapping, glyph offsets and nonempty outlines before publishing the font. This subset requires `glyf` and `loca` outlines; CFF, bitmap-only and other representations return `InvalidFont`. Authored zero advances and empty glyphs remain valid. Font identities are process-unique and never reused between books. Missing fonts, invalid bytes, missing glyphs and resource limits are distinct errors. There is no substitution for missing content.

The supported character set is printable ASCII, LF and CR. CR is ignored; LF starts a line. Other controls and all non-ASCII input return `UnsupportedShaping`. This implementation does not promise ligatures, bidirectional layout, combining marks or script shaping. With `HorizontalOverflow::Wrap`, a line that grows past the rectangle's width breaks before its last word and the space at the break is consumed; a word wider than the line breaks between characters (`ui/wrap.rs`). `HorizontalOverflow::Overflow` keeps each line whole. Font kerning applies within each line. Font size is pixels per em, as in Unity, converted to `ab_glyph`'s ascent-minus-descent scale through the font's units per em. Font ascent, descent, advance and line gap determine placement; line spacing multiplies the font height plus gap. Alignment applies to each line horizontally and to the entire block vertically. Empty strings produce no lines, glyphs or texture bytes.

Glyph output provides baseline origins and outward-rounded raster bounds separately. Local text coordinates start at the rectangle's bottom-left. `VerticalOverflow::Truncate` drops lines whose extent passes the rectangle's height; `VerticalOverflow::Overflow` keeps them. Glyphs are never clipped, so a kept line draws whole even outside the rectangle. With `rich_text` set, as by default, text containing `<` is rejected with `UnsupportedRichText`, because markup is not rendered. Rasterizer winding coverage is saturated to 1 before rounding to U8 and packed deterministically into a shelf atlas with a fixed configured row width. Atlas rows start at the top. Font storage, character count, atlas dimensions and atlas pixels are bounded. Render extraction applies character and pixel budgets across the complete frame as well as per text run.

`render_plan::ui::extract_ui` returns typed glyph quads and `CoverageTexture` values using linear R8 coverage. Quads carry matched UV orientation, tint, Canvas identity and sorting values. Text and graphic contributions carry the sorting layer, order and hierarchy preorder from `RectLayout::draw_order()`. Each list is sorted by that tuple; the retained compositor interleaves both lists using the same tuple. Overlay positions are viewport pixels; other modes produce native world positions. GPU upload, alpha sampling, composition and resource lifetime belong to the existing scene renderer, not this module.

Retained 3D hosts accept overlay text and screen-space graphics through
`render_scene::simulation::UiOverlay` and `scene_viewer::RetainedUpdate::Ui`.
`UiRenderPlan::graphics` wraps the existing `extract_graphics` output with its
hierarchy key and clip bounds. Solid and translucent quads need no texture;
`UiOverlay::with_textures` resolves image texture identities from an explicitly
supplied `SceneTexture` map. Unresolved identities return an error.

`SceneGpu` owns the dynamic resources. The existing compositor blends text and
graphics together over the display output after all scene post-processing, in
ascending `RectLayout::draw_order()` with graphics before text on the same object.
UI replacements advance retained generations without rebuilding scene resources.
Each contribution carries lower-left pixel clip bounds intersected with its
canvas viewport and any explicit `CanvasRenderer::clip_rect`. The latter is
measured in root-canvas coordinates and also applies to text on that object.
Bounds round inward to whole-pixel scissors, which intersect the current render
extent. This bounds rotated graphics even when CPU quad clipping keeps them whole.
Soft clipping and stencil/pop-material masks remain future work. Camera/world
text and world-space graphics use the scene path. The live 2D path is unchanged.
Built-player retained drivers use the same compositor. The loaded ABI image retains
`UiPresentation` draws from `extract_ui` alongside its prepared scene frame, before
UI is converted to the live 2D path's world-space quads. Typed copy-out preserves
pixel coordinates, coverage, image identities, hierarchy order and clip bounds.
The detached draw sequence interleaves graphics and glyphs in ascending canvas
sorting layer, canvas sorting order and hierarchy preorder. Graphics precede text
only on the same object; glyphs and mesh quads retain their original order within
that object. Canvas creation order does not override its sorting values.
The ABI rejects descending draw keys and clip extents whose unsigned endpoints
overflow before copying metadata. Empty clip rectangles are valid and draw nothing.
The driver refreshes image textures alongside mesh textures and replaces its UI
only when the draws, image generations or physical viewport change. See the
[host API](../../crates/scene-viewer/README.md#retained-screen-space-ui).

## Font dependencies and fixtures

The optional rasterizer is `ab_glyph` 0.2.32, licensed Apache-2.0. Its font stack uses Apache-2.0 `ab_glyph_rasterizer` and `owned_ttf_parser`, and MIT OR Apache-2.0 `ttf-parser`. Tests generate a small SFNT font from synthetic triangle outlines and fixed metrics in Rust. No external font data or distribution notices are bundled.
