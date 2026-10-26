# Checked draw providers

A host can keep its own shaders and draw code and still run them inside the engine executor and submission authority, mixed with stock draws. Run the procedural example with `cargo run -p worldrunner-render-post --example bridge`. The example needs a GPU adapter and opens no window.

Providers contribute immutable `CheckedDraw` packets to `TypedWork::Draws`. Packet order is draw order within one executor-owned render pass. The executor performs the declared clear, load, store and attachment resolves once for that pass. A provider cannot begin or end a pass, obtain an encoder or queue, or insert an undeclared clear.

## Packet and binding contracts

`CheckedDraw::new` creates a procedural nonindexed packet. `CheckedDraw::with_geometry` also accepts checked vertex and instance streams, an optional checked index slice and index format, element and instance ranges, and a base vertex. Vertex slots match the retained pipeline vertex ABI. Byte slices, fetch extents, alignment and usage are validated before pipeline creation. Indexed element bounds are checked against the index slice; index values remain GPU data and use the device's vertex-fetch bounds protection. Compilation checks the actual bindings against declared allocation generations, versions and ranges, including whole-pass buffer hazards.

Both constructors rebuild the pipeline layout from the actual checked groups. `CheckedBindings::build_with_resources` accepts textures, buffers and `SamplerBinding` entries. `CheckedSampler` derives filtering, nonfiltering or comparison classification from its creation descriptor. Aggregate dynamic-buffer and per-stage resource limits are checked per packet before creating its pipeline layout. No opaque group plus unrelated resource metadata is accepted.

`CopyOutput::checked_draw` uses the existing nearest-copy shader. Its source must be a single-sample D2 linear-color view with the same float, renderable format as the output. It reuses the stock output-format policy and rejects unsupported output sample counts with `ContractError::CopyOutput` before GPU creation. Existing stock composition paths are unchanged.

## Rendering boundaries and ownership

`TypedWork::Resolve` declares a color-only MSAA resolve boundary. It reads a stored multisampled version and writes a new single-sample version through a load/store pass with no draws. `ColorAttachment::resolve` instead resolves at the end of its authored drawing pass. Neither operation implies a hardware depth resolve.

The extended bridge fixture renders three color targets and depth at 1x and 4x, preserves the first color and depth snapshots, then loads the attachments for a second provider pass. Separate assertions check both boundaries and confirm the first snapshots remain unchanged. A checked fullscreen depth shader selects the minimum multisample depth. Sample-specific depth values make selecting sample zero observably incorrect.

Owned offscreen targets use ordinary registry imports and retain clone-based allocation identity. Externally owned frame targets use `FrameOwner` and `ResourceRegistry::import_leased`. The host moves the actual frame into its owner before importing its texture. A lease belongs to one submission authority. Checked encoding retains that lease in the prepared transaction, and submission retains the actual owner through the completion callback while recording its last submission serial. `FrameOwner::retire` returns the external frame only after prepared and submitted uses retire. Retirement or owner drop invalidates further checked imports, views, encoding and submission; an exported view cannot renew permission. Providers receive checked targets and packets, never the host owner or submission authority.

## Production regressions

`CompiledOperations::encode_checked` needs no draw callback and rejects raw-callback raster operations. Existing encoder-based entry points remain available to engine composition code; they are not supplied to draw providers. Cloned imports preserve allocation identity. Conflicting imports, stale generations, missing producers, read-only writes, undeclared bindings and missing cross-segment write ownership are rejected.

Tests exercise authored mixed-provider order, indexed instancing with nonzero byte and element offsets, dynamic uniform offsets, a selected-array-layer depth-only draw, filtering material and depth comparison samplers, and constructor rejection without GPU validation errors. A prefix readback drives a continuation-produced uniform upload consumed by a checked mesh packet. Cancellation discards staged CPU publication and submits no commands. Capture compilation validates both segments together without changing stage identities. The suffix is encoded only after prefix completion.
