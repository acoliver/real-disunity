# Configured post and output

`PostChain<F: StageFactory>` is a retained, **linear** post/output service. It is not a general render graph and does not submit commands or acquire surfaces.

- `FrameConfig` specifies the scene extent, color format and sample count. `FrameTargets` retains the resolved color texture and optional multisample texture. Use `scene()` and `resolve()` together for the scene pass; subsequent stages always bind `resolved()`.
- `Resources::new` allocates opaque, explicitly sized caller-authored buffers. Move these into the chain for exposure/history/config ownership. Neither resize nor reconfiguration clears or reallocates them. The provider defines their binding layouts and initialization bytes; the service does not infer or reinterpret an exposure ABI.
- `StageFactory::build` creates bind groups and returns an ordered `StageSet`. It is called on construction, resize and reconfiguration, **not** once per frame. Move sized textures and buffers into `StageSet::resources`; pipeline and sampler handles can remain in the factory. Failed resize/reconfiguration leaves the previous target set and schedule intact.
- `Stage` describes a clear, buffer copy, compute dispatch or fullscreen draw. Bind groups retain the provider's explicit layouts and ascending group order. An optional stage can be absent or `enabled: false`. The provider is responsible for supplying a valid bypass/input when an effect is disabled. The service validates one final enabled output and bounded compute dispatches before accepting a schedule.
- `encode` executes the configured order against a supplied surface or offscreen output view. It does not choose exposure, bloom, tone curves, LUTs, adaptation rules or antialiasing. Providers must declare those stages and dependencies explicitly.
- `CopyOutput` is the built-in **no-effects** factory: nearest texel copy, including scaling and destination attachment transfer/channel encoding. It does not introduce a display look.
- `color_texture` allocates common intermediate/output textures. `Readback` retains bounded staging storage, verifies the source contract, strips padding and returns native bytes (RGBA/BGRA/HDR are not silently converted). Mapping has a shared 30-second deadline and unmaps on returned errors.

`CopyOutput::source` binds one sampled `AttachmentView` for a feature drawing
inside an executor-owned pass. It returns a retained `CopySource`; call `record`
inside that pass and keep the source's sampled-read declaration in the frame
plan. The pass must have one single-sample colour target in the factory's output
format and no depth attachment. Sources must be single-sample float colour
subresources with `TEXTURE_BINDING` usage. The copy uses nearest scaling, decodes
sRGB input through the sampled view, and leaves output transfer to the target
format. `CopyOutput::bottom_first` inverts the sampled row after selecting the
nearest texel. The default factory preserves row order. Rebinding a source reuses
the pipeline. Recording the draw does not submit commands or read back pixels.

Minimal copy-only presentation:

```rust,ignore
let mut output = PostChain::new(
    device,
    FrameConfig::new([width, height], scene_format, samples),
    Resources::default(),
    CopyOutput::new(device, surface_format),
    SceneDepth::Independent,
)?;
// Render using output.targets().scene() and output.targets().resolve().
output.encode(encoder, surface_view);
output.resize(device, [new_width, new_height])?;
```

For an authored effect, implement `StageFactory`; compile custom shaders with `fullscreen_pipeline` / `compute_pipeline` and construct explicit bind groups in `build`. For example, return histogram clear, histogram dispatch, exposure dispatch, bloom draws, then an output draw. A diagnostic exposure override is an in-order `CopyBuffer` stage, not a queue write racing the compute schedule. Updating per-frame uniforms remains a caller responsibility, before submission. An output format change requires a correspondingly configured factory/pipeline; device/surface lifecycle and capture publication remain host responsibilities.

Authored diagnostics follow `StageId`, independently of timestamp-query support.
`Stage::encoder_label()` supplies the cached name to render and compute descriptors:
`name` at level zero, `name level N` otherwise. Schedule acceptance refreshes
`Stage::label` from its identity, so a separate descriptive label cannot override
the timing name. `PassId` display uses the same spelling in timing reports.
Typed raster/compute/transfer lists cache their full operation identity, including
level and occurrence, when compiled. Encoding borrows retained labels without
formatting allocation; wgpu still performs its own backend label conversions.

The required GPU tests exercise exact copy, HDR/MSAA resolve, nonaligned readback, optional output changes, disabled-stage preservation, history across resize/reconfiguration and typed invalid configurations. They use native PRIMARY backends by default and honor `WGPU_BACKEND`; adapter absence fails rather than skips.

## Ordered host captures

For a capture inside an existing frame encoder, construct `Readback` with the **actual source format**, extent, sample count 1, and an explicit byte ceiling. Call `encode(encoder, source)` at the capture point, submit that encoder, then call `read_encoded(device, submission)`; `None` waits for all submitted work. `read(device, queue, source)` remains the convenience copy/submit/read path. A mutable borrow prevents overlapping maps through this owner. The caller must use the creating device and submit encoded copies before reading.

Supported native formats are RGBA8/BGRA8 linear and sRGB, RGBA16Float, R32Float and RGBA32Float. Wrong-format, multisampled, array, non-D2, wrong-size and missing-COPY_SRC sources are rejected before copy encoding. Floating-point data is never interpreted or converted: NaN payloads, infinities and signed zeros survive byte-for-byte. `read_encoded` strips the row padding of the GPU copy.

`BufferReadback` provides the same allocation ceiling, ordered encode, shared 30-second deadline and guaranteed unmap-on-return for opaque diagnostic buffers. Its configured byte count must be nonzero and copy-aligned; encoding copies exactly that count from source offset zero. The provider, not the SDK, interprets exposure/statistics layouts. Capture publication, metadata, float decoding and surface BGRA-to-RGBA packing remain host responsibilities.

## Geometry pass list

A frozen `ContourIds` entry must follow every Opaque occurrence, including when a
prepass already produced its read-only depth attachment. An earlier contour entry
is refused with `FrameError::StockOrder` naming both offending entries. CPU
coverage is in `src/frame/frozen_tests.rs`.

`PostChain::encode_geometry` takes a `FramePlan` whose `passes` slice is the frame's authored order. `validate_frame` walks that slice front to back and the executor encodes it in the same order; neither sorts it, so the `PassSlot` enum's declaration order carries no meaning. A pass may sample or load only a resource an earlier entry stored, and a read whose producer is listed after it fails as `FrameError::MissingProducer` naming the resource. `PostChain::encode_frozen` encodes a plan the same way and first refuses, before any feature prepares, an order the stock passes need that the plan breaks: `FrameError::StockOrder` or `FrameError::IncompleteDepth`.

Each pass is one *entry*, `PassEntry { slot, occurrence }`. `PassSlot` is the role, shown in labels as `Geometry(role)`. Timing and captures name the entry: `PassId::Geometry(entry)` is `Geometry(Opaque)` for occurrence 0, exactly as before, and `Geometry(Opaque#1)` for the next appearance. `DrawContext` carries both `slot` (the role) and `entry`, so a feature declared in two entries of one role can tell them apart. The occurrence tells repeated appearances of one role apart: `PassDeclaration::new(slot)` and `FeaturePass::new(slot)` are occurrence 0, and `PassDeclaration::nth(slot, n)` / `FeaturePass::nth(slot, n)` name a later one. A feature draws only in the entry its `FeaturePass` names. `FrameError::AttachmentContract(entry)` names the entry that broke its contract. `FrameError::DuplicatePass(entry)` refuses a list that contains an entry twice, and `FrameError::MissingPass(entry)` refuses a feature that names an entry the list does not contain. Attachment, version, discard and transparency rules are unchanged. A geometry prefix that was encoded earlier in the same frame is recorded per entry (`PrepareContext::completed_in_prefix(entry)`), and the suffix skips exactly those entries. Tests: `frame_tests` and `tests/repeated_roles.rs`.

Transparent entries (`TransparentWater` and `TransparentAlpha`) follow the same authored order, so water and alpha features in separate entries, or two water bodies in `TransparentWater` and `TransparentWater#1`, draw in the order the frame lists them. Each transparent entry holds at most one transparent feature: the executor does not sort packets across features inside one entry, and a second feature there fails as `FrameError::TransparentOrdering(entry)`. A later water entry refracts the snapshot taken before it, so listing another `AfterOpaqueSnapshot` between two water entries is how the second shows the first.

`FramePlan::operations` positions work that records commands of its own among those passes. An `OperationsGroup { before, work }` names the entry it runs immediately before (`None` runs it after every pass), and `work` is a `FrameOperations`, whose `encode_operations(timing, encoder)` the executor calls with the frame's `TimingState`, so a group that encodes a checked operations list through it still lists each operation as `PassId::Operation`. A group before an entry the plan does not author is `FrameError::MissingPass`, refused before anything is prepared or encoded. A group runs before the first pass at or after its position that the call encodes: when a geometry prefix already encoded that entry, the group runs at the head of the suffix, and a group meant to run with the prefix belongs in the prefix plan. Groups at one position run in plan order. `render-post` does not depend on the provider of the work; `worldrunner-render-program` implements `FrameOperations` for `AdmittedOffscreen`. Tests: `tests/operations_groups.rs` and `timing_operations_tests`.

A group may also leave textures for the passes after it. `FrameOperations::produces` returns the `OperationsOutput`s (resource, format and size of one mip and layer) the work leaves written; the default returns none. Validation walks the passes in order and, before it validates pass `i`, adds the outputs of every group positioned at or before `i` to the resources that have a producer. A feature's declared read and a read-only depth `Load` of an output therefore validate after the group, and the same read before the group's position, after a `None` group, with no group, or at another `ResourceRef::version` is `FrameError::MissingProducer`. The edge is one way: a geometry pass that attaches an output as anything but a read-only depth (a clear, a store, a load that stores back) is `FrameError::OffscreenWrite { pass, resource }`. The host hands the executor each output as an `AttachmentView` marked `with_retained_contents`, since the geometry registry imports the texture itself; an output with no such view, or whose format or size differs from the view's, is `FrameError::AttachmentView`. A retained view that a group outputs has no producer until the group's position, where a retained view that no group outputs has one from the start. `AdmittedOffscreen` in `worldrunner-render-program` reports the textures the host exported with `OffscreenOperations::export` before admission.

### Game places in the pass list

`PassSlot::Game(GamePlace)` lets a host add its own passes to the authored list without a second scheduler. `GamePlace::AfterDepthPrepass`, `BeforeOpaque`, `AfterOpaque`, `AfterOpaqueSnapshots` and `AfterTransparent` name where a producer of the stock forward frame places them; `AfterOpaque` runs before the contour IDs and the opaque snapshots, so the snapshots include what it drew, and `AfterOpaqueSnapshots` runs after the snapshot passes and before the water and every other transparent pass, so a pass there samples the snapshots and draws over the opaque scene ahead of the transparents; entries of one place run in occurrence order (`PassEntry::nth(PassSlot::Game(place), n)`). They are ordinary entries: the same attachment, `MissingProducer` and `DuplicatePass` rules validate them, and an entry that reads a target no earlier entry stored is refused by that target's name before any feature prepares. `PrepareContext::passes()` and `PrepareContext::pass(entry)` give a feature the validated declaration of the entry it records into.

A feature's `prepare` stages transfers and compute work on its `PrepareContext` without reaching the frame's encoder. `copy_buffer` and `copy_buffer_region` stage a buffer-to-buffer copy, `upload_buffer` stages bytes written into the frame authority's reusable staging, `dispatch` stages one compute pass, and `copy_texture(source, destination, size)` stages a texture-to-texture copy between `wgpu::TexelCopyTextureInfo` regions (any mip, origin and array layer, whole blocks for a compressed format). `finish` encodes everything staged into the frame's encoder in staging order, after every preparation has succeeded and before the frame's passes record, so a dispatch sees the copies staged before it and a copy sees the output of a dispatch staged before it. A cancelled frame, or one whose preparation fails, encodes none of it. A caller that keeps a result across frames, such as a resident array filled by a staged copy, marks it ready from `PrepareContext::publish`, which runs only after the frame submits, so a dropped frame leaves it to be staged again. Tests: `prepare_context_tests` (buffer copies, upload and dispatch order) and `prepare_texture_copy_tests` (a texture copy lands before a dispatch that samples it, a copy staged after a dispatch copies its output, mip, origin and layer addressing, and a cancelled or failed frame copies nothing).

```rust,ignore
// In a feature's `plan`: one pass before Opaque that writes `target a`, one that samples it.
let writer = PassEntry::nth(PassSlot::Game(GamePlace::BeforeOpaque), 0);
let reader = PassEntry::nth(PassSlot::Game(GamePlace::BeforeOpaque), 1);
// In `prepare`: the exact attachments of the entry being recorded.
let declaration = context.pass(writer)?;
```

