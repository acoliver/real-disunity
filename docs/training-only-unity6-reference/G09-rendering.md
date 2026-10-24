# G09 — Cameras, graphics, meshes, materials, textures, pipelines

**Training-derived, unverified, unofficial; no external source consultation. Training provenance and completeness cannot be certified.** No exact Unity 6.3 guarantee. Code/signatures are authored illustrative subsets from memory, not full declarations or compile-tested examples. See U001–U003, U028–U030.

**Boundary:** engine camera/rendering/mesh/texture modules and `UnityEngine.Rendering`; SRP Core, Universal Render Pipeline (URP), and High Definition Render Pipeline (HDRP) are separately versioned packages. A common engine API does not imply identical rendering across pipelines, platforms, color spaces, or graphics backends.

## G09.T01 — Cameras and coordinate conversion

`Camera : Behaviour` holds projection/view/culling/render-target state. Common members: `fieldOfView`, `orthographic`, `orthographicSize`, `nearClipPlane`, `farClipPlane`, `aspect`, `rect`, `pixelRect`, `pixelWidth`, `pixelHeight`, `cullingMask`, `clearFlags`, `backgroundColor`, `depth`, `targetTexture`, `worldToCameraMatrix`, `projectionMatrix`, `cullingMatrix`, and static `main`/`allCameras` families.

```csharp
Vector3 WorldToScreenPoint(Vector3 position);
Vector3 ScreenToWorldPoint(Vector3 position);
Vector3 WorldToViewportPoint(Vector3 position);
Vector3 ViewportToWorldPoint(Vector3 position);
Ray ScreenPointToRay(Vector3 position);
Ray ViewportPointToRay(Vector3 position);
void ResetProjectionMatrix();
```

Stereo-eye overloads exist but are not inventoried. Screen XY are pixels; viewport XY are normalized relative to the camera. The Z component of screen/world conversion is depth along the camera's viewing convention, not a depth-buffer sample and not arbitrary distance from world origin. ScreenToWorldPoint requires a meaningful depth. ScreenPointToRay is preferable for choosing a world surface via raycast. In recalled behavior, ScreenPointToRay/ViewportPointToRay ignore the input Z and return a world-space ray beginning on the camera's near plane, not necessarily at the camera Transform; orthographic rays have parallel directions with origins varying across that plane. OrthographicSize represents half the vertical visible span. FOV normally describes vertical perspective FOV unless a physical-camera configuration changes interpretation.

`Camera.main` finds a suitably tagged enabled camera in recalled behavior; it is not ownership injection and its caching/performance details should not become a contract. `CameraClearFlags.Skybox`, `Color`/historical `SolidColor`, `Depth`, `Nothing` are remembered identifiers; aliases and SRP support need U028 checking. CullingMask uses layer bits. Camera depth/order, clear behavior, stacking, XR, and render callbacks differ by pipeline.

Built-in messages include `OnPreCull`, `OnPreRender`, `OnPostRender`, and image-effect `OnRenderImage(RenderTexture source, RenderTexture destination)`. Do not promise these execute under URP/HDRP as if they were universal extension hooks.

## G09.T02 — Mesh data and drawing

`Mesh` is a native-backed resource with vertices, normals, tangents, colors, UV channels, indices/submeshes, skinning data, and local bounds. `MeshFilter.sharedMesh`/`mesh` and `MeshRenderer` connect mesh data to rendering; `SkinnedMeshRenderer` adds bones, bind poses, weights, blend shapes and deformation.

```csharp
// Selected Mesh shapes.
Vector3[] vertices { get; set; }
Vector3[] normals { get; set; }
Vector4[] tangents { get; set; }
Vector2[] uv { get; set; }
int[] triangles { get; set; }
Bounds bounds { get; set; }
int subMeshCount { get; set; }
void SetVertices(System.Collections.Generic.List<Vector3> vertices);
void SetTriangles(int[] triangles, int submesh);
void SetIndices(int[] indices, MeshTopology topology, int submesh);
void RecalculateBounds();
void RecalculateNormals();
void RecalculateTangents();
void MarkDynamic();
void UploadMeshData(bool markNoLongerReadable);
```

