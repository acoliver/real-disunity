# Adaptive material pages for rectangular heightfields

## Delivery boundary

`SceneGpu::enable_terrain_pages` connects decoded layered sources to the existing terrain mesh and shader. Camera-aware scene, resident and shadowed composition select pages each frame. The stock path is unchanged unless the scene explicitly enables pages. The standalone `terrain_material_pages` integration test renders square and rectangular heightfields with different material resolutions and colors, and pins exact stock pixels and pass identities.

## Topology and address space

One root covers one rectangular Z-up heightfield. Columns advance east and rows advance north. Geometry interval counts are positive, independent per axis and need not be powers of two. Material pages partition normalized terrain coordinates with a square quadtree, so their world-space footprints can be rectangular. A maximum depth of 16 bounds the address space; no fixed geometry interval count is assumed.

A page address contains depth and east/north coordinates at that depth. Parent, same-depth neighbor and resident ancestor UV mappings are exact dyadic operations. Out-of-bounds neighbors have no address. A requested sampling ancestor must contain the desired page.

## Settings and selection

`TerrainPageSettings` validates interior dimensions, padding, filter support, depth, leaf limits, hysteresis and resident/upload byte limits. Every budget must fit at least one complete page. There are two four-byte linear products per texel, including every generated mip.

Selection consumes projected material-texel size in thousandths of a screen pixel. A node splits strictly above the split threshold. Previously split nodes remain split at or above the lower merge threshold. Scene integration projects page edges at the terrain height bounds with the frame camera matrix and viewport. Both screen axes contribute in pixels. Selection history publishes only on successful submission. The preparation API also accepts a caller-provided metric.

Splits are admitted by decreasing error, then increasing page address for ties. A split replaces one leaf with all four children and costs three leaves. The hard leaf limit preserves complete non-overlapping coverage when finer detail cannot be admitted.

## Material products

Each texel consumes between one and 256 decoded layer samples. Layer color is linear RGBA8, normal is a signed tangent-space direction, roughness is perceptual UNORM8 and coverage is an unsigned 16-bit weight. Color and roughness use integer weighted means with ties rounded upward. Normal directions are normalized before coverage weighting and after composition. Layer composition rejects zero contributing normals, zero total coverage and complete normal cancellation.

Products contain linear RGBA8 color and packed normal XYZ plus perceptual roughness. No sRGB conversion occurs. Resident storage uses `Rgba8Uint` to preserve exact bytes in compute. The existing terrain shader has a cached variant that explicitly decodes and bilinearly filters integer texels, then interpolates safe mip levels. The cached products describe nonmetallic materials. Sources supplied through `TerrainPageSource::sample` return decoded linear layers at normalized global coordinates. They may be procedural or sample decoded source images. The callback owns its boundary rule and must evaluate neighboring content for gutters. Enabling pages explicitly replaces direct palette evaluation; it does not decode compressed scene textures automatically.

The CPU composer evaluates texel centers, including gutters, in global normalized terrain coordinates. A source sampler evaluates neighboring source content across internal page boundaries. At the outer heightfield boundary it applies its declared source boundary rule. Internal gutter texels cannot be filled by clamping the current page interior.

## Mips and filter footprints

The complete padded extent halves by integer division per axis, with a minimum of one. Reduction uses exact area-overlap weights, so a 5-to-2 reduction shares the middle source texel between its two destinations. Odd source edges are not dropped or duplicated. Normal reduction decodes and normalizes directions before averaging. When the weighted vector sum is exactly zero, both reducers preserve the first contributing source direction in row-major order. This defines an antipodal tie without inventing a default direction, and applies equally to uploaded images and prepared GPU buffers. Layer composition still rejects cancelling layers at a single source texel. GPU vector reduction uses f32; the independent CPU reference accumulates in f64. Exact packed-byte comparisons cover the tested nonflat normal patterns, not every possible cancellation or rounding threshold.