`GamePlace::AfterDepthPrepass` passes belong to the depth prefix: they load and write the prepass's depth and colour products, so the depth pyramid, the occlusion metadata, SSAO and Opaque read what they added. A frozen plan is held to that. Such a pass must follow the `DepthNormalPrepass` entry, and the `DepthPyramid`, `OcclusionMetadata`, `Ssao` stages and `Opaque` must follow it (`FrameError::StockOrder`). The last prefix entry (prepass or game pass after it) that writes a depth completes it: a read of Opaque's depth before that entry is `FrameError::IncompleteDepth`.

A route that submits the depth prefix ahead of the frame (`PostChain::encode_geometry_prefix`) runs these passes inside it. The chain holds the features that draw them as prefix participants, registered with `PostChain::register_prefix`. Before encoding a prefix, the caller plans them into the prefix's pass list with `PostChain::plan_prefix_features` and orders the list. `encode_geometry_prefix` then prepares and draws only those participants beside the borrowed prefix features, parks every other retained feature, and restores them in registration order. In the frame that completes the prefix, a participant whose declared entries the prefix completed is neither prepared, committed nor drawn again. A participant whose entries the prefix did not all complete runs as usual, as it does in every frame drawn whole. A participant that plans no pass into a prefix takes no part in it: `plan_prefix_features` keeps no declaration for it, so the prefix parks it like any other retained feature, and the frame prepares, commits and finishes it as usual, as it does in a frame without a prefix. A caller that cannot encode the prefix it planned (its list cannot be ordered, or preparing its own features fails) drops the kept declarations with `PostChain::discard_prefix_plan`, so no later prefix encodes with them; `has_prefix_plan` reports whether any are kept. Tests: `frame/prefix_participant_tests.rs` and, in `render-scene`, `tests/game_passes/prefix_idle.rs` and `src/compositor/game/inputs_tests.rs`.

A game pass that samples a stock snapshot declares the read in its `FeaturePass::reads` (`opaque snapshot color`, `opaque snapshot depth` and `opaque snapshot device depth`), which the frame validates like any other read: the snapshot writers must come earlier in the ordered list, so a reader at `AfterOpaqueSnapshots` validates and one at `BeforeOpaque` or `AfterOpaque` is `FrameError::MissingProducer` naming the snapshot. The depth snapshot pass writes the linear and the device depth as its two colour attachments; none of the three snapshot names may be written twice in a frame (`FrameError::AttachmentHazard`).

`render-scene` documents the placement helpers a game calls. Tests: `a_context_reports_the_declaration_of_each_entry_the_frame_opens`, `frame/frozen_prefix_tests.rs`, `frame/game_tests.rs` and the forward-order tests in `render-scene`.

### Prepared commands

A feature records nothing through the frame's encoder: in `prepare` it stages copies, dispatches and uploads, and `PrepareContext::finish` records them in staging order once every feature has prepared, before the first pass the call encodes. `PrepareContext::record_prepared(label, access, commands)` stages work a provider prepared in full, such as `worldrunner-render-program`'s `PreparedDispatch` and `PreparedBlockTransfer`, as a `Box<dyn PreparedCommands>`: the context calls `retain` when it stages the commands and `record(encoder, label)` when it finishes, so a dispatch reads constants uploaded before it and a later pass reads what it wrote. Compute work staged one after another, `dispatch` calls and prepared commands whose `compute()` gives a `PreparedCompute` (such as `PreparedDispatch`), is recorded into one compute pass labelled "prepared compute", through `record_in_pass(pass, label)` for prepared commands; a copy, an upload or prepared commands without a compute form (such as `PreparedBlockTransfer`) end the pass, and staging order is kept exactly. wgpu-core encodes every compute pass into two command buffers that it holds until the frame submits, and Metal's queue refuses the one after 4096, so a pass for each dispatch limited what one frame could prepare; inside one pass wgpu-core gives each dispatch its own usage scope and the barriers it needs after the dispatches before it, so a dispatch reads what the one before it wrote. Each staged item names itself `label` in GPU captures: inside the shared pass as a pass debug group, otherwise as the label of a pass it opens or as an encoder debug group around commands outside a pass (a debug group pushed on the encoder around a pass would cost a command buffer of its own). An item in the shared pass sets every bind group its pipeline uses, because the groups an earlier item set stay bound. `src/prepared_command_buffers_tests.rs` pins that a run of staged dispatches costs what one compute pass costs and that a page of runs and transfers costs its passes and the command buffers its transfers reopen; `src/prepared_run_tests.rs` reads back a chain of dispatches in one run that each read the storage image and buffer the one before wrote, a transfer between two dispatches, and a refusal inside a run named by its label. A refusal by the commands' owner, of the retention or of the recording, is `FrameError::PreparedRefused { label, refusal }`, where `refusal.downcast_ref()` gives back the owner's own error, such as render-program's `ProgramError`; a refused recording fails the frame.

`PrepareContext::dispatch(label, pipeline, group, workgroups)` and `record_prepared` both record in staging order; they differ in what the frame knows. `dispatch` stages a pipeline and bind group as they are: the frame checks no access, versions no write and holds nothing, so it is for a feature's own compute work over buffers and textures that are not views of the frame and that the feature keeps alive. Work that writes a view of the frame, reads a retained view, or uses resources the frame must hold until it completes (a program runtime's reservations, a lender's owners) goes through `record_prepared`.

The views such commands write are declared by the plan with `AttachmentView::with_prepared_contents`, and named in `PreparedAccess::writes`. Validation counts a prepared view as produced before the first pass, so passes may read it, and a pass that clears it or an operations group that writes it is `FrameError::AttachmentHazard`. The stock compile versions each prepared write as a copy write before the first pass, so the texture needs `COPY_DST`, and a capture that submits the frame checks it owns each written allocation before any feature commits. The flag declares no earlier contents: the texture is imported at the same initial version as without it. Staging must agree with the plan and with the commands: `PreparedCommands::writes` reports the subresources the commands write (a block transfer its destination layer, a dispatch its storage images), and when the views of the frame among them are not the ones `PreparedAccess::writes` names, staging is `PreparedWriteMismatch { label, declared, written }`; a write of a texture no view of the frame shows is the provider's own and is not compared, and a write of a layer of a frame texture the frame has no view of is named like that texture's views. An undeclared view is `AttachmentView`, a view the plan did not mark is `UnplannedWrite`, a view staged twice is `RepeatedPreparedWrite`, a planned write no feature staged refuses the frame with `UnstagedWrite`, a read (`PreparedAccess::reads`) must be of a retained view, and staging outside `prepare`, in `commit` or `finish_draws`, is `PreparedOutsidePreparation`. Nothing is staged on a refusal. Scratch images and staging buffers the provider owns are not views of the frame and are not declared. Tests: `prepare_context_tests`, `contracts_prepared_writes`, and `tests/game_passes` in `render-scene`.

### Compute passes

Work a feature prepares in `prepare` and stages with `record_prepared` runs before the frame's first pass. Compute work that reads what a pass of the frame writes, and that a later pass reads, runs in a compute pass of the pass list instead. `PassDeclaration::compute_pass(slot, occurrence, writes)` declares one: an entry with no attachments whose `compute` lists the `StorageWrite`s (resource, format, extent) it makes through storage images. It is an entry like any other, so it shares its role's occurrences with the render passes of the role (`DuplicatePass` for a collision), and it is validated in its place in the list: the reads its features declare in `FeaturePass::reads` need an earlier producer (`MissingProducer`, `LoadAfterDiscard`), a read of one of its own writes is `AttachmentHazard`, a write named twice or of a stock product written once per frame (the snapshots, the post-water depth) is `AttachmentHazard`, an attachment of the pass, an empty write or a feature that declares attachment formats in it is `AttachmentContract`, a write an operations group makes is `OffscreenWrite`, and each write needs a single-sampled view of a `STORAGE_BINDING` texture with its format and extent (`AttachmentView`). After the pass its writes are producers, as an attachment that stores is, and the stock compile versions them as storage writes, so the capture the frame belongs to owns them before the pass is encoded.