Array properties often copy data and should not be treated as direct mutable views. Editing a retrieved array alone does not necessarily modify the native mesh. `MeshTopology.Triangles`, `Quads`, `Lines`, `LineStrip`, `Points` are common. `UnityEngine.Rendering.IndexFormat.UInt16`/`UInt32` controls index width; large meshes need suitable index format/backend support. Submesh indices correspond to material slots, not separate Transform nodes.

Vertex attribute APIs (`VertexAttributeDescriptor`, `VertexAttribute`, `VertexAttributeFormat`), vertex/index buffer setters, `MeshUpdateFlags`, and `Mesh.AcquireReadOnlyMeshData`/`AllocateWritableMeshData`/`ApplyAndDisposeWritableMeshData` allow advanced data paths. Exact buffer descriptors, generic constraints, stream packing and overloads are U029. CPU readability is a resource state; uploading with nonreadable semantics can make later CPU reads fail. Recalculation can change normals at seams and does not reconstruct authored shading perfectly.

Renderer state includes enabled, bounds, sharedMaterial/sharedMaterials, material/materials, shadowCastingMode, receiveShadows, sortingLayerID/sortingOrder, light probe and reflection probe settings. Instanced `mesh`/`material` access can create copies, whereas shared properties refer to shared resources. Mutation and destruction ownership must be explicit.

## G09.T03 — Materials, shaders, property blocks

```csharp
// Selected Material/Shader shapes.
Material(Shader shader);
Material(Material source);
void SetFloat(string name, float value);
void SetInt(string name, int value);
void SetVector(string name, Vector4 value);
void SetColor(string name, Color value);
void SetTexture(string name, Texture value);
bool HasProperty(string name);
void EnableKeyword(string keyword);
void DisableKeyword(string keyword);
// Static Shader:
static Shader Find(string name);
static int PropertyToID(string name);
```

Integer property-ID overloads, getters, matrices/arrays/buffers, texture offsets/scales, pass selection, and local keyword APIs form additional families. Shader property IDs are session-local efficient handles, not stable serialized IDs. Shader.Find can fail in a player if variants/assets were stripped. A shader name is not a portable shading-language implementation.

`MaterialPropertyBlock` overrides renderer properties without allocating a unique Material per object. Use `Renderer.SetPropertyBlock(block)` and `GetPropertyBlock(block)`; submesh/material-index overloads exist. Setting a block should be understood as supplying/copying a property set, not establishing a live reference to every future mutation of that block. Property blocks, batching, GPU instancing and SRP Batcher support interact in pipeline-specific ways.

`renderQueue`, keywords, shader tags, blending, culling, depth state, texture sampling and pipeline shader passes jointly determine appearance. `_MainTex` and `_Color` are common historical names, not universal. URP/HDRP commonly use different property names and lighting conventions. Color values and textures have linear/sRGB interpretation; blindly copying numeric RGBA can change appearance. `ShadowCastingMode.Off`, `On`, `TwoSided`, `ShadowsOnly` are remembered enum members.

## G09.T04 — Textures and render targets

Types include `Texture`, `Texture2D`, `Texture3D`, `Cubemap`, `Texture2DArray`, `CubemapArray`, `RenderTexture`, `CustomRenderTexture`. Common texture state: width/height/dimension, filterMode (`Point`, `Bilinear`, `Trilinear`), wrapMode (`Repeat`, `Clamp`, `Mirror`, `MirrorOnce` subject to support), anisotropic level, format, mipmaps and readability.

```csharp
// Selected Texture2D operations and constructor family.
Texture2D(int width, int height, TextureFormat textureFormat, bool mipChain);
void SetPixel(int x, int y, Color color);
void SetPixels(Color[] colors);
Color GetPixel(int x, int y);
Color[] GetPixels();
void Apply(bool updateMipmaps, bool makeNoLongerReadable);
void ReadPixels(Rect source, int destX, int destY, bool recalculateMipMaps);
```

Linear-color constructor overloads and `GraphicsFormat` forms exist. `GetRawTextureData<T>`, `SetPixelData<T>`, compression and image-conversion APIs are more specialized. `ImageConversion.LoadImage`, `EncodeToPNG`, and JPG/EXR-family encoding are module APIs often exposed as extension-style usage; exact supported formats/readability requirements are U029. TextureFormat remembered examples include RGBA32, RGB24, Alpha8, RFloat, RGBAFloat and platform compressed formats. Do not assume a requested format is natively supported.

