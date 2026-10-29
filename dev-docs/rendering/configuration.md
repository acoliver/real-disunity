# Budgeted configuration replacement

`render-gpu::configuration::Configuration<T>` owns a published value and its allocation inventory. It reserves the owning submission authority before preparing a replacement. A prepared but unsubmitted frame prevents replacement; submitted frames may remain in flight.

The inventory is checked before the allocator runs. Peak usage includes the published generation, every unretired generation and the proposed replacement. Retained allocations with the same name and descriptor are counted once. Names for retained allocations must identify the same physical storage across generations. A new backing allocation must use a different name or an ordinary, non-retained entry.

Texture entries account for compression blocks, mip extents, layers and samples. Depth24Plus uses four bytes per texel. Stencil8 and combined depth/stencil use a conservative eight-byte backing bound, including backend stencil emulation and depth promotion. Readback entries use 256-byte row pitch. Counts describe physical allocation multiplicity, rather than the number of views or references. Buffer entries must include their actual padded size. CPU accumulation and temporary storage can be declared separately. These are descriptor bytes, not a measurement of driver-private bookkeeping or total process memory.

`Configuration::new` adopts an already constructed value. Custom builders must preflight its inventory before constructing it and resolve GPU error scopes before returning a replacement. The stock constructor supplies that preflight and captures out-of-memory, validation and internal driver errors. Device loss takes precedence over an allocation result after construction and scope resolution.

The allocation closure creates private replacement objects. It must not publish CPU state or write shared GPU state. An error discards the replacement and leaves the previous value and generation published. A successful replacement publishes once at the admission boundary. `collect` releases old values after the authority's completion serial reaches their last preceding submission. Hosts should call it while polling, including when no new frame is requested.

`ConfigurationError::Budget` is a predictable configured-budget rejection. `Invalid` identifies an invalid inventory. `Allocation` reports an allocator or driver failure. `Submission` covers busy admission and device loss. None chooses a lower-quality rendering policy. Device loss stops admission and requires device recreation.

## Stock renderer

`render-scene::compositor::ConfiguredFrameRenderer` supplies the stock inventory and replacement builder. `RendererConfiguration` describes targets, HDR output, post settings, SSAO, reflections, water, active frame slots and timing. External persistent storage, transient uploads, readbacks and CPU capture storage belong in `additional`; imported references do not create additional physical allocations.

Stock effect implementations retain three physical slots. `frame_slots` restricts the active subset and does not reduce the physical inventory. Timing capability selection is fixed at device setup. Exposure carry and slot history remain retained across replacement, including temporary disabling of the HDR output. Exposure reset stays explicit. Unread timing storage is also retained.

The stock wrapper exposes explicit frame operations and a restricted `FrameEncoding` handle for stock scene composition. It does not dereference to the low-level renderer or expose resize, timing setup, registration or arbitrary borrowed features. Reconfiguration rejects a foreign device with `ConfigurationError::ForeignDevice` before validation or construction. Existing low-level renderer APIs remain separately available for unbudgeted use.

Custom compositions can use `Configuration<T>` with a builder that includes their feature objects and allocation inventory.

The viewer uses this transaction for attachment resize and retained settings changes,
with the positive `ViewerOptions::presentation_budget` byte limit (1 GiB by default).
Budget errors retain `required` and `budget` through `ViewerError::Configuration`.
Retained desktop and windowless hosts include their 256 MiB capture-recipe pool
in the initial inventory, declared with `AllocationSet::shared_cpu` so the single
host-wide reservation is counted once across replacements. Recipe admission does
not reconfigure targets. Actual
attachment replacement still counts the old and new generations because the
allocator constructs new targets before publishing and retiring the old ones.
Surface configuration remains a separate native operation requiring idle
submission exclusion.