All levels preserve the base padded domain in normalized UV space. `maximum_sampling_mip` limits sampling to levels where both axes retain the declared complete filter radius and the padded extent and interior are divisible by that level's power-of-two reduction factor. These constraints align neighboring reduction bins in source space. Odd-size chains are still generated completely, but nonaligned levels use ancestor fallback. Later levels are generated and tested but cannot be sampled under that configuration. Coarser world-space footprints use a resident ancestor instead of reading outside a page's supported domain.

GPU cache admission limits padded page area to `u32::MAX / 256` texels, in addition to device texture limits. This bounds integer area-weighted color/roughness sums and their rounding additions without overflow. Geometry interval counts are independent of this material-resolution limit.

## Portable source anisotropy

`TerrainLayeredSource::anisotropy` accepts the validated levels 1, 2, 4, 8 and 16. Level 1 preserves the largest-axis isotropic gradient path byte for byte. Above 1, the shader measures both UV footprint basis vectors in source texels, choosing the longer as the major axis. For lengths `rho_max` and `rho_min`, it takes `N = max(1, min(ceil(rho_max / rho_min), level))` equally weighted taps centred along the major UV vector. The LOD is `log2(max(rho_max / N, rho_min))`, clamped to the image's mip range. A zero minor footprint uses the authored tap limit; a zero footprint uses one base-level tap. This bounded line integral approximates the footprint, rather than integrating an ellipse.

Each tap uses explicit-level trilinear filtering. Both hardware samplers keep `anisotropy_clamp = 1` at every authored level, avoiding backend-specific explicit-gradient anisotropy and double filtering. Repeat addressing applies to source layers; colorization remains clamped at mip zero. Material-level `SceneMaterial::with_anisotropy` does not affect terrain.

An independent f64 reference filters the authored byte mip chain and checks tiny rectangular fixtures within three byte values. That bound covers three 8-bit filter-weight roundings over the fixture's 180-value range plus final byte packing. Row and quad grazing regressions retain sampled values, adapter capabilities and gradients in failure messages. Separate baseline tests require non-flat isotropic detail, ratio-one tests require the same output as 1x, and a main-branch byte fixture pins the default path.

