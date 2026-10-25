# Atmospheric rendering

Atmospheric backgrounds are optional scene content. Install a `SceneAtmosphere`
with `SceneEffects::with_atmosphere` to draw a linear HDR gradient, an optional
solar disc, textured celestial discs and camera-relative celestial meshes.
Existing effects without this option retain their original clear background.
The stock upload rejects atmospheric rendering on a non-HDR color target.

## Scene space and units

All directions remain in the scene's Z-up coordinate frame. `SceneDirection`
validates quantized unit vectors. Radiance fields are linear RGB millionths,
so `2_000_000` means 2.0 rather than an encoded display channel. Radiance can
exceed one. Angular radii and rotations are millidegrees. Distances and base
heights are millimetres; fog density and height falloff are millionths per
metre. There is no calendar, orbital model or asset resolver in the renderer.

The background interpolates from horizon to zenith according to the clamped
world-ray elevation raised to an authored exponent. Below-horizon rays retain
the horizon color. The solar disc takes its direction and radiance from the
same shared frame fields used for directional lighting and shadow placement.
It has no independent light-direction setting. The visible solar disc uses a
stylized convention: its radiance is numerically equal to the shared solar
irradiance. It is not normalized as a physical emitter. A uniform disc with
angular radius r and radiance L would supply normal irradiance
`pi * L * sin(r)^2`.

The `SceneSky::ProceduralHemisphere` descriptor continues to define the
CPU-derived ambient hemisphere. The optional atmospheric gradient fields define
visible background radiance independently. They do not change ambient intensity.

With atmosphere enabled, `SceneSky::DomeTexture` selects an equirectangular dome
instead of the gradient. Sampling uses scene Z-up rays: +X maps to u=0.5, +Y to
u=0.75 and zenith to v=0. U repeats across the longitude seam and V clamps at
the poles. Explicit periodic longitude gradients retain mip filtering without
a derivative discontinuity at the seam. Texture upload owns sRGB decoding; the shader samples linear RGB,
ignores dome alpha and applies fog and shared exposure afterward. Camera
translation does not move the dome. Solar and celestial draws retain their
existing layering. Set both hemispheric ambient irradiances to zero for a dome.
Nonzero dome-derived ambient is rejected because no CPU hemisphere radiance has
been prepared from the texture.

## Composition and depth

Atmospheric draws run inside the existing opaque color pass, after opaque
geometry and before dynamic geometry. Sky and discs do not add executor passes.
Celestial meshes add one `PassSlot::Celestial` visibility pass with its own HDR
color and depth attachments, before the opaque pass. No StageId changes.
Final background fragments have far depth and use `LessEqual` without scene-depth
writes. Geometry already in the depth buffer occludes the background, discs
and celestial meshes. The same policy applies at one or multiple samples.
Sky content does not enter geometry metadata, shadow-caster selection or
screen-space reflection traversal.

A fullscreen triangle reconstructs world rays by intersecting the shared
camera transform's clip-coordinate planes. The cross product is affine in
screen coordinates, preserving perspective interpolation. Orthographic views
produce parallel rays across their complete extent. Camera translation does
not affect these directions. There is no separate camera state or per-frame
sky upload.

Discs use angular ray tests, so their shape does not depend on viewport aspect
ratio. Rotation is counterclockwise in the disc plane. A disc texture's RGB
multiplies emitted radiance and alpha controls coverage. The normal scene
texture upload owns its color-space interpretation. An absent texture uses the
existing white texture. Discs composite in authored far-to-near order.

Celestial meshes reuse resident scene mesh buffers, material groups and the
existing surface-lighting shader. Their source bounding radius and angular
radius determine scale through `radius = distance * sin(angular_radius)`.
Their quantized quaternion defines orientation. The model translation is
relative to the eye, with world translation added in the vertex shader.
Meshes reuse the ordinary material vertex transform, including billboard,
vertex AO and wind behavior. Translation is added before that transform so
billboards use world-space eye and pivot coordinates. Meshes are depth-tested
against each other in the visibility attachment. Per-fragment depth preserves
surface ordering regardless of triangle submission order. Original camera
near-plane and front-half-space tests still apply; the far plane does not limit
background geometry. The visibility image composites after the disc list,
reading the corresponding sample for multisampled targets. Meshes do not cast
shadows. Use `compositor::encode_scene`, `encode_resident` or `ShadowedScene`
to schedule this pass. The low-level single-pass `SceneGpu::draw` rejects scenes
containing celestial meshes. Source mesh bounds must agree with the supplied bounding
radius for that angular-size contract to hold.

The scene frame owns exposure. The sky shader applies its multiplier once,
just as surface shading does. HDR composition supplies an unexposed frame when
the post chain owns exposure. Background radiance, fog radiance and celestial
emission therefore enter metering in the same linear units as opaque geometry.

## Height fog

Fog density decreases exponentially with height above an authored base. The
Beer-Lambert integral starts at the configured distance along each ray rather
than multiplying the complete integral by a shortened length. Horizontal rays
use a series expansion around zero vertical displacement. The implementation
uses a logarithmic optical depth to retain downward contributions when density
at a high-altitude eye would underflow.

