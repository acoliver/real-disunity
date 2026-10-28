# Shared resources and authored raster passes

`worldrunner_render_post::contracts` describes physical texture allocations and the ordered raster passes that use them. A consumer with private kernels can supply its own MRT layout, semantic boundary tags and shadow/prepass order without adopting stock scene policy. Existing `StageId` values and timing identities are unchanged. Stock geometry lowers into this raster executor; its clear, load, store and draw order remain unchanged. Stock post stages retain their existing execution path.

## Identity and validity

`ResourceRegistry::import` accepts an actual texture, deduplicates cloned handles and records lifetime, semantic meaning and an optional initialized version. An initialized import asserts that the full allocation has valid contents at that version. Imported read-only allocations reject writes. Frame-local and persistent mutable allocations allow declared writes; these lifetime tags do not transfer GPU ownership or provide submission fencing.

`AllocationKey` identifies physical backing storage and its generation. For mutable allocations, `replace` requires different backing storage, increments the generation and removes initial validity. Every old checked view and binding then fails validation. Read-only imports reject replacement with `ReadOnlyReplacement`: no in-plan write could initialize that generation. Import the new backing separately with its initialized version; a rejected replacement leaves the old allocation and its bindings valid. The registry retains replaced handles to reject their reimport; it does not reclaim generations by completion ticket. Logical writes advance content versions independently of physical identity. Reimporting a clone cannot create another identity to evade overlap checks.

Views select nonempty mip and layer ranges and an applicable aspect from the recorded allocation. Views support D2, D2Array, Cube and CubeArray dimensions; see [typed operations](typed-operations.md) for copies and buffer ranges. Raster attachments select one mip and layer. Validity is tracked per mip, layer and aspect, not per rectangle. A read or load requires a matching initialized import or an earlier stored write. Overwrites invalidate earlier versions, and discarded writes cannot be consumed. A clear initializes only its selected subresource.

`ResourceRegistry::create` creates storage and records the same `TextureDescriptor::view_formats` declaration. Alternatively, `DeclaredTexture::new` retains the descriptor's declaration alongside its texture; register it with `import_declared`. For foreign textures, await `DeclaredTexture::from_texture(device, texture, descriptor)` with the descriptor used at creation. It cross-checks format, size, mip count, sample count, dimension and usage, then validates each alternate view in a native validation error scope. Descriptor mismatches return `TextureDescriptor`; invented or incompatible alternate declarations return `TextureDeclaration` without returning an invalid checked view. The public import interface accepts no raw alternate-format list.

`view_format` selects the allocation format or a retained alternate, otherwise returning `ViewFormat`. `view` and `view_dimension` keep the allocation format. `CheckedView::format` reports that selected format before native depth/stencil aspect resolution. sRGB views explicitly exclude storage binding and storage atomic usage; `CheckedView::usage` exposes the retained subset. Both access declarations and checked bindings enforce that subset, and storage layouts use the selected format. Attachment pipelines and resolves also match selected view formats, while hazards and versions remain shared across aliases.

Ordinary and leased imports remain native-format-only. Importing the same handle requires the same declaration list in the same order, lifetime, semantic and initial version. An ordinary import cannot later be upgraded to an alternate-aware import. Replacement clears alternate declarations along with initial validity; import a newly created declared texture separately when its alternates are needed.

`ResourceUse` declares read, write or read-write access and attachment, sampled, storage or copy usage. Raster attachment accesses are derived from their attachment declarations. Raster passes reject standalone attachment or copy uses, which cannot describe work executed inside their draw callback. Overlapping writes and writable attachment sampling fail regardless of their logical version numbers. Compatible read-only sampled, storage and depth attachment uses may overlap. Simultaneous read-only depth attachment sampling is unavailable on the GLES backend. `CheckedBindings::build` creates the actual texture layout and bind group from registry views, checking sample type, view dimension, multisampling, storage format/access, binding indices, per-stage limits, visibility and semantic tags. Storage support uses the format guarantees of enabled device features; adapter-specific extensions beyond those guarantees are not exposed by this contract. The entries remain attached to that handle. Put checked groups in `RasterPass::bindings`; compilation checks their effective views against declared access, and encoding installs those groups in list order. This does not infer private shader arithmetic or prevent a draw callback from using the existing raw render-pass API.