Cached-page reads are the next slice in [#347](https://github.com/acoliver/real-disunity/issues/347); they remain isotropic. Their `Rgba8Uint` storage requires applying the same bounded major-axis taps to the existing integer trilinear reader, with gutter-safe ancestor selection for every tap. That extension must preserve the 1x lookup and filtering bytes and test page boundaries, rectangular pages and grazing-angle detail.

## GPU execution

`CompositionBatch` validates and packs variable layer spans. Its `TypedOperation::Compute` uses checked buffer bindings and explicit read/write declarations. Source ranges and samples must be produced by uploads of that batch's byte slices. Output is write-only scratch from the executor's perspective, despite WGSL's read-write storage binding syntax.

`PageWriter` retains store and mip-reduction pipelines for one padded extent. `encode` uploads already-composed packed products. `encode_prepared` accepts a checked packed buffer and producer operations, allowing GPU layer composition to feed residency without CPU readback. The producer list, base store and all mip dispatches compile through one `ResourceRegistry`. Each slot occupies two array layers, color followed by normal/roughness, with checked disjoint subresources for every mip. No page module owns a queue or submits work.

## Residency and publication

`ResidentPages` allocates its array through the existing `Configuration` allocation preflight and replacement boundary. Reservations consume a free slot and per-frame upload allowance. Only submitted reservations enter lookup. Publication is a keyed edit in the existing submission transaction and occurs after enqueue. Dropped or cancelled frames release their reservations and publish nothing. Foreign submission authorities cannot reserve cache resources.

Resident references contain cache identity, slot and a monotonic token. `is_current` rejects references after eviction, edit invalidation, slot reuse or whole-cache replacement. Pending reservations carry the same identity; invalidation removes their eligibility, so an edit between encoding and submission cannot publish stale content. Explicit eviction removes the lookup entry before a slot can be reused. Scene scheduling protects the root and every current selected-tree node. When full, it evicts the deepest submitted unused page, with address order breaking ties. Pending pages are never eviction candidates.

`invalidate` takes a changed normalized rectangle and removes every intersecting padded page footprint, including affected ancestors and neighboring gutters. Unaffected pages retain their identities. `SceneGpu::invalidate_terrain_pages` is the source-edit boundary. It advances the source generation and invalidates affected pages; material-wide changes invalidate the whole terrain domain. Sources with externally mutable data must invoke this boundary when that data changes.

Lookup chooses the nearest submitted matching-token ancestor with sufficient filter support. For a requested mip, each ancestor step subtracts one level from the required material mip, clamped at zero. This defines the material LOD policy. A missing root produces no terrain draw while streaming; other scene geometry still draws. The root clamps footprints larger than its final safe mip. The draw table is a compact selected quadtree with contiguous four-child blocks and resident slots. A shader lookup reads one record per traversed level, at most 17 records, independent of the leaf and cache counts. Off-tree residents are omitted. CPU key lookup uses an ordered key-to-slot index, reservation uses an ordered free-slot set, and token validation checks the token's slot directly. Draw preparation snapshots token-checked resident references, and edits after preparation fail before sampling stale references.

Scene devices request the selected adapter's full `max_texture_array_layers` capacity. Desktop and windowless hosts use this same device creation path; other device limits keep their existing requests. Layered scene policies require three array layers per slot. Before uploading scene resources, admission compares that count with the actual device limit and returns `DrawError::TerrainPages(ConfigurationError::TextureArrayLayers { required, available })` if it cannot fit. Direct cache creation and replacement perform the same check, with two layers per linear-material slot or three per layered slot. Each shared layered-source array is checked independently against that limit before generic source and allocation validation, returning the same typed error. Layer counts from separate arrays are never summed. No slot count is silently reduced.

## Budgets and retirement

The settings resident ceiling covers both texture-array products and every mip. The caller also supplies the existing `AllocationBudget`, which covers the array, packed input buffers and executor upload staging. Scene integration additionally reserves three bounded draw tables and their upload staging before cache allocation. Hosts must follow the existing admitted-frame-slot reuse contract. Slot capacity must hold the bounded selection tree so a requested capture can reach exact residency. Packed inputs need only buffer-copy alignment, not texture-row padding, because the GPU store kernel writes the array directly. Source layer/range allocations supplied to `encode_prepared` require separate caller admission.

Scratch capacity equals the configured whole-page uploads per frame. Reservations hold that capacity through submission completion, even after page eviction or edit invalidation. Another frame receives `ScratchFull` until enough earlier work retires. Cancellation releases the hold. Thus overlapping frames cannot multiply the declared scratch allocation. The per-frame byte allowance remains a separate limit and resets with transaction identity.

Whole-array replacement admits old plus new arrays and both generations' scratch allowances before allocation. Existing configuration retirement retains old storage and its lease until preceding submissions complete. A failed replacement leaves the current cache unchanged.

## Capture readiness

A `TerrainResidencyRequest` records scene identity, source generation and an exact camera selection. `terrain_residency_ready` rejects foreign or edited-generation requests. Ancestor coverage does not satisfy a request for a leaf. `OffscreenTarget::render` declares its camera selection and submits bounded preparation batches until those exact pages are published, waiting for each batch to retire before admitting more scratch. Readback is encoded only afterward. Encoder-only hosts can declare and check the same request explicitly before appending capture copies.

## Verification

Tests compare every texel of every mip against the CPU reference for rectangular odd/even padded extents, including borders and nonflat normals. They cover composition-to-resident execution, distinct array slots, slot reuse, exhaustive small-tree neighbor/ancestor mapping, filter-safe ancestor lookup, edit footprints, pending invalidation, eviction, cancellation, foreign authority, exact replacement peaks, per-frame limits and scratch held across submissions.

GPU sampling tests pin ancestor fallback, LOD transitions and nonconstant adjacent-page gutters at base, fractional and integer mip levels. Standalone capture tests pin exact sRGB bytes away from rounding boundaries for two configurations, source-generation invalidation and unchanged disabled output/pass identities. Sources remain pathless decoded inputs; compressed palette decoding is outside this optional source API.
