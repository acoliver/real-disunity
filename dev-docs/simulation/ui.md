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

## Display scale and text fields

The retained host publishes `RetainedInput::DisplayChanged { scale_factor, physical_size }`
at window creation, on `ScaleFactorChanged`, and before a changed `Resized` observation.
The scale event carries the window's current physical extent; the following resize supplies
its final extent if it differs. `RetainedHost::scale_factor()` and `size()` expose that state.
Windowless sessions start at scale 1 and expose `set_display_scale` for deterministic tests;
`resize` continues to accept physical pixels. Neither operation opens a window.

Set `router.layout.display_scale` from the host observation and resolve `ui_layout_with` again.
The rule multiplies screen Canvas scaling without rewriting authored components. Logical
rectangle sizes, text size and menu rules then share the resulting physical transform.
World-space canvases are unchanged. Pointer coordinates stay in physical pixels; dividing
or multiplying them by display scale would misalign hit testing. `ui_layout` without rules
continues to use scale 1. Viewport-matched Canvas scaling and explicit display scaling multiply,
so applications whose reference-resolution policy already accounts for display density leave
the rule at 1.

Register `UiControl::text_field()` in `controls` and a `UiTextField` in `text_fields`, under the
same existing object handle. Construct the field from a `TextRun`, a maximum character count
and its `FontBook`. The field uses the active font, plain single-line left-aligned text and
bounded character indices. Initial values and edits validate both supported shaping and
actual glyph availability. `TextReject::MaxLength`, `Font(UiError)` and `InvalidPreedit` distinguish
rejections; an edit is accepted whole or leaves the committed value unchanged. No character
substitution or partial insertion is performed.

`RetainedInput::Text(TextInput)` carries native `KeyEvent.text` commits and IME Enabled,
Preedit, Commit and Disabled observations. Key names never supply inserted characters.
Pressed key repeats deliver repeated commits, and releases deliver no commit. While composing,
keyboard text is suppressed in favour of the IME commit. Nonprinting keyboard control text
is handled through `UiKey` instead. `simulation_action` maps text to `InputAction::Text`;
each `InputView::text` contains ordered observations for its phase and clears on the next
projection. Focus loss cancels composition and unfocused simulation text is ignored.
Route text directly to `router.text`, or from that projection, once per event.

Map editing keys to `UiKey`: Left/Right, Home/End, Shift, Control/Alt, Backspace/Delete,
SelectAll, Enter and Escape. Shift extends selection; Control or Alt moves/deletes by
whitespace-delimited words. Map the platform's select-all shortcut to `SelectAll`.
Repeated movement and deletion are accepted. Space inserts only through committed text.
Enter emits `TextSubmitted`; Escape clears preedit and emits `TextCancelled` without reverting
the committed value or removing focus. The caller decides whether to revert, leave focus or
close its editor. `TextChanged` identifies accepted value changes; read `value`, `cursor` and
`selection` on the field. Ordinary control eligibility, Tab traversal, modal restrictions and
cancellation apply. Removing a registration disables its field's interaction.

After routing and relayout, pass `router.text_input_area(world, layout, fonts)` to the retained
host or event host's `set_text_input_area`. Do this after each focus, cursor, modal, layout or
scale change. `Some` enables native IME and positions its cursor area in physical top-left
pixels; `None` disables it. Focus loss disables it immediately. This explicit driver connection
also works before window creation and in the CPU session tests, without a real window.

Call `text_fields_plan` on a fresh extracted plan, before `style_plan`, with caller-authored
caret and selection colours. Fields render on overlay canvases through the existing glyph
and solid-quad contributions. Selection and caret share the field transform and clip, and
horizontal scrolling keeps the cursor visible. Preedit replaces the selected span visually
without changing `value`; commit inserts it once and cancellation discards it. Unsupported
preedit has the same typed rejection as committed input. Pointer dragging and clipboard are
not implemented. Native-window display migration and IME candidate-window placement remain
unverified; the Metal windowless test checks exact caret and selection pixels.

## Native accessibility

