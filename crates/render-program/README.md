# Running Unity player shader programs

`worldrunner-render-program` draws and dispatches the shader programs that Unity ships in a player build, with wgpu. It sits in the Graphics layer. This slice proves that one pass, a vertex and fragment program or a compute kernel, can be selected, laid out, fed its constants, textures and state, and executed on a real adapter with predicted results.

## Inputs

The crate takes data that the importer has already prepared. It does not read bundles or translate shaders.

| Input | Type | Source |
| --- | --- | --- |
| Prepared SPIR-V words and entry point | `module::StageModule` | The shader preparation in `unity-import` |
| Binding table | `module::ModuleBinding` | Mirrors `unity-import`'s `ResourceBinding`: set, Unity binding, prepared binding and, for a split combined sampler, the sampler's binding |
| Parameters | `unity_serialized::shader_program::ProgramParameters` | The serialized sub program |
| Keyword variants | `unity_serialized::shader_program::Program` | `variant::select_sub_program` picks the sub program whose keyword set equals the enabled keywords, and `select_player_sub_program` does the same across platform lists |
| Fixed-function state | `unity_serialized::shader_state::ShaderState` | `state::resolve_state` reads each value from its fixed number or from the first property layer that names it |
| Properties | `properties::PropertySet` | Layers searched in order; the first layer with a name wins |

## What it does

- **Layouts.** `layout::ProgramLayout::build` merges the stages' resources into one pipeline layout and checks them against Unity's parameters. An unsupported parameter, a resource without a parameter, a parameter of another kind than its resource and stages that declare one binding differently are named errors. A parameter the entry point does not use is reported, not bound.
- **Constants.** `constants::pack_constant_buffer` writes vectors, matrices and arrays at Unity's byte offsets, a register of 16 bytes apart, sizes the buffer for the shader's needs and reports missing properties. `StageProgram::constant_buffer_targets` and `texture_targets` give the prepared set and binding each parameter fills.
- **Variants.** An exact keyword match selects the sub program. A missing combination is a named error.
- **State.** Cull, `ZTest`, `ZWrite`, color mask and blend for render target 0 resolve from fixed or property-controlled values. Unknown values and state this slice does not draw (stencil, alpha to mask, depth bias, conservative raster, clip off, separate blend) are named errors.
- **Draws.** `ProgramRuntime::draw` renders non-indexed or indexed triangles with a vertex layout the caller gives, into a color target with an optional depth target. `read_target` returns the pixels. Render and compute pipelines and shader modules are cached by content.
- **Compute.** `ProgramRuntime::dispatch` runs a kernel with Unity-style bindings; `read_buffer` returns a storage buffer.

## Measured conventions

The tests in `tests/program/conventions.rs` draw into an 8x8 target and compare bytes. They pin what a Unity Vulkan module needs:

- The module's position output is used unflipped. Positive clip Y lands in the bottom rows of the image.
- The depth range that survives clipping is 0 to w, as in Vulkan; the nearer of two triangles wins under `LessEqual`, and z of -0.5 or 1.5 is clipped.
- `wgpu::FrontFace::Cw` marks a triangle that is clockwise in the image, with row 0 at the top, as a front face. Cull Back drops the others and Cull Front drops the front faces. The draw request carries the winding, so a caller states it.

`tests/program/pass.rs` and `drawn_state.rs` draw a textured pass with a tint from a constant buffer, the multiply and add keyword variants, a split combined sampler, blend off against `SrcAlpha`/`OneMinusSrcAlpha`, and cull Back against Off for a back-facing quad. Pixels are predicted in closed form from the texels, tint and offset, independent of the shader, and compared with the 8-bit bound from `worldrunner_render_gpu::tolerance_test_support` (0.625 steps). The alpha-blend test adds 0.5 step because lavapipe's fixed-point blend measured 0.8 steps from the prediction; the Radeon stayed within the shared bound.

## Not done

- **Unity sampler state.** Every sampler binding gets one fixed sampler: linear filtering, clamped to the edge. The packed sampler state in the serialized parameters is not decoded, so wrap, filter and anisotropy settings are ignored.
- **Multisampling, stencil and multiple render targets.** Targets are single-sample with one color attachment and an optional depth attachment. Stencil state is a named error.
- **Player-variant parameter blobs.** `Program::parameter_blob_indices` are not decoded, so parameters come from the sub programs' own `ProgramParameters`.
- **Executor and frame integration.** The runtime submits to the queue directly. It does not go through the frame submission authority of `worldrunner-render-gpu` and no scene or material system calls it.
- **Calling shader preparation.** `shader_prep` lives in `unity-import`, which is in the Tool layer, so a Graphics-layer runtime cannot call it. Callers supply the prepared words and binding table.

## Tests

```sh
cargo test -p worldrunner-render-program -- --test-threads=1
```

The draw and dispatch tests need a Vulkan adapter. Set `VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.json` to run them on lavapipe.