`render-lighting` supplies both the CPU integral and the shared WGSL integral.
Mesh and terrain fragments fog their complete outgoing radiance, including
mesh emission, before exposure. The background uses its authored finite ray
length and the same WGSL integral. Every consumer reads the current frame fog
lanes, including enable state, rather than an upload-time copy. Fog does not
change depth. Orthographic rays start at the pixel's position on the camera
plane and run parallel to the viewing direction. Density is adjusted from the
eye height to that per-pixel origin. Perspective rays start at the eye.

Six formerly unused frame-uniform lanes encode logarithmic eye density,
falloff/enable state, start distance and RGB fog radiance. The frame block
remains 160 bytes. Disabled fog writes the same zero lanes as before. The
negative falloff encoding distinguishes enabled fog from the old zero padding
without reserving a logarithmic density value.

### Water composition interface

The water pass owns its transmission/scattering composition. It can consume
`FrameUniform::height_fog` and the shared WGSL function
`rd_height_fog_transmittance(log_density_at_eye, falloff, start_distance, offset, up)`.
`offset` is ray-origin-to-surface in metres and `up` is the unit scene Z axis.
For perspective views the origin is the eye. For orthographic views, use the
pixel's camera-plane origin and parallel segment, as the surface shader does.
The first parameter is logarithmic density at that origin:
`ln(density) - falloff * (origin.z - base_height)`.
The CPU references are `HeightFog::transmittance` and `HeightFog::apply`.

The existing frame lanes carry log density in camera.w, `-(falloff+1)` in
sun_direction.w, start distance in sun_irradiance.w, and fog RGB in
ambient_sky.w, ambient_ground.w and grading.w. Nonnegative sun_direction.w
disables fog. A zero logarithm is a valid enabled density, not a disabled flag.

After composing water's unexposed outgoing surface radiance, the air segment
uses `L_eye = T_air * L_surface + (1-T_air) * L_fog` before the owning exposure
stage. Opaque color snapshots already contain eye-path fog. They must not be
mistaken for unfogged endpoint radiance or fogged a second time. Water owns its
segment and snapshot policy and the corresponding composition test; this
atmosphere change leaves water rendering untouched.

## Environment preparation

`sky::preparation::encode` records GGX environment filtering and a split-sum
BRDF lookup integration into the current authority-owned encoder. Call it
inside `SubmissionAuthority::prepare`. It does not submit a queue or open a
window. The source must be a square, filterable linear `Rgba16Float` cube view
with six layers. The caller owns its storage and upload accounting.

`EnvironmentPreparation::allocations` declares the output mip chain, LUT and
per-dispatch uniform storage. The preparation call validates that plan against
device limits and the supplied `AllocationBudget` before creating resources.
Face sizes are powers of two through 1024, LUT dimensions through 1024, and
sample counts through 4096. Output images are `Rgba16Float`, without an sRGB
transfer function. The two LUT channels are A and B for `F0 * A + B`.

The convolution uses deterministic low-discrepancy GGX samples and normalized
cosine weighting. Each source mip is selected from the sample's solid-angle
footprint: `0.5 * log2((1 / (samples * pdf)) / texel_solid_angle)`, clamped to
the bound source view's available mips. For the normal-incidence prefilter,
`pdf = D_GGX / 4` and `texel_solid_angle = 4*pi / (6 * source_size^2)`.
Mirror roughness samples mip zero exactly. Source mip chains are caller-owned
and must already contain valid radiance; this job does not populate them.
Higher output mips increase roughness; they are not a box-filter chain. A
constant-radiance source stays constant, including radiance above one. The LUT
uses correlated Smith visibility and Schlick Fresnel, matching the reference
math in `render-lighting::ibl`.

Generated map views are unavailable until the transaction submits. Cancellation
or dropping an unsubmitted frame leaves them unavailable. After submission,
queue ordering permits immediate use by later work without a CPU wait.
`sky::allocations(settings, size, samples)` separately declares resident
atmospheric uniform and instance storage, plus the HDR color and depth visibility
attachments when meshes are present, for inclusion in a host's allocation plan.
Visibility storage matches the viewport and sample count and is reused until
that size changes. Referenced scene
textures, mesh buffers and material resources remain counted by their owner.

The preparation API returns resources rather than installing an automatic IBL
material policy. Consumers choose sampling and material integration explicitly.

## Verification

CPU tests cover horizon/zenith gradients, HDR values, fog continuity and start
distance, angular disc mapping, mesh sizing/orientation and reference BRDF and
cubemap integrals. GPU tests compare fog against CPU values at horizontal rays
and altitude extremes, read exact HDR/depth values, check disc coverage and
shared solar direction, compare generated LUT values against CPU integration,
and preserve constant cube energy. A mip-distinct source verifies coarse
source selection at rough output mips and mip-zero selection for mirror output.
A synthetic full-scene HDR capture includes two opaque objects, a celestial mesh,
sky and fog. Numeric pixels check depth ordering, object occlusion of the mesh,
analytic fog over distance and absolute radiance with one exposure. Dome tests
exercise the normal scene texture upload in linear color space. Preparation
tests also cancel a transaction and verify that generated views remain
inaccessible. Existing disabled-feature shader snapshot comparisons continue
to check exact output bytes.