A feature stages the work in `prepare` with `PrepareContext::record_in_compute_pass(entry, label, access, commands)`, for commands with a compute form (such as render-program's `PreparedDispatch`); `access` names what they read and write among the frame's views. A write must be one the pass declares (`UnplannedWrite`), named once in `access` (`RepeatedPreparedWrite`); several commands of the pass may write one subresource. A read must be one the pass's features declared, or a subresource commands staged earlier in the same pass write, since each dispatch of a compute pass reads what the ones before it wrote (`MissingProducer` otherwise, `AttachmentHazard` for a read the same commands write), and the views the commands report writing must be the ones `access` names (`PreparedWriteMismatch`). An entry that is a render pass, or a compute pass a completed geometry prefix encoded, is `NotComputePass { label, entry }`, an entry the plan lacks `MissingPass`, commands without a compute form `NotComputeWork { label, entry }`, and a declared write no commands staged is `UnstagedWrite` when preparation closes. The executor opens one compute pass at the entry's place, labelled and timed as `PassId::Geometry(entry)`, after the operations groups positioned before it, and records the staged commands in staging order with `PreparedCompute::record_in_pass`, as the preparation's run of compute work is recorded. Tests: `src/frame/compute_tests.rs` (validation on a no-op device, and a chain on it that records a render pass, the staged compute work and a render pass in that order, with every staging refusal).

## Typed operations

`contracts::TypedOperation` lists work for the executor: raster passes with checked draw packets (`TypedWork::Draws`), fullscreen draws, compute, resolves and copies. `ResourceRegistry::compile_operations` checks the whole list before anything is encoded: every operation declares its texture and buffer uses, each write takes the next version of the subresource it writes, and each read names the version it reads. A read without a producer, a read of a stale version, overlapping incompatible access, a missing usage flag and a stale or leased-out allocation are `ContractError`s. `CompiledOperations::encode_checked` encodes the list in order and `PassId::Operation` names each operation in the frame's timing.

Four additions serve a provider that draws with pipelines it did not author here, such as `worldrunner-render-program`'s offscreen work:

- **`CheckedDraw::rebind(bindings, geometry)`** makes another draw of a draw's pipeline. `CheckedDraw::with_geometry` creates the pipeline every time, which a provider that draws hundreds of meshes with a few programs cannot afford. The new bind groups must have the same layout entries in the same order, the geometry the same number of vertex streams, and a strip topology keeps requiring the pipeline's index format. Ranges and counts are checked as for a new draw. A mismatch is `ContractError::Pipeline`, a range past a buffer `ContractError::Range`. Tests: `operations_rebind_tests`.
- **`TypedWork::Fullscreen { vertices: Some(FullscreenVertices { streams, count }) }`** draws `count` vertices reading the streams, stream `n` in vertex slot `n`. A fullscreen pipeline whose vertex stage has inputs could not be used before, because the operation drew three vertices with no vertex buffer. Each `VertexStream` gives its view, stride and the extent one vertex reads. Streams are checked like those of a draw packet: a vertex buffer usage, four byte alignment, `count` vertices inside the slice (`ContractError::Range`), and a declared `BufferUseKind::Vertex` read in the operation's `buffers` (`ContractError::Binding`). `vertices: None` is the three-vertex draw as before. Tests: `operations_fullscreen_streams_tests`.
- **`ResourceRegistry::compile_operations_with_reads(device, operations, reads)`** also checks sampled reads that follow the list in work it does not contain, such as the draws of a stock pass that sample what the list rendered. Each read must name the version the list leaves, so a read of something no operation wrote, of a version a later operation replaced or with another access or kind is an error. The encode checks the read views again. Tests: `operations_reads_tests`.
- **`TimingState::encode_checked(compiled, registry, encoder)`** encodes a compiled list with the frame's pass timer, which lives in the post chain. A host holding the chain's `TimingState` can then have its operations stamped between `begin_timing` and `finish_timing` like the chain's own passes. Without a timer the list is encoded unstamped. Tests: `timing_operations_tests`.

## Stock HDR effects

`ToneMapOutput::with_effects` installs optional adaptive exposure, bloom and
FXAA. An empty policy uses the unchanged fixed-exposure shader. Slot-aware
factories build one immutable schedule per admitted slot. Exposure history
survives resize, while sized filter targets rebuild transactionally.

Metering sanitizes each channel, clamps log2 luminance into 256 bins, and reduces
the fractionally trimmed low/high percentile interval using bin centres.
Adaptation is `previous + (target - previous) * (1 - exp(-speed * delta))`,
starting from exposure one. Up and down rates are independent. Before encoding,
call `prepare_effects` with the selected slot and explicit delta/reset. Its
upload is an encoder-ordered copy, never a queue write. A fixed capture delta
overrides host timing; fixed capture exposure bypasses adaptation. One queue-ordered carry buffer advances history in submission order. Each
reduction publishes its result into a separate slot buffer before tone mapping,
so later frames cannot change that frame's result. Submit successful encoders
in preparation order; do not reuse a slot until its submission completes. A
reset applies once at the requested frame boundary. The effective delta must
be finite and within 0..=10 seconds; invalid preparation returns
`PostError::FrameDelta` before allocation or encoder mutation. A valid fixed
capture delta overrides even invalid host timing. Clearing an installed policy
with empty settings removes all effects and restores the fixed schedule.

The four per-frame uploads of the effects share one rule: `prepare_effects` (the 16-byte
exposure step), `prepare_taa` (the 96-byte `TaaUniform`; `None` uploads zeros),
`prepare_temporal` (the 48-byte `TemporalBlock`) and `prepare_temporal_camera` (the 80-byte
`TemporalCamera`). Each copies its bytes into the reusable upload staging of the
`SubmissionAuthority` frame that `encoder` records and records the copy into the slot's buffer
at once, so the values are final when the call returns and a cancelled or dropped frame gives its
staging back unread. Call them inside `SubmissionAuthority::prepare`, or on the frame `begin`
returns, with that frame's encoder. Outside a frame they return
`PostError::Frame(FrameError::Transaction(TransactionError::Missing))`; with any other encoder,
`ForeignEncoder`. Neither stages anything. Invalid values (`PostError::FrameDelta` for the
exposure delta, `PostError::UnsupportedProfile` for a `TemporalBlock`) fail before anything is
staged. A factory without effects has nothing to upload to, so those calls need no frame and
return `Ok`; `prepare_effects` still validates the delta.

Bloom prefilters unexposed radiance with a quadratic soft knee, downsamples
with a normalized nine-tap binomial kernel, and upsamples with the same
separable 3x3 tent. Each upsample averages its local and coarser levels,
preserving constant radiance instead of multiplying brightness by pyramid
depth. Bloom uses Rgba16Float intermediates and adds before the selected tone curve.
NaN, infinities and negative channels contribute zero to metering and bloom.
Positive infinity remains a white highlight in direct output but does not
spread into neighbours or enter histogram arithmetic. Finite half-float
highlights remain eligible. This extends the existing output special-value
policy to finite statistical/filter inputs without changing direct tone mapping.

`ToneMap::Reinhard` retains `x/(1+x)`. `ToneMap::Filmic` applies
`clamp(x*(a*x+b)/(x*(c*x+d)+e),0,1)` with `(a,b,c,d,e) =
(2.51,0.03,2.43,0.59,0.14)`, the fitted ACES approximation published by
[Krzysztof Narkowicz (2016)](https://knarkowicz.wordpress.com/2016/01/06/aces-filmic-tone-mapping-curve/).
This implements the published equation without importing shader code. It is a
component-wise fit, without ACES color-space transforms. `aces.rs` owns the
coefficients used by both shader generation and CPU references, including the
unclamped bloom bright-extraction curve. Bloom is added before exposure, then
the selected curve runs before the attachment's single sRGB encoding. Negative
radiance and NaN map to zero; positive infinity maps to one. Zero exposure maps
all radiance to zero. `ToneMap::evaluate` provides the display-linear CPU reference;
`ToneMapOutput::evaluate` retains its existing Reinhard semantics.

FXAA uses `(R + 2G + B) / 4` as its luminance proxy. `HdrPostSettings::resolve` is
`PostResolve::Fxaa(FxaaSettings { strength, quality })` to enable it. `strength` is
the filter strength. `quality: FxaaQuality` sets the relative
`edge_threshold`, absolute `minimum_threshold`, `subpixel_blend` strength multiplier,
and `luminance` transfer. Strength and each numeric quality setting must be finite
and within `0..=1`; a zero strength validates but schedules no pass. Filtering skips contrast at or below
`max(minimum_threshold, maximum * edge_threshold)`. Defaults are `0.1`, `1/64`,
`1.0`, and `FxaaLuminance::Linear`, preserving the existing bytes and division by ten.
`FxaaLuminance::Srgb` applies the sRGB OETF to the scalar linear luminance proxy
before contrast and gradient calculations. It does not encode RGB or reinterpret
the source texture: the chain always supplies display-linear RGB.

A Sobel-derived tangent selects bilinear samples 1.5 texels away along its
major axis, with center weight 1/2 and side weights 1/4. The product of strength
and `subpixel_blend` blends that result with the original pixel. Zero gradient
or zero blend preserves the original. The Rgba16Float LDR intermediate keeps flat
regions within one 8-bit step of the unfiltered output: Vulkan lets the float
conversion round either way, so a stored value is off by at most one unit in the last
place, `2^-10` of itself, and the sRGB transfer turns that into at most
`255 * 0.4396 * 2^-10 = 0.11` steps. Only the final sRGB attachment applies output transfer;
alpha is preserved. The filter uses explicit integer loads and bilinear
arithmetic, without implicit derivatives in divergent control flow. See the
[post-filter derivation](../../dev-docs/rendering/post-filters.md) for the base
kernel; the quality controls above extend its fixed contrast cutoff and strength.

## Choosing antialiasing

`AntiAliasing { msaa: MsaaSamples, resolve: PostResolve }` is the one setting for
antialiasing. `MsaaSamples` has the variants `X1`, `X2`, `X4` and `X8`;
`MsaaSamples::try_from(u32)` rejects every other number with
`PostError::SampleCount`. `AntiAliasing::frame_config` builds the `FrameConfig`
for the scene attachments, and `PostResolve` goes into `HdrPostSettings::resolve`.
`PostResolve` is `#[non_exhaustive]`: match it with a wildcard arm, because
further resolves are added as variants. Its variants are `None`, `Fxaa`, `Smaa`
(see [SMAA 1x](#smaa-1x)) and `Cmaa2` (see [CMAA2](#cmaa2)); one resolve is selected, never
two. Prefer SMAA when long high-contrast geometry edges dominate and three passes fit the
budget, FXAA as the cheaper single pass for noisy specular detail, and CMAA2 when the
adapter has compute shaders and the image has text or thin detail that must stay crisp.
Sharpening is a separate, optional field, `AntiAliasing::sharpen: Option<CasSettings>`, that
composes with any resolve (see [Contrast adaptive sharpening](#contrast-adaptive-sharpening)).
`AntiAliasing::new` leaves it `None`; `with_sharpen` sets it.

`ProjectionJitter` shifts the projection by a sub-pixel offset.
`ProjectionJitter::new(length)` is a Halton(2,3) sequence of 1 to 64 frames
(`ProjectionJitter::default()` has 8). `offset_pixels(frame_index)` gives the sub-pixel
offset in internal raster pixels, strictly inside (-0.5, 0.5) with +y down, and
`clip_offset(frame_index, size)` the matching clip-space translation. The projection shift moves
the scene by +offset, so the pixel at `q` holds the scene at `q + 0.5 - offset`. Only
`PostResolve::Taa` jitters, with the default sequence
(`PostResolve::effective_jitter`); see [Temporal antialiasing](#temporal-antialiasing).
`ProjectionJitter::for_extents(render, output)` returns the sequence for a scene rasterised below
its output extent. Its length is `ceil(8 * max(Ow / Rw, Oh / Rh)^2)`, computed per axis with integers
from the rounded extents: 8 at equal extents, 32 at 0.5, 18 at 0.67 and 14 at 0.77 of 1920x1080.
It rejects empty extents, a render extent above the output or below half of it, and a length above
64. Temporal upscaling selects it by itself (`HdrPostSettings::effective_jitter`).

`FrameConfig::validate` admits the sample counts every device guarantees (1 and 4).
`FrameConfig::validate_device` also admits 2 and 8 when the device enabled
`TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES`, and `FrameConfig::validate_adapter`
checks the adapter's reported sample counts and resolve support for the color and
depth formats. Run `validate_adapter` before creating a chain with 2 or 8 samples;
wgpu otherwise reports an unsupported count as a device validation error. An
unsupported count is always an error and is never reduced.

### SMAA 1x

`PostResolve::Smaa(SmaaSettings)` selects SMAA 1x instead of FXAA; `HdrPostSettings::resolve`
holds one resolve, so the two cannot run together. `PostResolve::smaa(SmaaPreset::High)`
builds a preset. `SmaaSettings::LOW`, `MEDIUM`, `HIGH` and `ULTRA` equal the reference
`SMAA_PRESET_*` values:

| Preset | `threshold` | `max_search_steps` | `max_search_steps_diag` | `corner_rounding` |
| --- | --- | --- | --- | --- |
| Low | 0.15 | 4 | off | off |
| Medium | 0.1 | 8 | off | off |
| High | 0.1 | 16 | 8 | 25 |
| Ultra | 0.05 | 32 | 16 | 25 |

Scene consumers author the same resolve through
`SceneAntiAlias::smaa(SceneSmaaPreset::Ultra)` or
`SceneAntiAlias::Smaa(SceneSmaaSettings { .. })` in `render-plan`.
`render-scene` converts the threshold from millionths and preserves every search,
corner and edge-mode setting. This route selects no automatic jitter and rejects
any second resolve from an HDR post block or frame descriptor.

Custom values are validated: `threshold` is above 0 and at most 0.5, `max_search_steps`
is 0 through 112, `max_search_steps_diag` is `None` or 0 through 20, and `corner_rounding`
is `None` or 0 through 100. `None` skips diagonal or corner handling, as the reference does
for Low and Medium. `SmaaEdgeDetection::Luma` (default) compares Rec. 709 luma and
`SmaaEdgeDetection::Color` compares the largest channel difference.

The stage runs where FXAA runs, on the display-referred image at the render extent after tone
mapping and before any upscale. It schedules `smaa_edges` (Rg8Unorm), `smaa_weights`
(Rgba8Unorm) and `smaa_blend` as separate stage identities, each with its own timing slot.
The two intermediate targets and the lookup buffers belong to the `StageSet`, so resize and
reconfiguration rebuild them with the rest of the schedule. Edge decisions use sRGB-encoded
values because the reference thresholds assume gamma-space input; the blend itself reads and
writes linear color. All four channels are blended, so alpha follows the same weights as
color and stays within the range of the pixels it mixes. A constant image is unchanged.

The area (160x560 RG8) and search (64x16 R8) lookup tables are generated at run time by
`src/smaa_lut.rs`, a Rust port of the reference `AreaTex.py` and `SearchTex.py`, and bound as
storage buffers (the stage builder has no queue for texture uploads). The orthogonal half of the
area table is cached for the process; the diagonal half is generated only when
`max_search_steps_diag` is set. Tests compare the output with 64-bit digests of the reference
`AreaTex.h` and `SearchTex.h`, so the bytes are identical to the published tables. The digests were computed from the published headers when the generator was ported; the headers themselves are not committed.

SMAA reads clean stair-step shapes. A straight axis-aligned edge is already sharp and is left
unchanged; blending happens at steps, diagonals, corners and line ends. A one-pixel line is
softened at its ends and kept along its length. Compared with FXAA it preserves thin geometry
better and does not blur texture detail, but it does not smooth shader or texture aliasing.
Choose it when geometry edges dominate and the three passes fit the budget. Choose FXAA for a
single cheap pass over noisy specular detail.

Differences from the reference shader: area lookups use explicit bilinear arithmetic, and
predication, reprojection and subsample indexing (SMAA S2x, T2x, 4x) are not implemented.
Edge detection reads the color input with point loads and neighborhood blending uses one
hardware bilinear fetch per side, as the reference does.

The display-referred image between tone mapping and SMAA is `Rgba16Float`. Values in 0..1
have an 11-bit mantissa at half precision, finer than the 8-bit output, and the format is
filterable, which the bilinear blend fetch needs. FXAA and CMAA2 use the same format; FXAA
previously kept `Rgba32Float`, and its flat-region contract now reads "within one 8-bit step"
(see the FXAA section). `PostResolve::display_format` is the one place that chooses the
format. A resolve followed by an upscale still writes `Rgba32Float` between the
resolve and the upscale.

Measured with the ignored probe `chain::aa_tests::smaa_gpu_tests::smaa_and_fxaa_stage_timestamps` on an
AMD Radeon 780M (RADV, Mesa 26.0.8) at 1920x1080, median of 5 timestamp-query samples per
stage, on a synthetic frame of diagonal edges. The first column is the `Rgba32Float`
intermediate with two point-loaded texels in the blend, the second the `Rgba16Float`
intermediate with bilinear blending:

| Stage (ms) | SMAA high before | SMAA high after |
| --- | --- | --- |
| tone | 2.168 | 1.152 |
| `smaa_edges` | 0.892 | 0.547 |
| `smaa_weights` | 1.223 | 1.184 |
| `smaa_blend` | 1.210 | 0.571 |
| total | 5.493 | 3.454 |

The other presets moved the same way (total, before to after): low 4.989 to 2.953, medium
4.951 to 3.124, ultra 5.674 to 3.259. FXAA measured 3.387 before and 3.102 after with no code
change, and no AA measured 0.366 and 0.408, which bounds the run-to-run noise at about 0.3 ms.
The weights pass reads only `Rg8Unorm` edges and is unchanged.

### CMAA2

`PostResolve::Cmaa2(Cmaa2Settings)` selects Conservative Morphological Anti-Aliasing 2.3 instead
of FXAA or SMAA; `HdrPostSettings::resolve` holds one resolve. `PostResolve::cmaa2(Cmaa2Preset::High)`
builds a preset. `Cmaa2Settings::LOW`, `MEDIUM`, `HIGH` and `ULTRA` equal the reference
`CMAA2_STATIC_QUALITY_PRESET` levels. They differ only in the edge threshold:

| Preset | `edge_threshold` |
| --- | --- |
| Low | 0.15 |
| Medium | 0.10 |
| High (default) | 0.07 |
| Ultra | 0.05 |

`Cmaa2Settings::edge_threshold` accepts values above 0 and up to 0.5; others fail with
`PostError::UnsupportedProfile`. `with_extra_sharpness` turns on the reference
`CMAA2_EXTRA_SHARPNESS` option, which keeps more text and small detail and smooths less.
Edges are found on the square root of the display-linear color as a luma approximation, as the
reference does for linear input.

The stage runs where FXAA and SMAA run: on the display-referred image at the render extent
after tone mapping and before any upscale. The image is `Rgba16Float` with storage binding. It
schedules `cmaa2_clear` (a buffer clear of the counters), the compute passes `cmaa2_edges`,
`cmaa2_candidate_args` and `cmaa2_candidates`, and the full-screen `cmaa2_present` as separate
stage identities, each with its own timing slot. It needs compute shaders and storage buffers in
the compute stage and in the fragment stage (read-only).

Deviations from the reference `CMAA2.hlsl` (the list is also at the top of `src/cmaa2.wgsl`):

- Only the single-sample path is ported. The MSAA path (`CMAA2_SUPPORT_MSAA`) is not.
- `ComputeDispatchArgsCS` becomes the one-thread pass `cmaa2_candidate_args`, because the
  candidate count is final only after the edge pass. It writes indirect dispatch arguments from
  the counter into a 12-byte `INDIRECT` buffer, and `cmaa2_candidates` uses
  `dispatch_workgroups_indirect` (`Operation::ComputeIndirect`). Lists longer than 4096
  workgroups wrap to further rows. A buffer clear zeroes the counters instead of the reference
  pass.
- WGSL has no `R8Uint` storage texture and storage textures have no `InterlockedExchange`.
  Edges are one `u32` per 2x2 quad (4 bits per pixel) in a storage buffer, and the candidate and
  blend-item lists and the atomic list heads are storage buffers. Positions are not packed into
  14 bits, so the packing does not limit extents to 16384 pixels.
- The reference writes blended colors into the input image in place, in a compute pass
  (`DeferredColorApply2x2CS`) that visits each touched quad through a location list. The input
  here is a sampled texture, so `cmaa2_present` writes the output instead: every fragment reads
  its pixel, walks the blend list of its 2x2 quad (the list heads are per quad, as in the
  reference) and averages the items for its own pixel, or returns the input pixel unchanged when
  there are none. The result and the alpha of the original pixel match the reference apply
  pass. There is no working copy of the image, no apply pass and no location list. The edge
  pass resets every quad head, not only the heads of quads with edges, so the fragment pass
  never reads items of an earlier frame.
- Samples outside the image clamp to the border instead of reading as zero.
- A full work list drops the item instead of writing past the end. The lists hold a
  fraction of the pixel count, as in the reference.
- Luma edge detection, `CMAA2_COLLECT_EXPAND_BLEND_ITEMS` and
  `CMAA2_DEFERRED_APPLY_THREADGROUP_SWAP` are enabled, and colors in the lists use the
  `R11G11B10` E4 packing (non-HDR range, clamped to 1.99).

At 1920x1080 the CMAA2 storage is about 14.6 MB of buffers (edges and list heads 2.1 MB each,
candidates 2.1 MB, blend items 8.3 MB). There is no second image. These are counted in the post
allocation report. Without a CMAA2 resolve none of it is allocated.

Measured with the ignored probe `chain::aa_tests::smaa_gpu_tests::smaa_and_fxaa_stage_timestamps` on an
AMD Radeon 780M (RADV) at 1920x1080, median of 5 timestamp-query samples per stage, with the
total including the `tone` stage (about 0.9 to 1.2 ms, the same work for every resolve). Times are in ms. The "disc"
frame is a sparse disc edge and the "stripes" frame is dense stripes. Run-to-run noise is about
0.1 to 0.3 ms.

| Resolve | AA stages, stripes | Total, stripes | Total, disc |
| --- | --- | --- | --- |
| none | none | 0.375 | 0.344 |
| FXAA | `fxaa` 0.499 | 1.655 | 1.518 |
| SMAA low | edges 0.542, weights 0.770, blend 0.441 | 2.760 | 2.017 |
| SMAA medium | edges 0.543, weights 0.788, blend 0.516 | 2.932 | 2.101 |
| SMAA high | edges 0.543, weights 1.120, blend 0.441 | 3.053 | 2.081 |
| SMAA ultra | edges 0.543, weights 1.173, blend 0.430 | 3.144 | 1.981 |
| CMAA2 low | edges 0.450, candidates 0.582, present 0.540 | 2.613 | 1.801 |
| CMAA2 medium | edges 0.413, candidates 0.546, present 0.478 | 2.433 | 1.735 |
| CMAA2 high | edges 0.429, candidates 0.522, present 0.454 | 2.374 | 1.727 |
| CMAA2 ultra | edges 0.413, candidates 0.518, present 0.473 | 2.383 | 1.746 |

FXAA is the cheapest resolve in total: 1.66 ms on stripes and 1.52 ms on the disc, against 2.37
and 1.73 ms for CMAA2 high and 3.05 and 2.08 ms for SMAA high. Its row was measured after the
FXAA intermediate moved from `Rgba32Float` to `Rgba16Float`, as the median of three runs
alternating with main on the same GPU: the total fell from 3.34 to 1.66 ms on stripes and from
2.95 to 1.52 ms on the disc, `tone` from about 2.2 to 1.15 ms and `fxaa` from about 1.0 to 0.45
ms. The other rows predate that change and are unaffected by it. Counting only the AA passes,
CMAA2 high is 1.40 ms on stripes and 0.73 ms on the disc, SMAA high is 2.10 and 1.06 ms, and
the single `fxaa` pass is 0.50 and 0.38 ms. FXAA does the least work, but it only blurs along
detected edges and does not reconstruct geometry, so CMAA2 and SMAA remain the higher-quality
choices. The disc edge pass costs 0.30 ms, the same as `smaa_edges` at
0.32 ms, which is close to the time to read the 16.6 MB image once. The extra cost on the stripes
frame is the candidate pass (0.52 ms, atomics and the line search) and the present pass
(0.45 ms against 0.38 ms on the disc). The preset changes the time by less than the noise, because
the fixed-size dispatches run the same number of threads whatever the edge count.

How the time got here, on the stripes frame at the high preset (total with `tone`): indirect
dispatch of the candidate and apply passes gave 3.88 ms (disc 2.71 ms); folding the apply pass
into the present pass and dropping the full-image working copy gave 2.37 ms (disc 1.73 ms). The
edge pass fell from 1.20 to 0.43 ms mostly because it no longer writes four `Rgba16Float`
texels per thread. Edges were already stored packed, one `u32` per quad. One experiment was
rejected: sharing the quad lumas through workgroup memory so each edge thread loads 4 texels
instead of 8 (one more barrier) changed the stripes edge pass from 0.429 to 0.418 ms and the
disc from 0.305 to 0.295 ms, within noise, so it was reverted to keep the pass simple. The probe
skips the measurement and passes on adapters without `TIMESTAMP_QUERY`, such as lavapipe.

### Contrast adaptive sharpening

`CasSettings { sharpness }` selects AMD FidelityFX CAS in its sharpen-only form. It is not a
`PostResolve` variant: it does not remove aliasing, it restores local contrast that a resolve or
a soft texture filter took away, so it follows whichever resolve is selected. Set it with
`AntiAliasing::with_sharpen(CasSettings::new(0.5))` or `HdrPostSettings::sharpen`.
`sharpness` must be finite and within 0 through 1, or `PostError::UnsupportedProfile("CAS settings")`
is returned. Sharpness 0 is the reference default with the least ringing and still sharpens;
1 is the strongest setting. `None` schedules nothing: the stage list, intermediates and output
bytes are those of a build without the field.

The stage runs after the resolve (FXAA, SMAA, CMAA2, or tone mapping when the resolve is `None`) and
before any upscale, at the render extent, as one fullscreen pass with the stage identity `cas`
and its own timing slot. Its input is the display-referred image between tone mapping and the
output, so the same intermediate rules apply: a resolve that feeds CAS writes an `Rgba16Float`
texture, and so does tone mapping when the resolve is `None`. The FXAA, SMAA and CMAA2 intermediates
are unchanged when CAS is off. With an upscale, CAS writes an `Rgba16Float` texture that the
upscale reads. The allocation budget counts that one extra intermediate.

For each pixel the filter reads itself and its four neighbours. The green channel gives the local
minimum and maximum; the headroom to black or white divided by the maximum, square-rooted, is the
amount of sharpening. Where contrast already reaches 0 or 1 the amount is 0, so flat areas and
saturated edges are untouched. The amount scales a negative lobe between -1/8 (sharpness 0) and
-1/5 (sharpness 1) on the four neighbours, and the result is normalized by `1 + 4w` and saturated.
Every output channel stays within the 3x3 neighbourhood range on a soft edge, a constant image is
unchanged at every sharpness, and alpha is copied.

How it differs from RCAS: RCAS is the optional second pass of the FSR 1 upscale. It runs at the
output extent after EASU, takes its strength in stops from 0 (strongest) through 2, and limits
ringing from the minimum and maximum of each color channel. It also supports an opt-in native-resolution pass. CAS
runs without scaling, at the render extent, with a 0 through 1 sharpness and a contrast
adaptive amount driven by headroom, so it works with MSAA, FXAA or SMAA at native resolution. With
an upscale configured, CAS sharpens before reconstruction and RCAS after it; use both only if the
combined result is not over-sharpened. Choose the filter and its domain explicitly when restoring detail after reconstruction or a
soft resolve at native resolution.

Differences from the reference header: only the `noScaling` path of `CasFilter` is ported, as a
fragment pass. Taps outside the image clamp to the edge texel. The default quality path is used
(green-channel weights, no `CAS_BETTER_DIAGONALS`) with exact reciprocals and square roots in
place of the `APrx` approximations, and the 16-bit packed form is not ported. The shader file
keeps the AMD notice.

`src/cas_reference.rs` is a scalar f64 reference of the same kernel. The pass alone, writing an
`Rgba32Float` target from f32 input images with odd extents, is compared with it at an absolute
per-channel bound of 0.00001, a few f32 ulps. Chain tests then check each resolve, resize, the
upscale ordering and the 8-bit sRGB result.

Measured with the ignored probe `chain::aa_tests::smaa_gpu_tests::smaa_and_fxaa_stage_timestamps` on an
AMD Radeon 780M (RADV, Mesa 26.0.8) at 1920x1080, median of 5 timestamp-query samples, on a
synthetic frame of diagonal edges, sharpness 0.5:

| Configuration | `cas` stage (ms) | Total (ms) |
| --- | --- | --- |
| tone mapping, then CAS | 0.417 | 1.678 |
| FXAA, then CAS | 0.492 | 4.618 |
| SMAA medium, then CAS | 0.490 | 4.644 |

The CAS pass costs about 0.4 to 0.5 ms. The run-to-run noise on this adapter is about 0.3 ms
(the FXAA and SMAA blend passes moved by more than the CAS pass between runs), so read the
stage figure as a bound rather than a precise cost.

## CPU admission contracts

CPU-only consumers use `worldrunner_render_plan::post::{RenderScale, UpscaleMode,
UpscaleSettings, PostContractError, MAX_GAUSSIAN_RADIUS}`. These definitions belong
in the CPU planning boundary; `render-post` depends on that implementation using
the permitted Graphics-to-Bridge edge. Neither crate forwards aliases for these
names. The facade exposes them through `worldrunner::scene::plan::post` or
`worldrunner::world::worldrunner_render_plan::post` with the CPU-only `world` feature.

`RenderScale::new`, `RenderScale::extent` and `UpscaleSettings::validate` return
`PostContractError::UnsupportedProfile` or `PostContractError::EmptyExtent`, keeping
the admission messages and rounding below. GPU entry points convert these errors
to the corresponding `PostError` variants. `MAX_GAUSSIAN_RADIUS` is 64.
The existing xtask CPU dependency guard checks the complete `render-plan` closure,
including build and development dependencies, for GPU backends.

## Reduced internal resolution

`FrameConfig::size` is the internal scene extent. Construct a configuration with
its presentation extent, then call `with_render_extent([render_width, render_height])`.
`output_extent()` returns the presentation extent. Both dimensions must be
nonzero, within device limits, and the output must contain the render extent.
`RenderScale::new` accepts 500000 through 1000000 millionths (0.5 through 1.0).
Its `extent` method rounds to nearest, with halfway values rounded up.

`SpatialUpscale` is a retained `StageFactory` for display-referred inputs.
Select `UpscaleMode::Bilinear` or `UpscaleMode::Easu`. Set `sharpness` to
`Some(stops)` for RCAS, from zero (maximum sharpening) through two stops.
`None` is the exact off value and omits the RCAS pass. With RCAS enabled, the
reconstruction pass writes an Rgba16Float intermediate: its display-referred
values need no more precision, and on an integrated GPU at 1600x900 output it
measured about half the reconstruction and RCAS time of an Rgba32Float
intermediate. The WGSL port uses full
precision reciprocals and square roots, with clamped border loads. EASU clamps
each reconstructed RGB channel to the nearest four source samples. Alpha uses
bilinear interpolation and passes unchanged through RCAS.

For HDR, set `HdrPostSettings::upscale` and provide the two extents in
`FrameConfig`. Exposure and bloom run at the render extent before tone mapping.
FXAA runs after tone mapping at the render extent, then EASU/bilinear and RCAS
run before final attachment transfer and format conversion. At equal extents,
the HDR path omits reconstruction and upscale RCAS. Native RCAS is separately opt-in,
so the default preserves the native pass count and output bytes.
Use `PostChain::resize_extents` to replace both extents and their dependent
bindings transactionally. `resize` changes the internal extent; an explicit
presentation extent remains unchanged. Reconfiguration rebuilds stage bindings.

### Temporal upscaling

`UpscaleMode::Temporal` is not a `SpatialUpscale` pass (`SpatialUpscale::validate_config` rejects it).
With a TAA resolve, one sample per pixel and a render extent of at least half the output,
`hdr_taa::append` builds a temporal bank instead of the native one: the `temporal upscale` stage
(`taa_upscale.wgsl`) reads the render-extent frame, the velocity target and the `Rgba16Float`
history the previous frame wrote, and writes the other history at the output extent; the stages
after it read that history and there is no per-slot output texture. The bank holds two such histories shared by every slot and the pass
alternates them (see "History without a copy" below). Three compute
stages at the render extent (`taa_reconstruct.wgsl`, scheduled by `taa_reconstruct_gpu::append`) run
ahead of it, listed below. A `TemporalBlock` (48 bytes: both extents, jitter, phase count, history weight, clip scale,
`history_valid`) is uploaded per frame with `ToneMapOutput::prepare_temporal` (inside a frame, see above); there is no all-zero
block, a reset is a real block with `history_valid: false`. Metering, bloom, Gaussian, CAS, tone
mapping and RCAS use the colour extent of the chain (`Scene::extent`, the output extent here), and
`UpscaleSettings::sharpness` becomes a display-linear RCAS after the stage. The history bank is
keyed by render extent, output extent, mode and sample count and replaced transactionally: a
rejected configuration keeps the committed bank (`StageFactory::settle_build`).
`HdrPostSettings::effective_jitter` gives `ProjectionJitter::for_extents(render, output)` when no
explicit sequence is chosen, and `validate_jitter` rejects a shorter one. The allocation budget
counts two shared histories at the output extent and no output texture, plus per slot the dilated depth (R32Float), dilated motion (Rg32Float), depth confidence (R32Float) and reconstructed depth buffer at the render extent (20R bytes), each a per-generation entry (a shared entry would need one descriptor across generations, #399). Measured static error ratios
against reset-every-frame are in [docs/rendering.md](../../docs/rendering.md#temporal-upscaling).

Passes of a temporal slot, in order (each a `StageSet` stage). Pass 4, the lock half of pass 5 and the lock history copy of
pass 6 exist only when the thin-feature locks are on, which is test only (`WithLocks`). With locks off, the
default, there is no `temporal create locks`, no `temporal lock history`, no lock bank and no lock
memory, and `temporal upscale` runs its `taa_upscale_unlocked` entry point with the history as its only target.

1. `temporal depth clear`: fills the reconstructed depth buffer (`array<atomic<u32>>`, 4R bytes) with the bits of +inf.
2. `temporal reconstruct dilate`: per texel, the nearest valid depth (`0 < z < 1`) of the 3x3 neighbourhood
   (ties: first in raster order), its residual plus the camera term (`TemporalCamera`, 80 bytes, binding in
   this pass, uploaded with `ToneMapOutput::prepare_temporal_camera`) as total backward motion in uv, written
   to the dilated depth and motion planes, and scattered with `atomicMin` over the 2x2 footprint at `s + motion * R`.
3. `temporal depth clip`: compares the linear depth (`b / (z - a)` from `TemporalCamera::depth_terms`)
   of each texel with the nearest depth that reached its history position; confidence exactly 0 or 1.
   No valid depth (sky, cleared) or no depth terms: 1. History outside the frame or unreached: 0.
4. `temporal create locks` (`taa_thin.wgsl`, compute at R): marks one texel thin features, texels whose
   luminance (`y / (1 + y)` of the Rec. 709 sum) differs from both neighbours of one of four axes by at
   least 1/16 while the two neighbours agree within a quarter of the smaller contrast. `Rg32Float`
   (strength, luminance), 8R bytes per slot; the one texel border writes 0.
5. `temporal upscale`: reads the dilated motion and the minimum confidence of the 2x2 texels around its sample
   position, which scales the history weight. It also gathers the strongest candidate of those 2x2 texels
   (0 at confidence 0, which includes failed depth, and at nonfinite motion), reprojects the lock bank with
   the total motion (exact texel load), and writes the new lock bank next to the colour (two colour
   attachments). A lock is killed by an unusable history (reset, outside the frame), confidence 0, or a
   one frame jump of the 3x3 neighbourhood luminance larger than the contrast stored with the lock; it
   lives one jitter cycle (`phase_count` frames) after its last candidate. A locked texel halves the share
   of the new sample (`w` becomes `1 - (1 - w) / 2`); `history_weight == 0` stays 0. The `Rgba16Float` lock
   bank holds frames left, reference luminance, contrast and one unused lane, 8O bytes per image (two 16
   bit float channels are not a core storage format).
6. With locks only, `temporal lock history`: the shared copy of the lock bank. The colour history has no copy.

History without a copy. The bank holds two `Rgba16Float` histories at the output extent. `temporal upscale`
writes its colour only to the history it does not read, and tone, exposure metering and bloom read that
history (`scene_color::SceneColor`, bind groups per parity), so there is no slot output texture and the former `temporal upscale history` texture copy (about 1.5 ms of the 1080p frame on a Radeon 780M, the
ablation with the copy stage disabled) is gone. The target is the history alone unlocked (8 bytes per
sample) and history and lock bank locked (8 + 8 = 16 bytes per sample). The stage keeps two bind group and target variants of one pipeline
(`Operation::FullscreenAlternating`), and each reader of the history keeps two bind groups
(`FullscreenAlternating`, `ComputeAlternating` for the luminance histogram); the variant is chosen at encode time by a `HistoryParity`. The host
sets the parity with `ToneMapOutput::select_history_parity` before it encodes a frame: `render-scene`
passes the parity of its submitted frame count (`TemporalHistory::frame_index`), which advances only when
a frame is submitted and is zero after a reset. An encoded frame that is cancelled or fails therefore
does not change which history the next frame reads, and a reset starts even. Even parity reads the second
history and writes the first, odd parity the reverse; a stage never reads and writes one texture in a pass.
The output is bit-identical to the copying version (checked on lavapipe for the pass alone, locked and
unlocked, and for eight frames of the quality scene at three render scales). The second history costs
8 O bytes (16,588,800 bytes at 1920x1080). Reading the history the pass wrote, instead of a separate
output, removes the three per-slot output textures (3 x 8 O bytes, 49,766,400 bytes at 1920x1080) and the
second colour target of the pass.

With a temporal mask, `temporal mask dilate` runs after `temporal depth clip` and the upscale pass consumes the
prepared mask; see [Reactive and composition mask](#reactive-and-composition-mask).

Lock state costs, only with locks on and measured by `taa_allocation_tests` (with locks off they are zero): candidates 8R per slot, lock bank 8O per slot and one
shared lock history 8O, so 24R + 32O bytes with three slots (the design counted 16O for two 16 bit channels).

**The locks ship off and no measurement shows them improving the output.** `taa_upscale_gpu::LOCKS_DEFAULT` is
`false` (the `locks` pipeline override of the shader), so the default output equals the output without locks
byte for byte; the thin-feature sweep asserts `to_bits` equality of the default and `WithoutLocks` chains on
all 15 fixture and speed cases at each of the scales 0.5, 0.67 and 0.77. The rules, fixed before any lock code:
lock-on ROI error at most 0.90 times lock-off on 18 designated cases (grid, wire 45, wire 90, static and at 0.3
px per frame, three scales), at most 1.10 times on the other 27, and a vanished line within 2/255 display levels
after at most two submitted frames. Two lavapipe measurements, both failing:

| Attempt | Rule 1 (18 designated, 0.90) | Rule 2 (27 others, 1.10) | Rule 3 (vanished line, 2 frames) |
| --- | --- | --- | --- |
| 1. lock bank and raised weight (shipped, off) | held on 0 of 18 (0.952 to 1.045) | held on 27 of 27 (largest 1.097) | held (1 frame) |
| 2. clip box widened by the lock contrast (not kept) | held on 6 of 18 (the static cases at 0.67 and 0.77, 0.618 to 0.859), failed on 12 (0.902 to 1.192) | failed on 1 of 27 (1.118) | failed on 6 of 6 (14 to 32 frames), and an x4 shading change left 44.3 levels |

The plumbing, allocation, reset behaviour, the exposure and radiance guards, the shading change test and the
measurement ship; enabling the locks is tracked in issue #436 (a lock kill when the contrast vanishes, a
separate diagnosis of scale 0.5 and 0.3 px per frame, a re-measure on both devices). The full ratio tables are
in [docs/rendering.md](../../docs/rendering.md#temporal-upscaling). `lock_rule_measurement` in
`temporal_thin_feature_tests.rs` reproduces the 45 row table (27 hold with the shipped shader) and asserts only
finite output; enabling locks requires changing it to assert `lock_rule_holds` on both devices.

Luminance normaliser: none was added. The path is already scale invariant (relative ROI error 0.97 to 1.02 of the
unscaled run from 2^-10 to 2^13; `Rgba16Float` overflows beyond 2^13), and luminance is computed only for the
lock reference. The exposure guard asserts that the history, copied before the tone stage, is unchanged within
2^-11 relative (2^-24 floor) across display exposures 2^-6 to 2^6.

The test-only switches `taa_upscale_gpu::WithLocks`, `WithoutLocks`, `taa_reconstruct_gpu::WithoutDilation` and `WithoutClip` (thread-local guards over
pipeline-overridable constants; production passes the defaults) recompute the PR 2 path on the same device
for the comparison tests. Third-party material: nothing is ported; the stages follow the design text only.

`HdrPostSettings::material_mip_bias(config)` returns exactly `+0.0` in every mode, including
`Temporal` at 0.5, 0.67 and 0.77. The render-scene sampling sites take the bias from the frame
block, so a policy can be switched on in that one function. None ships: on the AMD Radeon 780M no
nonzero candidate stayed within 1.10 times the native TAA shimmer at scale 0.5. The
measurements on lavapipe and the Radeon, the decision rule and what remains before a bias can be
enabled are in [docs/rendering.md](../../docs/rendering.md#material-mip-bias).

#### Qualification on the Radeon 780M

The same measurements, repeated from [docs/rendering.md](../../docs/rendering.md#qualification-on-the-radeon-780m) so the crate page is complete. Section 7 of issue #256 measured `Temporal` against fixed gates on a composite scene that no tuning used. The runner is `qualification_matrix` in `crates/render-scene/src/compositor/qualification_matrix_tests.rs`. It drives `ConfiguredFrameRenderer` at two outputs (1920x1080 and 1452x921), three scales (0.50, 0.67, 0.77) and five modes: M1 bilinear, M2 EASU, M3 reduced TAA plus EASU, M4 `Temporal` and M5 full-resolution 1x TAA with no upscale. RCAS is off in all of them, `Temporal` ships with locks off and no mask producer, and the mip bias is 0. The gates and the outcome rule were fixed before the first measurement and are not changed here.

Device: AMD Radeon 780M, Mesa 26.0.8 RADV. Quality cells use a 4x4 supersampled reference, 64 warm-up frames and 4 measured frames per cell. Cost cells use the whole-frame GPU timestamp envelope (a timestamp pair around scene, post, copies and clears), 30 warm-up frames and 120 measured frames, one frame submitted and waited for at a time, so the numbers are per-frame latency on an idle queue. The full matrix took 1470 s. Lavapipe ran the same tests for correctness only; its times come from a CPU rasteriser and are not cost evidence.

**G1, static quality.** Converged static edge MAE of `Temporal` against EASU (M2), bound 0.90. All six cells pass.

| Output | Scale | Temporal edge MAE | EASU edge MAE | Edge ratio | Whole-image MAE ratio |
|---|---|---|---|---|---|
| 1920x1080 | 0.50 | 0.0247 | 0.0306 | 0.807 | 0.709 |
| 1920x1080 | 0.67 | 0.0211 | 0.0284 | 0.743 | 0.737 |
| 1920x1080 | 0.77 | 0.0176 | 0.0272 | 0.647 | 0.655 |
| 1452x921 | 0.50 | 0.0308 | 0.0354 | 0.870 | 0.780 |
| 1452x921 | 0.67 | 0.0155 | 0.0248 | 0.625 | 0.592 |
| 1452x921 | 0.77 | 0.0113 | 0.0221 | 0.511 | 0.514 |

**G7, cost.** Median whole-frame GPU time of `Temporal` against full-resolution TAA (M5), bound 0.90, and p95 of `Temporal` no higher than the p95 of M5. All six cells fail both conditions.

| Output | Scale | Temporal median (p95) ms | Full-resolution TAA median (p95) ms | Ratio |
|---|---|---|---|---|
| 1920x1080 | 0.50 | 8.60 (8.74) | 7.06 (7.21) | 1.22 |
| 1920x1080 | 0.67 | 9.58 (9.83) | 7.06 (7.21) | 1.36 |
| 1920x1080 | 0.77 | 10.36 (10.54) | 7.06 (7.21) | 1.47 |
| 1452x921 | 0.50 | 5.27 (5.58) | 4.50 (4.63) | 1.17 |
| 1452x921 | 0.67 | 6.25 (6.40) | 4.50 (4.63) | 1.39 |
| 1452x921 | 0.77 | 6.69 (6.83) | 4.50 (4.63) | 1.49 |

At 0.77 on 1920x1080 the bound needs `Temporal` at or below 6.35 ms; the matrix run measured 10.36 ms.

**Per-stage breakdown at 1920x1080.** `qualification_stage_timings` reports the median GPU time of every timed pass over 30 warm-up and 120 measured frames. Scene rasterisation, clears and texture copies have no timing slot (the measured temporal versions still copied the output-sized history, which `temporal upscale` now writes itself) and sit in the gap between the whole-frame median and the sum of the stage medians.

| Mode and scale | Stage medians, ms | Sum of stages | Whole frame median (p95) | Gap |
|---|---|---|---|---|
| M3 0.50 | taa resolve 0.891, tone 0.775, easu 1.162 | 2.828 | 3.898 (4.049) | 1.070 |
| M4 0.50 | temporal depth clear 0.121, reconstruct dilate 0.431, depth clip 0.251, mask dilate 0.051, **temporal upscale 5.002**, tone 0.792 | 6.649 | 8.876 (13.622) | 2.228 |
| M5 | taa resolve 3.079, tone 0.816 | 3.896 | 7.201 (10.873) | 3.306 |
| M3 0.77 | taa resolve 1.929, tone 1.800, easu 1.432 | 5.161 | 7.290 (10.474) | 2.129 |
| M4 0.77 | temporal depth clear 0.302, reconstruct dilate 1.085, depth clip 0.617, mask dilate 0.095, **temporal upscale 5.031**, tone 0.877 | 8.007 | 10.719 (14.044) | 2.712 |

The output-resolution `temporal upscale` pass costs about 5.0 ms at 1080p whether the scene renders at 0.50 or 0.77. That is 4.3 times the EASU pass at 0.50 and 1.6 times the full-resolution TAA resolve. The stages that run at the internal extent grow with the render scale but stay near or below 1.1 ms each. This stage-timing run was a separate invocation from the matrix, so its whole-frame medians (for example 8.876 ms for M4 at 0.50) differ from the matrix cost table (8.60 ms); the two were not averaged.

**Conclusion under the predeclared rule.** A scale that misses a gated item is not recommended, and if 0.67 and 0.77 both miss, the mode keeps its experimental label. G1 holds everywhere and G7 fails everywhere, so `Temporal` is not recommended at any of the three scales and stays experimental. `Easu` and native TAA remain the supported choices. Optimising the `temporal upscale` pass is separate work with its own proposal and measurements, and the matrix is rerun afterwards.

**Reported, not gated.** On the composite scene with a 0.3 px per frame pan, `Temporal` edge MAE against M3 (reduced TAA plus EASU) is 0.849, 0.876 and 0.882 at 0.50, 0.67 and 0.77 on 1920x1080 and 0.905, 0.800 and 0.807 on 1452x921. The reference-relative instability ratio against M3 is 1.373, 1.165 and 1.089 on 1920x1080 and 1.310, 1.168 and 1.105 on 1452x921, above the 1.10 bound in five of six cells. The gate G3 itself uses the thin-feature fixtures, so these values are context only.

**Not run yet.** G3 and G4 on the thin-feature fixtures at the matrix sizes, and G5 (reset, cancel and resize against a fresh renderer) at the matrix sizes. The matrix has no reset-every-frame mode, so G2 at the matrix sizes has no row either; the small-size ratios above (160x96 and 64x64) are the only G2 evidence. The small-size tests for these behaviours still pass. G6 is a report and the matrix table carries the full-resolution TAA numbers for it. G8 is carried over from the mip bias decision above.

**Reproducing.** Both tests are `#[ignore]` and print tables:

```sh
cargo test --locked -p worldrunner-render-scene --all-features qualification_matrix -- --ignored --nocapture --test-threads=1
cargo test --locked -p worldrunner-render-scene --all-features qualification_stage_timings -- --ignored --nocapture --test-threads=1
```

The `RD_QUAL_*` variables shrink the runs: `OUTPUTS` (`WxH,...`, matrix default `1920x1080,1452x921`), `SCALES` (millionths, default `500000,670000,770000`), `MODES` (`M1,...,M5`), `PAN` (output pixels per frame, default `0,0.3`), `FRAMES` (4), `WARM` (64), `FACTOR` (reference supersampling per axis, 4), `TIMING_WARM` (30) and `TIMING_FRAMES` (120). The stage breakdown defaults to `1920x1080`, scales `500000,770000` and modes `M3,M4,M5`. Select the Radeon with `VK_DRIVER_FILES` pointing at the RADV ICD. A device without `TIMESTAMP_QUERY` prints `unavailable` for cost, never zero.

#### Reactive and composition mask

A temporal upscale (`UpscaleMode::Temporal` at a render extent below the output) owns one mask beside the
velocity target. `StageFactory::needs_temporal_masks` is true for that mode only, and `FrameTargets`
allocates the mask only when TAA reconstructs the output grid; at equal extents (the native TAA path) and in
every other mode no mask exists.

* Resource. `TEMPORAL_MASK_FORMAT` is `Rg8Unorm` at the internal extent R, one sample, usable as render
  attachment, sampled texture, copy source and copy destination. Red is the reactive value, green the
  composition value, both in [0, 1]. `FrameTargets::temporal_mask()` and `temporal_mask_texture()` mirror
  `velocity()` and `velocity_texture()`; `FrameTargets::validate_temporal_mask(config, &texture)` rejects a host
  texture of the wrong sample count, format (`PostError::UnsupportedFormat`) or extent
  (`PostError::MaskExtent { expected, actual }`) before any encoder exists.
* Clear. The host calls `FrameTargets::encode_mask_clear(encoder)` first in the frame's encoder, before any
  producer. It encodes one render pass clearing both channels to zero and nothing when there is no mask. The
  clear is in the frame's own encoder, so a frame whose encoder is dropped never submits its producers' writes
  and the next frame clears again (`temporal_mask_clear_tests`, including a stale 1.0 that would otherwise be
  dilated and consumed).
* Producer contract, for any alpha, particle or water feature. Declare the mask as an
  indexed colour output with `AttachmentLoad::Load` and the view with `with_retained_contents()`, so the
  frame validator accepts the host clear as the producer. Write with `BlendOperation::Max` on both
  channels, so overlapping producers combine monotonically and no producer lowers another's value. Never
  clear it: a clear in a producer is right only while that producer is the first writer of every frame and it
  would erase an earlier producer and hide a stale mask from the cancelled-frame guarantee. Draw at one
  sample. Cap the value in the producer; the consumer applies no cap, so reactive 1 means the history weight
  is zero. A producer that is not a pass of its own must stay within the 32 byte colour attachment limit
  (`ContractError::ColorAttachmentBytes`); the stock water pass already uses all 32 bytes, so a water
  producer needs a pass of its own (see the [render-scene README](../render-scene/README.md#temporal-reactive-mask)).
* Dilation. When a mask exists, `temporal mask dilate` (`taa_mask.wgsl`, `taa_mask_gpu.rs`) is scheduled
  after `temporal depth clip` and before `temporal create locks` or `temporal upscale`. It takes the 3x3
  maximum of each channel (clamped at the image border) into a per-slot `Rg8Unorm` "temporal prepared mask"
  at R. It is a fullscreen fragment pass because `Rg8Unorm` is not a core storage format. A single set
  texel grows to its 3x3 neighbourhood and not beyond; 0.5 passes through to within 1/255.
* Reactive consumption (`taa_upscale.wgsl`). The shader reads the maximum of the prepared red over the
  same 2x2 internal texels as the depth confidence and computes
  `wh = history_weight * (1 - reactive) * clamp(1 - 1.5 * moved) * confidence`. Reactive 1 gives `wh = 0`,
  which is the current-frame Lanczos2 reconstruction (the reset image, within 2^-11 per channel on texels
  whose dilated reactive is 1); the history weight is monotone in reactive. Alpha stays the current alpha.
* Composition consumption. The locked entry point reads the maximum of the prepared green over the same
  texels. The lock share is interpolated, `mix(LOCK_SHARE, 1, composition)`, which is exactly 0.5 at
  composition 0. At composition 1 no lock is reprojected, no candidate is gathered and the new lock bank is
  written as zero, so composition 1 removes the locks (the lock-off image within 2^-11, the bank all zero).
  `taa_upscale_unlocked`, the default entry point, has no composition consumer: with locks off (the default,
  #436) the green channel changes nothing.
* No mask, no cost. An absent mask schedules no clear and no `temporal mask dilate`, allocates no mask and
  binds a 1x1 zero texture with `masks = 0` in the shader, so the stage list, the allocation and the output
  are those of the path before masks. The default output is unchanged with no mask and with an all-zero mask:
  `absent_and_all_zero_masks_are_the_path_before_masks` (`temporal_reactive_tests.rs`) compares the last 8
  frames by `to_bits`, and `composition_zero_is_the_locked_path_without_masks`
  (`temporal_composition_tests.rs`) does the same for the image and the lock bank under `WithLocks`. The
  test-only switch `taa_mask_gpu::WithoutMasks` builds the chain whose targets carry no mask.
* Measured bytes. The producer mask is one shared `Rg8Unorm` entry, "frame temporal mask", 2R bytes per
  frame generation (written and read within one encoded frame in queue order, reset by the clear). The
  prepared mask is a per-slot entry, "temporal prepared mask", 2R bytes each. With three slots that is
  2R + 3 x 2R = 8R bytes: 7,680 + 23,040 = 30,720 at the 160x96 test rig at scale 0.5 (R = 80x48 = 3,840 texels).
  `taa_allocation_tests` and `post_extent_allocation_tests` pin these and pin that no other configuration
  names a mask. The 1x1 zero texture a slot binds for a frame without a mask is not in the declaration.
* Measured quality, lavapipe, mean absolute HDR error over the last 8 of 48 frames of a 70% white translucent bar
  moving 2 canvas px per frame over an opaque textured background (`temporal_translucent_scene.rs`), trail
  region = strip vacated in those frames, reactive painted on the bar (`temporal_reactive_tests.rs`):

  | Scale | R | Trail at reactive 0, 0.25, 0.5, 0.75, 1 | Reset floor | Trail at 0.9 | Static region, every strength |
  | --- | --- | --- | --- | --- | --- |
  | 0.50 | 80x48 | 0.07979, 0.06440, 0.05051, 0.04203, 0.04102 | 0.04357 | 0.04068 | 0.07141 |
  | 0.67 | 107x64 | 0.06215, 0.05353, 0.04210, 0.03328, 0.02906 | 0.03235 | 0.02921 | 0.03831 |
  | 0.77 | 123x74 | 0.05100, 0.04357, 0.03284, 0.02256, 0.01547 | 0.03053 | 0.01672 | 0.03363 |

  The rule fixed before implementation: each step of the strength sweep at most the previous plus 2^-11,
  trail at 0.9 at most reset + 0.5 (zero - reset), static region at most 1.10 times the zero-mask run. All hold at
  the three scales on lavapipe (the static region is bit-equal to the zero-mask run) and on the Radeon 780M
  (17 tests of `temporal_translucent` and `temporal_mask`). The composition tests were not run on the Radeon.

No pass in the tree produces the mask: the host clear is its only writer. A water producer was measured,
made the TAA output worse and was deleted; see the
[render-scene README](../render-scene/README.md#temporal-reactive-mask).

### Native-resolution RCAS

`HdrPostSettings::native_rcas: Option<NativeRcasSettings>` schedules one `rcas`
fullscreen pass when the render and output extents are equal, after tone mapping,
the selected resolve and CAS. `None` is the default and allocates no extra texture
or stage. `NativeRcasSettings { stops, domain }` requires finite stops within
`0..=2`: zero is strongest, two is weakest and neither means off. Invalid settings
fail validation before allocation. At reduced render extents this setting is
inactive; `UpscaleSettings::sharpness` retains its display-linear behavior.

`RcasDomain::DisplayLinear` applies the existing RCAS equations directly to
RGB. `RcasDomain::EncodedOutput` applies the output transfer to each RGB tap
before the same kernel: sRGB outputs use the piecewise sRGB OETF, while linear
UNORM and float outputs use identity transfer. Alpha is copied without transfer
or filtering. Border loads clamp to the edge. Both domains use full-precision
f32 division, including `1 / (1 + 4*lobe)`, rather than the reference's approximate
reciprocal. No input tap is quantized to bytes.

The final native RCAS input is `Rgba32Float`, one extra texture per admitted slot.
For sRGB encoded-domain filtering the shader inverses the transfer on its result
so the attachment's OETF encodes it once before quantization. Values saturate to
`[0,1]` for the encoded sRGB output. UNORM conversion rounds to the nearest code;
transfer implementation error and rounding at code boundaries permit a difference
of one 8-bit code from the scalar f64 reference, including ties. The tiny known-edge
GPU test checks that bound on each channel. The direct display-linear float test
uses absolute error `0.00001` for f32 arithmetic; it has no half-float store.

Interactive viewer frames select `FrameProfile::HdrNativeRcas { samples, tone_map,
rcas }`. Retained captures select `RetainedCaptureRecipe::with_native_rcas(rcas)`,
which validates the same settings and requires an HDR frame profile. Each capture
sample runs at full raster resolution without reconstruction; the recipe override
is restored to the interactive profile afterward. Supersampling and temporal
accumulation combine the sharpened samples through the existing capture path.
Feature readbacks retain their existing non-accumulated recipe restriction.

### Third-party license

`src/upscale.wgsl` ports EASU and RCAS from
[AMD FidelityFX Super Resolution 1, ffx_fsr1.h](https://github.com/GPUOpen-Effects/FidelityFX-FSR).
Copyright (c) 2021 Advanced Micro Devices, Inc. All rights reserved.
Licensed under the MIT License. The full copyright, permission and warranty
notice is preserved in the shader file.

`src/smaa.wgsl` ports SMAA 1x from
[SMAA.hlsl](https://github.com/iryoku/smaa/blob/master/SMAA.hlsl), and `src/smaa_lut.rs` ports
the lookup-texture generators `Scripts/AreaTex.py` and `Scripts/SearchTex.py` from the same
repository. Copyright (C) 2013 Jorge Jimenez, Jose I. Echevarria, Belen Masia, Fernando
Navarro and Diego Gutierrez. Licensed under the MIT License. The full copyright, permission
and warranty notice is preserved in both files. The port was written from the reference
source. No code was copied from Bevy's WGSL port.

`src/cmaa2.wgsl` is a modified port to WGSL of
[CMAA2.hlsl](https://github.com/GameTechDev/CMAA2) (Conservative Morphological
Anti-Aliasing 2.3, author Filip Strugar). Copyright (c) 2018, Intel Corporation. Licensed
under the Apache License, Version 2.0. The full license text is in
[LICENSE-APACHE-2.0-CMAA2.txt](LICENSE-APACHE-2.0-CMAA2.txt). The shader file carries the
copyright and license header verbatim, a statement that it was modified, and the list of
changes. The upstream repository has no NOTICE file.

`src/cas.wgsl` ports the sharpen-only path of
[AMD FidelityFX CAS, ffx_cas.h](https://github.com/GPUOpen-Effects/FidelityFX-CAS) (ffx-cas/ffx_cas.h).
Copyright (c) 2017-2019 Advanced Micro Devices, Inc. All rights reserved.
Licensed under the MIT License. The full copyright, permission and warranty
notice is preserved in the shader file.

### Numerical checks and informational timing

The scalar f64 reference and f32 GPU kernels are compared with an absolute
per-channel tolerance of 0.0001. Reconstruction is compared without RCAS. RCAS is
compared against the half-float reconstruction it reads, captured by rendering
the same reconstruction into an Rgba16Float target, so the check does not depend
on the driver's rounding for that store. HDR composition allows one 8-bit code step
for attachment transfer and quantization. Fixtures cover constant fields,
vertical and 45-degree edges, a steeper rasterized edge, full-range and
mid-range checkerboards, single-pixel inputs, and odd extents. The EASU
nearest-four clamp bounds each result; arbitrary rasterized slopes can have
small reversals within those bounds.

Run the informational timing test on the selected Vulkan device:

```sh
cargo test -p worldrunner-render-post gpu_post_chain_scale_timings_are_informational -- --nocapture
```

It reports three post-chain GPU samples for each of 0.5, 0.77 and 1.0 scale at
320x180 output. Samples include tone mapping and enabled reconstruction/RCAS.
The first sample can include driver warm-up. These timings have no performance
threshold and exclude scene rendering. Linux CI selects Mesa lavapipe with
`VK_ICD_FILENAMES` and `WGPU_BACKEND=vulkan`.

## Separable Gaussian bloom

Set `HdrPostSettings::bloom_kernel` to `BloomKernel::SeparablePyramid` to
select Gaussian bloom. `BloomKernel::Tent` remains the default and retains the
existing binomial reduction and tent reconstruction. `bloom: None` schedules no bloom passes for either
kernel. The shared `Bloom` parameters control threshold, quadratic knee, level
count (1 through 8), and final intensity. The Gaussian option supplies eight
`GaussianLevel` entries; only the requested prefix is read. Each entry specifies
an integer radius (0 through 64 texels), RGB tint and scalar weight. Tint and
weight must be finite and in 0 through 64. Radius zero is an identity blur.

```rust
use worldrunner_render_post::{Bloom, BloomKernel, GaussianLevel, HdrPostSettings};
let settings = HdrPostSettings {
    bloom: Some(Bloom { threshold: 1.0, knee: 0.5, intensity: 0.7, levels: 5, policies: Default::default() }),
    bloom_kernel: BloomKernel::SeparablePyramid([GaussianLevel {
        radius: 44,
        tint: [1.0, 0.9, 0.8],
        weight: 0.2,
    }; 8]),
    ..HdrPostSettings::default()
};
settings.validate().unwrap();
```

The first pass applies the bright pass independently to four source texels and
averages them into a ceiling-half-resolution target. Later levels average 2x2
blocks with edge clamping, so the last odd row and column remain represented.
Each extent is `ceil(previous / 2)` on each axis. By default, a requested level
after 1x1 is rejected by the factory before target allocation. Set
`bloom.policies.terminal = BloomTerminal::Repeat` to retain the requested level
count on small extents. `gaussian_extents(size, count, terminal)` returns the
extents the selected policy admits.

After all downsample passes, levels execute from coarsest to finest. A horizontal
Gaussian blur writes a scratch target. The vertical blur writes back into that
level's original downsample target, applies its tint and weight, and adds the
bilinearly upsampled result from the next coarser level. The terminal vertical
pass has no coarser contribution. The final half-resolution image is bilinearly
upsampled during tone mapping and multiplied by `Bloom::intensity`. Alpha comes
from the original scene. Unit tints and weights summing to one preserve a
constant field's post-bright-pass radiance. Other weights deliberately change
energy.

The CPU generates a normalized discrete Gaussian with `sigma = radius / 3`.
Adjacent positive taps are folded into one bilinear sample with a fractional
offset and mirrored around the centre. Each axis requires
`1 + 2 * ceil(radius / 2)` filtered samples per pixel. Folded taps live in an
immutable storage buffer and are read directly by the shader. Each level owns
two RGBA16Float textures and a tap buffer per admitted slot. The original
level target is reused after its downsample consumers have executed. Resize and
reconfiguration use the existing transactional `PostChain` replacement path.

Every pass has a stable timestamp identity: `gaussian_bright` at level zero,
`gaussian_downsample` at subsequent levels, and `gaussian_horizontal` /
`gaussian_vertical` at each logical level. Enable the existing chain timing API
to collect these individually. The explicit synthetic probe renders a
1600x900 frame with radii 44, 32, 20, 12 and 6, discards five warm-up samples
and reports per-pass medians over twenty samples:

```sh
WGPU_BACKEND=vulkan cargo test -p worldrunner-render-post --lib \
  gaussian_bloom_stage_timestamps -- --ignored --nocapture --test-threads=1
```

The shader reads taps directly from the storage buffer. Passing the tap array to
a helper by value and indexing it dynamically produced the same output but cost
about 236 ms per frame on an RDNA3 integrated GPU, against about 1.2 ms for direct
access, so that form is not used.

Numerical tests compare each odd-extent pyramid level to a discrete CPU
convolution, including centre/edge impulses, radius 0, 1, 4, 44 and 64, threshold,
knee, tint and weights. The GPU tolerance is `0.004 * abs(reference) + 0.0001`,
accounting for half-float stores and hardware bilinear interpolation. CPU folded
weights preserve normalization and discrete convolution within `1e-6` for every
supported radius.

### Authored pyramid policies

`Bloom::policies` configures the same separable pyramid through `BloomPolicies`.
Use `BloomPolicies::default()` when migrating existing `Bloom` literals. Defaults
retain per-source-texel soft-knee extraction, box reduction, radius/3 kernels,
vertical tint, all levels blurred and output level zero. Their stage identities
and arithmetic order remain unchanged. Nondefault policies require
`BloomKernel::SeparablePyramid`; the tent path rejects them during validation.

- `BrightExtraction::SoftKnee` retains the max-channel quadratic knee.
  `ToneCurve(ToneCurveBright)` sanitizes and clamps RGB to `maximum`, computes
  luminance with `luminance` weights, and evaluates
  `s = max(T(exposure * exposure_scale * luminance) - T(threshold), 0) * contrast`.
  It returns `rgb * s / max(max_channel(rgb), epsilon)`. `BrightCurve` selects
  ACES filmic `x*(2.51*x+0.03)/(x*(2.43*x+0.59)+0.14)` or Reinhard `x/(1+x)`.
  These curves are evaluated without an additional output clamp. Maximum and
  epsilon must be finite in `(0, 65504]`; exposure scale and contrast must be
  finite in `[0, 65504]`. Luminance coefficients must be finite in `[0, 1]` and sum
  to one within `0.0001`. Exposure remains a caller-owned GPU value. The tone
  curve maps `Bloom::threshold` into its output range, so with this extraction
  the threshold may be any finite value in `[0, 65504]`. The soft knee compares
  colour values directly and keeps the `[0, 64]` threshold range.
- `BloomReduction::BoxPerTexel` is the legacy mode: extraction runs
  independently on four integer source texels before averaging. Later levels
  average four integer texels. `QuarterTexelAfterAverage` averages four bilinear
  clamp-to-edge samples at destination centre plus/minus one quarter **source**
  texel on each axis, then extracts brightness on the first reduction. Every
  later reduction uses those same positions. Destination centres map through
  normalized coordinates using the ceiling-half target extent, including odd
  source extents.
- `GaussianKernelRule::RadiusThird` retains the original radius and sigma.
  `AxisClampedPairs` computes each axis independently: `r = min(radius,
  max(axis_extent, 1))`, `N = ceil(r/2)`, support `2*N`, and
  `sigma = (2*N+0.5)/3`. Radius zero remains identity. Symmetric taps normalize
  to one and adjacent taps fold into bilinear pairs. Horizontal and vertical
  passes bind their own immutable storage weights for this rule. The public
  `parameters` and `weights` methods expose the CPU calculation.
- `BloomLevelSelection` separates `first_blurred` and `output` from
  `Bloom::levels`, the number of reductions. Indices begin at zero for half
  resolution. Validation requires `first_blurred <= output < levels`; levels
  below `first_blurred` remain bright-only reductions. For six reductions with
  a bright-only half-resolution level and quarter-resolution output, select
  `{ first_blurred: 1, output: 1 }`. Coarser blurred levels still fold additively.
- `BloomTerminal::Reject` is the default and rejects any requested level after
  the first 1x1 reduction. `Repeat` keeps all remaining logical levels at 1x1.
  Each is reduced, blurred, tinted, weighted and folded independently through
  the existing pyramid. Selection indices still address logical levels, and
  `AxisClampedPairs` clamps each radius to the 1-texel axis before pairing.
  For six levels, a 1x1 source produces six 1x1 levels; 17x9 produces 9x5,
  5x3, 3x2, 2x1, 1x1, 1x1; 32x32 produces 16x16, 8x8, 4x4, 2x2, 1x1, 1x1.
  This option applies to both `BloomPyramid::append` and stock HDR bloom.
- `BloomTintPass::Vertical` retains the original tint placement.
  `Horizontal` applies RGB tint before the horizontal half-float store; weight
  remains in the vertical pass before additive folding. Moving tint changes
  intermediate half-float rounding, so the two placements can differ slightly.

### Bloom in a caller schedule

`BloomPyramid { bloom, levels }.append(device, set, scene, exposure)` appends
only the existing bloom passes and retains their textures and weight buffers
in the caller's `StageSet::resources`. It returns an RGBA16Float output view at
`selection.output`. The output is folded bloom **before final intensity**; the
caller multiplies by `Bloom::intensity` in its own composite. No exposure,
tone-mapping, anti-aliasing or composite stage is scheduled by this builder.

The scene must be a single-sample, single-mip, single-layer, filterable D2 color
texture view with `TEXTURE_BINDING` usage, covering the complete texture. For
`ToneCurve`, pass `Some(&exposure_buffer)`: its first four bytes are a native
f32 nonnegative exposure and its usage includes `STORAGE`. Additional trailing
bytes are permitted. `SoftKnee` accepts `None`. Invalid settings, extents and
missing/incompatible exposure buffers return `PostError` before appending any
stages or resources. The caller must supply matching-device handles and a view
of the complete texture.

Schedule exposure writes or copies before appending bloom stages, then append
the caller's composite using the returned view. Keep per-frame updates ordered
with the host's frame admission and submission contract. Updating the bound
buffer changes extraction on the next execution without rebuilding pipelines.
The stock HDR path supplies its admitted-slot exposure when adaptation is
installed, or its configured fixed exposure otherwise.

Synthetic GPU references exercise both extraction curves, exposure changes
between frames, both reduction modes, even and odd extents, independently
clamped axis kernels, tint placement, six reductions with a bright-only first
level, and a caller-owned exposure-copy and composite schedule. Each RGB channel
allows `0.006 * abs(reference) + 0.0002` for accumulated half-float stores and
bilinear interpolation. The existing default-policy comparisons retain their
original tighter tolerance.

## Temporal antialiasing

`PostResolve::Taa(TaaSettings)` (the scene recipe `SceneAntiAlias::Taa { history_micro, clip_scale_micro }` authors both fields; `history_weight` 0.0 to 0.99, default 0.9; `clip_scale` 0.5 to 2.0,
default 1.0) adds three stages, `taa resolve`, `taa history` and `taa depth history`, ahead of the luminance histogram, so
exposure metering, bloom and the tone stage read the TAA output. `taa.wgsl` follows Karis 2014 (color box and
compressed-luma clip); the current frame uses the nearest jittered sample (1x; a multisampled
frame is not jittered, so the sample is the MSAA pixel and a static 4x image equals MSAA alone,
0.00046 against 0.00161 with jitter at 64x64; resampling the shifted frame gave 0.00153), the box is the
3x3 min/max, the blend is linear and history is read with Catmull-Rom; the history weight is scaled by `clamp(1 - k * offset_px, 0, 1)` (a motion-adaptive blend factor, as in Karis 2014 and later TAA talks), k = 1.5 at 1x and 16 on multisampled frames, so static pixels keep the full weight (static 1x 0.00201 against 0.00436 aliased, static 4x equal to MSAA alone) and in motion 4x TAA equals MSAA alone (0.00036 and 0.00043 against 0.00036 and 0.00043) while 1x TAA is 0.88x and 0.73x the history-reset error (mean over frames, +41 and -29 mm per frame); the cost is less supersampling of moving pixels. A multisampled target is never jittered; `taa_reference.rs` is the f64 reference the GPU tests compare against.

`taa resolve` reads the resolved scene color, the frame depth (`texture_depth_2d`, or
`texture_depth_multisampled_2d` with a per-sample minimum when MSAA is on), the history texture and
the per-slot uniform block, and writes the TAA image. `taa history` copies that image into the
history texture with `Operation::CopyTexture` (`copy_texture_to_texture`, encoded outside a render
pass, so it has no timing slot). One `Rgba16Float` history texture per render size is shared by all
frame-slot stage sets, so the history is the previous frame whichever slot ran it and however many
frames are in flight. A resize replaces it, and settings without TAA release it. The shader ignores
the history while the block says it is invalid, so no clear is needed.

`taa resolve` also binds an object-motion residual at `@binding(5) velocity` (`taa_gpu::Inputs::velocity`
with `velocity_samples`). The format is `Rg16Float`; the value is the object-only residual in
previous-frame uv units, `uv(P_prev * world_prev) - uv(P_prev * world_now)`, where `world_prev` is the
same surface with the previous transform and animation time. `history_uv` adds the residual of the
closest-depth texel to the camera reprojection (Karis dilation: depth and velocity come from the same
texel, and from the minimum-depth sample when the target is multisampled, where the binding is
`texture_multisampled_2d` and its sample count must equal the depth's). Zero means camera
reprojection alone, so a cleared target needs no special case. With `velocity: None` the resolve binds
a 1x1 zero texture of the same sample count, which is what a scene without velocity pipelines gets:
object passes of `render-scene` write the wind residual only when built with `with_taa_velocity`
(`ShadowedTarget::velocity` for shadow receivers; SSR scenes write it when the colour budget fits), and
output is unchanged from a build without the binding. The motion-adaptive
weight reads the total displacement including the residual. The reference and GPU tests for this are in
`taa_gpu_tests/velocity_tests.rs`.

Measured at 1920x1080 on a Radeon 780M with `smaa_and_fxaa_stage_timestamps` (median of 5, stripes
and disc scenes). The fullscreen `taa history` pass took 0.87 to 0.97 ms of a 2.99 to 3.18 ms timed
TAA path (`taa resolve` 1.81 to 1.89 ms, `taa history`, `tone` 0.31 to 0.33 ms). The texture copy has
no timestamp, so the probe also times 24 frames in one submission without queries: the whole chain
(scene paint, TAA and tone) fell from 3.95 and 3.86 ms per frame to 3.37 and 3.39 ms, so the copy
costs about 0.4 ms against 0.9 ms for the pass, a saving of 0.5 to 0.6 ms per frame. Sampling the
resolve output into a second history target and swapping the two needs bind groups that change per
frame, which slot-static stage sets cannot hold, so it was not tried.

`TaaUniform` is the per-frame block, `TAA_UNIFORM_BYTES` = 96 bytes, little-endian, one uniform
buffer per frame slot uploaded by `ToneMapOutput::prepare_taa`:

| Bytes | Content |
| --- | --- |
| 0..64 | `reprojection`, column-major `mat4x4<f32>`: previous unjittered view-projection times the inverse current one, computed in f64 by the host |
| 64..72 | `jitter_pixels` x, y (+y down) |
| 72..76 | `history_valid`: 1.0 when the history texture holds the previous frame, else 0.0 |
| 76..80 | zero |
| 80..88 | `depth_terms` `a`, `b` (two `f32`): linear view depth of device depth `z` is `b / (z - a)`; `b` = 0 disables the disocclusion test |
| 88..96 | zero |

An all-zero block means no history: the stage returns the reconstructed current color. The block is
built by `FrameRenderer` from the camera data, so this crate needs no matrix library.

History lifecycle: the first frame, `reset_temporal_history`, a resize, a jitter change and a
cancelled or failed transaction give `history_valid` = 0. The host must draw into
`FrameTargets::depth()`.

### TAA depth history

The 96 bytes are the 80-byte block of the reprojection, jitter and `history_valid`, then `depth_terms` `(a, b)` at bytes 80..88 and 8 bytes of padding. The linear view depth of device depth `z` is `b / (z - a)` with `a = far / (far - near)` and `b = -near * far / (far - near)`; `b == 0` disables the disocclusion test. `render-scene` reads the terms off the unjittered view-projection (the z row minus `a` times the w row is the translation lane), so no near or far plane travels with `TemporalView`.

`taa resolve` writes a second target, the linear depth of the closest-depth texel (`R32Float`, `taa_gpu::DEPTH_FORMAT`), through `StageTarget::Pair`, and `taa depth history` copies it into the depth history with `Operation::CopyTexture`, as the color path does; the frame depth itself cannot be copied (it may be multisampled and has no `COPY_SRC` guarantee). The depth history has the lifecycle of the color history: one texture per render size shared by all frame slots, created and replaced together with the color history, released by `release()` when TAA is no longer requested, and reset only through `history_valid`. The resolve reads it at `@binding(6)` with `textureLoad`.

Measured at 1920x1080 on a Radeon 780M with the same probe (stripes and disc scenes, median of 5): `taa resolve` takes 1.99 and 2.01 ms with the depth history against 2.04 and 1.93 ms before it, and the untimed whole chain takes 3.99 and 4.18 ms per frame against 3.78 and 3.77 ms, so the second target and the depth copy cost 0.2 to 0.4 ms per frame.

### FXAA quality search

`FxaaKernel::Quality { preset: 12, sample_scale: [1.0; 2] }` selects axial early
exit, weighted orientation and preset edge search in the existing pass. Sobel stays
the default with unchanged bytes. Only preset 12 is accepted. X/Y search and final
offset scales are finite in (0, 4] texels; contrast taps stay one texel apart.
Scene quality settings expose the same option with scales in millionths of a texel.
See [quality settings and math](../../dev-docs/rendering/post-filters.md#quality-fxaa-kernel).