RenderTexture construction uses width/height/depth/format or a `RenderTextureDescriptor`. `Create`, `Release`, `IsCreated`, static `active`, `GetTemporary`, and `ReleaseTemporary` manage GPU resources. Returning a temporary requires the matching pool operation, not Destroy as a substitute. Releasing GPU storage is distinct from destroying the managed engine object. Reading pixels can synchronize CPU/GPU and requires the intended active render target. Preserve previous render state when performing isolated readback.

Depth/stencil bits, MSAA sample count, mip generation, random-write support, memoryless attachments, texture dimension, array slices and sRGB settings all affect allocation. Async GPU readback has a separate lifetime/error model and is not equivalent to immediate GetPixels.

## G09.T05 — Graphics, render state, command buffers

`Graphics.Blit(source, destination)` copies through a rendering path, optionally with a Material/pass; `Graphics.CopyTexture` performs compatible copy operations, not arbitrary format conversion. Source and destination identity, sampling, MSAA and backend restrictions matter. Historical DrawMesh/DrawMeshInstanced families coexist with newer `Graphics.RenderMesh`-style APIs; current overloads/obsolescence are U028. Do not invent all combinations.

`UnityEngine.Rendering.CommandBuffer` records commands for later execution, including render-target changes, clears, mesh draws, texture copies, and temporary resources. Recording a draw is not immediate GPU completion. Recorded resources must remain valid for the execution window. Camera command-buffer attachment through built-in CameraEvent points differs from SRP command execution through `ScriptableRenderContext`.

`GraphicsBuffer`, historical `ComputeBuffer`, `ComputeShader`, dispatch group sizes, structured layouts and resource binding form compute facilities. Buffer strides, target flags, readback and disposal are explicit. `ComputeShader.Dispatch` dimensions are group counts, not necessarily element counts. Most creation/binding work is engine-thread-affine even when GPU execution is asynchronous.

Common rendering enums/types include `CompareFunction`, `BlendMode`, `BlendOp`, `CullMode`, `RenderQueue`, `RenderTargetIdentifier`, `RenderTextureFormat`, `GraphicsFormat`, `CameraEvent`, and `LightEvent`, with different namespace locations. Lighting types include `Light`, `LightType.Directional`/`Point`/`Spot`/`Area` and baked/mixed/realtime concepts. LineRenderer, TrailRenderer, ParticleSystem, terrain rendering, reflection probes, light probes, occlusion culling, LODGroup and XR are important but only acknowledged here, not substantially specified.

## G09.T06 — Built-in, SRP, URP, HDRP boundaries

The Built-in Render Pipeline is the legacy integrated engine pipeline. SRP provides engine interfaces (`RenderPipelineAsset`, `RenderPipeline`, `ScriptableRenderContext`, culling/drawing/filtering settings) for implementing pipelines. `GraphicsSettings` and `QualitySettings` select/configure pipeline assets in version-sensitive ways.

URP (`com.unity.render-pipelines.universal`) and HDRP (`com.unity.render-pipelines.high-definition`) are package pipelines, with SRP Core (`com.unity.render-pipelines.core`) as shared infrastructure. Their camera/light data, renderer features/custom passes, volume components, shader libraries and asset schemas are not interchangeable. URP renderer-feature concepts recall `ScriptableRendererFeature` and `ScriptableRenderPass`; HDRP exposes custom-pass and volume workflows. Unity 6 render-graph adoption and compatibility modes make exact extension signatures especially uncertain (U030).

SRP exposes begin/end rendering callbacks through `RenderPipelineManager`; these are not a blanket replacement with identical timing for every built-in callback. Do not claim that OnRenderImage, Camera.Render, CameraEvent command buffers, deferred settings, skybox handling or camera stacking work identically everywhere.

For a subset runtime, choose a declared pipeline and minimal shader/material contract. Proposed tests (not run): world-to-screen round trips at known depth; orthographic size; submesh/material mapping; shared-versus-instanced material mutation; property-block isolation; mesh readability after upload; texture Apply visibility; render-target restoration after readback; and explicit resource disposal. Image equivalence, complete shader compilation, imported texture/mesh formats, GPU-native ABI, and exact pipeline asset compatibility remain unknown.