## Raster execution

`RasterPass::colors` is indexed and permits empty locations. Each occupied MRT slot declares exact load/store operations and an optional explicit resolve view. Resolves require a multisampled source, a single-sampled destination and matching extent, format and semantic meaning. At 1x, render directly to the destination without a resolve. Depth can be writable or read-only, using a depth-only texture format. Stencil attachment operations are not exposed. Attachment format support, samples, extent and the aggregate MRT bytes-per-sample budget are checked before encoding. The byte budget uses render-target costs and component alignment in attachment-location order, including layouts with empty slots. Clear and Load are distinct: Load consumes `before`, while the attachment view identifies the new written version.

`compile` retains authored order and rejects duplicate `RasterId` values. Both shadow-before-prepass and prepass-before-shadow are valid when their resource declarations have valid preceding producers. `CompiledRaster::encode` checks all referenced generations before opening any pass. Compilation does not publish logical versions into the registry across frames. Use one registry per submission plan when external work changes initialized contents. Duplicate imports must retain the same initial version; a persistent registry cannot publish a new external version through reimport. Callers must declare initialized imports at their own established submission boundary and retain GPU objects until their submitted work completes.

## Checked input with two color locations

This declaration assumes a single-sample initialized input tagged `Unspecified` and two distinct writable output views at their next versions. A draw pipeline must use the checked group's layout, match both target formats and write locations 0 and 1. The clears make the locations distinguishable even without a draw.

```rust
use worldrunner_render_post::{AttachmentLoad, StageId, contracts::*};

fn sampled_mrt(
    device: &wgpu::Device,
    registry: &ResourceRegistry,
    input: CheckedView,
    outputs: [CheckedView; 2],
) -> Result<(CompiledRaster, wgpu::BindGroupLayout), ContractError> {
    let checked = CheckedBindings::build(device, registry, &[TextureBinding {
        binding: 0,
        visibility: wgpu::ShaderStages::FRAGMENT,
        view: input.clone(),
        samples: 1,
        semantic: Semantic::Unspecified,
        kind: TextureBindingKind::Sampled(
            wgpu::TextureSampleType::Float { filterable: false },
        ),
    }])?;
    let layout = checked.layout().clone();
    let colors = outputs.into_iter().zip([wgpu::Color::RED, wgpu::Color::GREEN])
        .map(|(view, clear)| Some(ColorAttachment {
            view, before: None,
            load: AttachmentLoad::ClearColor(clear),
            store: wgpu::StoreOp::Store, resolve: None,
        })).collect();
    let compiled = registry.compile(device, &[RasterPass {
        id: RasterId::new(StageId::new("sampled MRT", 0), 0),
        colors, depth: None,
        uses: vec![ResourceUse::read(input, UseKind::Sampled)],
        bindings: vec![checked],
    }])?;
    Ok((compiled, layout))
}
```

The contract regressions exercise real sampled draws with installed checked groups, including simultaneous sampled/read-only depth and sampled/read-only storage access. The MRT execution test checks distinct red, green and blue outputs at 1x and 4x.

## Why linear order

The application already knows where its dependencies, diagnostic boundaries and draw ties belong. Resource declarations check that order without moving work. There is no topological scheduler, lifetime-based allocation aliasing, pass culling or merging.

[Filament FrameGraph](https://google.github.io/filament/notes/framegraph.html) uses declared reads and writes to compile a graph and manage resource lifetimes. These contracts use explicit resource access without implementing a graph compiler. [Bevy's per-view post-processing example](https://github.com/bevyengine/bevy/blob/v0.17.2/examples/shader_advanced/custom_post_processing.rs) separates graph ordering from view-owned targets and bindings; its resource queries do not provide the same exhaustive access validation.

The raster-only entry point retains its existing contract. [Typed operations](typed-operations.md) extend the same registry and validator to mixed raster, fullscreen, compute, upload, copy and readback lists, including checked buffer bindings and dynamic offsets. Sampler registration, pipeline registration and submission ownership remain separate work. Supplied cascade matrices are supported; generic caster collection remains deferred.
