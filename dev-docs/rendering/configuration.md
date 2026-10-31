# Budgeted configuration replacement

`render-gpu::configuration::Configuration<T>` owns a published value and its allocation inventory. It reserves the owning submission authority before preparing a replacement. A prepared but unsubmitted frame prevents replacement; submitted frames may remain in flight.

The inventory is checked before the allocator runs. Peak usage includes the published generation, every unretired generation and the proposed replacement. Retained allocations with the same name and descriptor are counted once. Names for retained allocations must identify the same physical storage across generations. A new backing allocation must use a different name or an ordinary, non-retained entry.

`Configuration::reserve` admits independently owned storage and returns an `AllocationReservation`. Admission and replacement include every live reservation exactly once. Dropping its owner releases the charge. Reservations do not replace targets or advance the configuration generation. A pending scene owns its transient preparation reservation; scene texture storage belongs to its resident and remains charged through retirement. These inventories do not enter compositor generation descriptors.

Texture entries account for compression blocks, mip extents, layers and samples. Depth24Plus uses four bytes per texel. Stencil8 and combined depth/stencil use a conservative eight-byte backing bound, including backend stencil emulation and depth promotion. Readback entries use 256-byte row pitch. Counts describe physical allocation multiplicity, rather than the number of views or references. Buffer entries must include their actual padded size. CPU accumulation and temporary storage can be declared separately. These are descriptor bytes, not a measurement of driver-private bookkeeping or total process memory.

`Configuration::new` adopts an already constructed value. Custom builders must preflight its inventory before constructing it and resolve GPU error scopes before returning a replacement. The stock constructor supplies that preflight and captures out-of-memory, validation and internal driver errors. Device loss takes precedence over an allocation result after construction and scope resolution.

The allocation closure creates private replacement objects. It must not publish CPU state or write shared GPU state. An error discards the replacement and leaves the previous value and generation published. A successful replacement publishes once at the admission boundary. `collect` releases old values after the authority's completion serial reaches their last preceding submission. Hosts should call it while polling, including when no new frame is requested.

`ConfigurationError::Budget` is a predictable configured-budget rejection. `Invalid` identifies an invalid inventory. `Allocation` reports an allocator or driver failure. `Submission` covers busy admission and device loss. None chooses a lower-quality rendering policy. Device loss stops admission and requires device recreation.

## Stock renderer

`render-scene::compositor::ConfiguredFrameRenderer` supplies the stock inventory and replacement builder. `RendererConfiguration` describes targets, HDR output, post settings, SSAO, reflections, water, active frame slots and timing. Storage whose lifetime follows compositor replacement belongs in `additional`; imported references do not create additional physical allocations. Work and scene resources with independent owners use `ConfiguredFrameRenderer::reserve`.

Stock effect implementations retain three physical slots. `frame_slots` restricts the active subset and does not reduce the physical inventory. Timing capability selection is fixed at device setup. Exposure carry and slot history remain retained across replacement, including temporary disabling of the HDR output. Exposure reset stays explicit. Unread timing storage is also retained.

The stock wrapper exposes explicit frame operations and a restricted `FrameEncoding` handle for stock scene composition. It does not dereference to the low-level renderer or expose resize, timing setup, registration or arbitrary borrowed features. Reconfiguration rejects a foreign device with `ConfigurationError::ForeignDevice` before validation or construction. Existing low-level renderer APIs remain separately available for unbudgeted use.

Custom compositions can use `Configuration<T>` with a builder that includes their feature objects and allocation inventory.

The viewer uses this transaction for attachment resize and retained settings changes,
with `ViewerOptions::presentation_budget` (a fixed 1 GiB by default).
`PresentationBudgetPolicy::Derived` delegates to `AllocationBudgetPolicy` in the
existing configuration owner. The policy resolves the exact live replacement peak
before allocation, adding headroom and rounding up to a positive byte granularity.
An optional cap clamps the result and preserves typed budget rejection. Failed
replacement leaves the published configuration and effective budget unchanged.
The host reports the effective limit in `RetainedFrameStatistics::presentation_budget_bytes`.
`scene-viewer::plan_presentation` uses the same configuration descriptors and
capture reservations without constructing a device; recipe plans use the recipe's
raster extent and native RCAS selection. `PresentationPlanning` also carries the
retained effect-history state exposed by `ConfiguredFrameRenderer::has_effect_history`.
Planning and live replacement share `RendererConfiguration::allocations_with_history`,
which retains exposure storage after effects are disabled or a recipe is restored.
Budget errors retain `required` and `budget` through `ViewerError::Configuration`.
Retained desktop and windowless hosts include `ViewerOptions::capture_pool`
in the initial inventory, defaulting to a fixed 256 MiB. `CapturePoolPolicy::Derived`
evaluates the largest declared recipe once at host creation, at the policy's
nonzero `max_extent`, using initial frame samples and AO format. It uses the
presentation budget's checked headroom, granularity and optional cap rule.
Capacity stays constant across resize, effect reconfiguration and capture raster
overrides. The inventory uses `AllocationSet::shared_cpu` with the same descriptor
across replacements, counting the host pool once even after GPU retirement.
Planning and request admission share the recipe storage and pool checks, returning
`CaptureRequestError::RecipeBudget` on excess. Queued derived-policy reservations
cover the larger of current and `max_extent` storage, without checking the
recipe's own `max_bytes` at the hypothetical extent. Admission checks that limit
at the current extent; execution checks it again at the execution extent and
charges the frozen capture extent until result delivery. Larger extents are
accepted only when they fit. Growth beyond the declared maximum or the recipe's
own byte limit after admission can fail execution. Runtime reservation failures
remain `ViewerError::Driver`, including with the default fixed pool.
Constant capacity is exposed as `RetainedFrameStatistics::capture_pool_bytes`.
Recipe admission does
not reconfigure targets. Actual
attachment replacement still counts the old and new generations because the
allocator constructs new targets before publishing and retiring the old ones.
Surface configuration remains a separate native operation requiring idle
submission exclusion.