`UiRouter::semantic_tree(world, layout)` exposes a platform-free tree from the same freshly
resolved `UiLayout` used for input and rendering. Call it after routing input and resolving
again if scrolling changed. Nodes use `UiNodeId::Object(ObjectHandle)`, including the world's
identity and slot generation, so renaming a label or changing a value preserves identity and
replacing a destroyed object does not reuse it. `UiNodeId::Window` is the single host root.
Children follow the router's rectangle order. Bounds are axis-aligned `[left, top, right,
bottom]` in physical client pixels, derived from the rectangle's transformed screen corners.
Do not multiply by display scale. World-space rectangles have no screen target and are omitted.

Buttons, checkboxes, sliders, lists and list items receive roles from registered controls.
Rectangle containers receive group roles. Enabled `TextRun` components provide text nodes and
accessible names; descendant text also names its nearest registered control. Register
`router.semantics[object] = UiSemantics::label("Start")` to override a control's name explicitly.
`UiSemantics { label: None, live: true }` marks a text component as a polite status region.
An explicit label can supply status content without a text component. Text fields receive
text roles and focus actions; the tree does not advertise assistive text editing.

The tree reports router-owned toggle, slider and selection values, disabled state, supported
actions and keyboard focus. A slider reports its value, minimum, maximum and optional step.
Opening a modal includes only the highest visible modal's subtree below the window root.
Other controls are absent, including for assistive action requests. Tree extraction revalidates
focus and modal opener history even without an intervening input event; closing the dialog
restores its eligible opener under the same rules as keyboard and pointer routing.

`semantic_update` returns changed nodes, removed identities, current focus, a focus-change flag
and live announcements since that router's preceding update. Its first result is a full tree.
Publish every result in order to one host; `semantic_tree` can be inspected independently and
does not consume updates or announcements. Nonempty live text announces once when first
observed and once per subsequent text change. Geometry, focus, unrelated labels and hiding then
restoring unchanged live text do not repeat the announcement. Changes while hidden are observed
when that status becomes navigable again. The application must not announce the same update
through a second speech service.

Retained drivers pass each result to `RetainedHost::set_accessibility` or the event host's same
method. `accessibility_tree` exposes the current full tree without creating a window, GPU or
platform adapter. Desktop startup creates the winit window invisible, attaches AccessKit, then
shows the window. Native window events reach the adapter before the driver. A synchronized
snapshot answers initial-tree requests with a full tree; ordinary publication sends incremental
node updates and focus. These CPU services also operate during presentation pauses.

AccessKit requests arrive as `RetainedEvent::Accessibility(UiAccessRequest)`. Route them through
`router.accessibility_action(world, layout, request)` and process the returned `UiResponse`
through the same application action handling as pointer and keyboard responses. Press, focus,
toggle, selection, increment/decrement and finite numeric set-value requests use the router's
existing eligibility, activation, slider snapping and scroll-reveal helpers. Unsupported,
obsolete, foreign and modal-excluded targets do nothing. Native AccessKit click requests map to
press, including toggle and list selection; numeric set-value accepts either numeric data or a
parseable finite numeric string. No second control-state owner is created.

The opt-in viewer example opens a window only when launched explicitly with a caller-supplied
TrueType `glyf` font. It contains a button, toggle, slider, live status and modal dialog:

```sh
cargo run -p worldrunner-scene-viewer --example accessibility -- /path/to/font.ttf
```

For the macOS VoiceOver check, enable VoiceOver, navigate the controls, activate the button,
toggle sound, adjust volume, and open then close the dialog. Compare spoken focus with Tab
navigation, confirm that background controls disappear during the dialog, and confirm that
unchanged status text does not repeat. Resize on a scaled display and check hit targets and
accessible bounds. This check requires a display and is separate from CPU tests. The example
uses Escape to exit. The tests `ui_accessibility`, `ui_accessibility_edges` and retained
`accessibility::tests` exercise trees, shared actions, live updates and platform translation
without creating a display or requesting a graphics adapter.

### Accessibility dependencies

`scene-viewer` uses `accesskit` 0.24.0 (MIT OR Apache-2.0) and `accesskit_winit` 0.32.2
(Apache-2.0).
The adapter's declared winit requirement is 0.30.5, satisfied by the workspace's pinned 0.30.13.
Its enabled features are `rwh_06`, `accesskit_unix` and `async-io`; the workspace already selects
winit's window backends. macOS and Windows adapters are target dependencies. On Linux,
`accesskit_unix` uses AT-SPI through the Rust `zbus` stack, with no `libdbus-sys` dependency.
A working desktop accessibility bus is needed at runtime; no D-Bus C development package is
introduced for compilation. See the [adapter API](https://docs.rs/accesskit_winit/0.32.2/accesskit_winit/struct.Adapter.html)
for the window visibility order and activation contract.

## Text contract

Register owned TrueType SFNT bytes with `FontBook::register`, then use its typed `FontId` in a `TextRun`. Registration validates horizontal metrics, positive font height, character mapping, glyph offsets and nonempty outlines before publishing the font. This subset requires `glyf` and `loca` outlines; CFF, bitmap-only and other representations return `InvalidFont`. Authored zero advances and empty glyphs remain valid. Font identities are process-unique and never reused between books. Missing fonts, invalid bytes, missing glyphs and resource limits are distinct errors. There is no substitution for missing content.

The supported character set is printable ASCII (U+0020..U+007E), Latin-1 U+00A0..U+00FF, LF and CR. CR is ignored; LF starts a new paragraph. Other controls and characters return `UnsupportedShaping(character)`. A supported character absent from the supplied font returns `MissingGlyph(character)`, including spaces. Neither error substitutes or drops the character. This implementation does not provide ligatures, bidirectional layout, combining marks or script shaping. Latin-1 support requires the caller's font to contain the requested glyphs; the synthetic test font covers ASCII only.

With `HorizontalOverflow::Wrap`, a line that grows past the rectangle's width breaks before its last word and the space at the break is consumed; a word wider than the line breaks between characters (`ui/wrap.rs`). A single glyph wider than the rectangle occupies a line and is clipped at extraction. `HorizontalOverflow::Overflow` keeps each line whole. Font kerning applies within each line. Font size is pixels per em, converted to `ab_glyph`'s ascent-minus-descent scale through the font's units per em. `line_spacing` multiplies the font height plus gap. `line_height: Some(pixels)` overrides that baseline distance and must be finite and positive. `paragraph_spacing` adds finite nonnegative logical pixels after explicit newlines, including blank paragraphs. Wrapped continuations get only the baseline distance. Alignment applies to each line horizontally and to the entire visible block vertically. Empty strings produce zero content height and no lines, glyphs or texture bytes.

`TextRun::spans` contains ordered, nonempty, disjoint `TextSpan` UTF-8 byte ranges. Each range must end on character boundaries inside `text`. Invalid ranges or colours return `InvalidText`. A span's linear RGBA colour multiplies the run colour and inherited group alpha; unstyled glyphs use white. The source ranges survive wrapping, consumed break spaces and CR/LF. `GlyphLayout::color` and `UiGlyphQuad::color` carry the span tint through extraction, and both presentation paths multiply it by the contribution tint. This structured API accepts no markup. There are no bundled font faces or simulated bold or italic weights. A span can carry `link: Some(UiLinkId(value))`; plain colour spans use `None`. Link identities are caller-chosen and must be unique within one run. Duplicate identities return `InvalidText`. Both cached glyph layouts and extracted glyph quads retain the link identity. Link spans also produce solid underline quads through `UiRenderPlan::graphics`, one per line segment, using the span colour multiplied by the run tint and inherited alpha. Their thickness is one sixteenth of the font size, with a minimum of one logical pixel; their upper edge is one thickness below the baseline. Underlines share glyph pixel snapping and rectangle, renderer and scroll clipping.

Glyph output provides baseline origins and outward-rounded raster bounds separately. Local text coordinates start at the rectangle's bottom-left. `VerticalOverflow::Truncate` drops lines whose extent passes the rectangle's height; `VerticalOverflow::Overflow` lays them all out. `TextLayout::content_height` reports the full extent before truncation, including font height, baseline distances and paragraph spacing. Measure at the intended content width and use its rounded-up height in `UiSize::Fixed` or the content's `RectTransform` to size a `UiScrollView`; group content sizing does not measure text implicitly. Pixel-perfect snapping moves the glyph origins before extraction clips glyph geometry and matching UVs to the local rectangle, then intersects screen clip bounds with renderer and enclosing scroll clips. This also bounds rotated text at its local edges; an edge-clipped glyph's new corner can fall between pixels. With `rich_text` set, as by default, text containing `<` is rejected with `UnsupportedRichText`, because markup is not rendered. Rasterizer winding coverage is saturated to 1 before rounding to U8 and packed deterministically into a shelf atlas with a fixed configured row width. Atlas rows start at the top. Font storage, character count, atlas dimensions and atlas pixels are bounded. Render extraction applies character and pixel budgets across the complete frame as well as per text run.

Keep a `FontBook` across frames. `FontBook::layout` returns an immutable `Arc<TextLayout>` from a bounded cache keyed by the complete text run and rectangle size. Width, height, alignment, font, size, spans and paragraph settings therefore invalidate the layout; unrelated world or control mutations do not. Cached entries share glyphs and `GlyphAtlas::alpha`, and extraction and `UiPresentation` share those coverage bytes through `CoverageTexture::bytes`. A cache hit allocates no new glyph layout or atlas. The cache retains at most `UiLimits::max_glyphs` input characters and `max_atlas_pixels` coverage pixels, evicting oldest entries when necessary; even empty entries consume one character slot. Caller-held results remain valid after eviction. `cache_stats().layouts` counts successful misses and `atlas_builds` counts misses with nonempty coverage. Failed requests publish neither a cache entry nor a counter increment. Renderer uploads still belong to the existing compositor: publish UI replacements when the plan changes, rather than submitting an identical replacement each frame. The built-player host already performs this comparison.

`ui_text_flow` tests measure wrapping, paragraph height, alignment, source-range colours, typed glyph errors, scroll and rectangle clips, and shared layout/coverage pointers across 100 unrelated updates. The windowless scene-viewer paragraph test draws two coloured paragraphs, checks exact opaque RGBA regions on each line, and verifies unchanged scene pixels in the line gaps. A real-window reading check remains pending.

## Interactive paragraph links

Register the paragraph object with `UiControl::text()` and construct the router with
`UiRouter::with_fonts(Arc<FontBook>)`. Register fonts before sharing the book; pass the
same book to extraction. Routing measures linked text from the current run and rectangle
using that book's bounded cache, so text edits and resizing update targets without a separate
hit-region registration. A linked text control without a supplied book returns `MissingFont`.
Missing text on a registered text control returns `MissingLayout`.

Each link is one focus and capture target, even when it wraps. Its hit regions are its
outlined glyph rectangles, with the same snapping and clipping as extraction. Unlinked
glyphs, spaces without outlines and unused paragraph area do not consume hits. Scroll-clipped
links remain keyboard targets; glyphs clipped entirely by the paragraph's own rectangle or
removed by vertical truncation do not. CanvasGroup eligibility, draw order and modal blocking
apply as they do to other controls.

Tab and Shift-Tab traverse the links in span reading order at the paragraph's position in
the ordinary control order. `focused()` returns its paragraph object; `focused_link()` also
returns the typed link identity. Enter or Space activates on the first key-down. A primary
press and release on any two regions of the same link activates once; release on another
link does not. Both paths emit `UiAction::LinkActivated { control, link }`. Modal opener
restoration preserves the individual link identity. Removing the span invalidates its focus
and capture on the next event.

Supply individual state tints in `router.link_styles[(control, link)]`, or use the paragraph's
entry in `router.styles` for all its links. `style_plan` tints every glyph and underline of
that link with the ordinary disabled, pressed, focused, hovered, selected, normal priority.
Unlinked text keeps its authored colour. The engine supplies no link colour scheme.
Keyboard focus reveals the link's glyph bounds through enclosing scroll viewports,
innermost first. A link taller than a viewport reveals its reading start. Resolve and extract
again after navigation changes an offset, as for list items.

`ui_links` covers wrapped hits and state tints, activation edges, reading order, modal focus,
affine snapping, text and nested scroll clips, renderer clips, underline geometry and cache
identity changes. The windowless scene-viewer link test checks exact underline pixels through
the existing overlay graphics path. A real-window reading check remains pending.

## UI contributions

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
text and world-space graphics use the scene path. The live 2D text append includes
its generated underline graphics through `append_graphics` before glyph quads;
ordinary CanvasRenderer graphics retain their explicit extraction path.
Built-player retained drivers use the same compositor. The loaded ABI image retains
`UiPresentation` draws from `extract_ui` alongside its prepared scene frame, before
UI is converted to the live 2D path's world-space quads. Typed copy-out preserves
pixel coordinates, coverage, image identities, hierarchy order and clip bounds.
The driver refreshes image textures alongside mesh textures and replaces its UI
only when the draws, image generations or physical viewport change. See the
[host API](../../crates/scene-viewer/README.md#retained-screen-space-ui).

## Font dependencies and fixtures

The optional rasterizer is `ab_glyph` 0.2.32, licensed Apache-2.0. Its font stack uses Apache-2.0 `ab_glyph_rasterizer` and `owned_ttf_parser`, and MIT OR Apache-2.0 `ttf-parser`. Tests generate a small SFNT font from synthetic triangle outlines and fixed metrics in Rust. No external font data or distribution notices are bundled.
