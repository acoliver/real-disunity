# CPU resolved camera

`CameraState` and `resolve(&CameraState) -> Result<ResolvedView, ViewError>` provide the shared CPU camera boundary. The crate has no GPU or window dependency. Controllers retain their input, movement, zoom and profile policies.

## Inputs and coordinate ownership

- `CameraPose` supplies an eye in metres and explicit forward/up directions. Resolution normalizes the directions and orthogonalizes the view basis. A planar view needs an up direction independent of forward.
- `CameraLens::Perspective` takes vertical field of view in radians and finite `0 < near < far` distances in metres.
- `CameraLens::Orthographic` takes a finite positive vertical half-height and finite `0 <= near < far`. Horizontal half-width follows viewport aspect.
- `Viewport` is the existing camera-profile viewport type. Zero dimensions are errors. Resize updates the state and resolves a new immutable view.
- `ViewConvention` retains its existing handedness field and serialization contract.

The resolver does not reinterpret world axes or convert source units. All geometry, camera poses and query points must use the same coordinate frame. Governed scene documents remain Z-up millimetres. `SceneView` converts their integral camera positions to metres. Existing FreeFly/RTS methods retain their Y-up controller coordinates. A future explorer adapter must make its world-frame conversion explicit. The compatibility player converts source Y-up metres at its extraction boundary.

## Outputs and reconstruction coupling

Matrices multiply glam column vectors. Depth is non-reversed zero-to-one: near maps to zero and far maps to one.

| Accessor | Contract |
| --- | --- |
| `view_matrix` | World to the selected raster view space. Right-handed forward is -Z; DirectX-left-handed forward is +Z. |
| `projection_matrix` | Raster view to homogeneous clip space. |
| `view_projection_matrix` | World to clip, also used by all screen queries and point-frustum tests. |
| `inverse_view_projection_matrix` | Inverse of the actual single-precision raster VP. |
| `reconstruction_view_matrix` | Right-handed world-to-view affine matrix for either raster convention. |
| `reconstruction_projection_matrix` | Projection paired with that right-handed reconstruction view. The left-handed raster convention mirrors clip X. |
| `eye_meters`, `viewport`, `convention`, `depth_convention` | The camera's world-space eye and resolved presentation metadata. |

Use both reconstruction matrices from the same `ResolvedView`. Combining a left-handed raster projection with its right-handed reconstruction view gives incorrect depth. The pre-existing `ViewHandedness::reconstruction_projection` helper remains available for callers of the old API.

Camera construction uses f64 inputs and intermediate view/projection arithmetic, followed by f32 GPU matrices. VP is narrowed after multiplication; multiplying its separately narrowed view/projection matrices can differ by floating-point rounding. The inverse is computed from the exposed raster VP, so queries invert the matrix actually submitted for rasterization. Resolution rejects nonfinite matrices, singular inverses and clip volumes whose near/far corners become infinite at GPU precision.

## Screen queries

`world_to_screen` returns continuous top-left pixel coordinates and raster depth. Coordinates describe pixel edges; the centre of pixel `(x, y)` is `(x + 0.5, y + 0.5)`. Projection permits finite points outside the viewport/frustum and reports homogeneous or pixel-coordinate overflow. It does not establish visibility or occlusion.

`screen_to_world` accepts finite continuous coordinates, including positions outside the viewport, with depth in `0..=1`. `contains_point` tests the six homogeneous clip planes. `screen_ray` unprojects the near/far planes and returns a unit forward ray originating on the near plane for either lens. Ray normalization and plane intersection use f64 intermediates so representable f32 clip distances and finite nonzero plane-normal scales do not overflow or underflow during normalization. `ScreenRay::intersect_plane` rejects parallel rays, nonfinite geometry and intersections behind the ray origin.

## Compatible adapters

`FreeFlyCamera::camera_view` and `RtsCamera::camera_view` retain their signatures, existing aspect/clip/FOV normalization and `CameraView` output. Their matrix construction now invokes the common resolver. Inputs that cannot describe a finite invertible camera produce a NaN VP in this legacy non-Result interface; `resolved_view(viewport, near, far)` exposes diagnostics. Controller movement and tuning are unchanged.

`SceneView::new`, `orthographic`, `orthographic_with_up`, and all existing accessors retain their signatures. SceneView stores one `ResolvedView`; `resolved_view()` borrows it, and `from_resolved` adapts a previously resolved camera without recalculation. Its existing direction/roll/viewport/orthographic error variants remain; `ViewError::Resolution` carries additional shared resolution diagnostics. Adding an enum variant requires downstream exhaustive matches to include it.

The unchanged Orbit controller delegates to `SceneView::new`, so its views use this same resolver transitively.

## Integration and tests

`SceneView` and camera controllers share matrix resolution, inverse, reconstruction, viewport and screen-query contracts. `SceneView` converts its f32 unit/angle inputs before resolution.

Headless [landmark tests](../worldrunner/tests/resolved_camera.rs) compare rendered pixels with CPU screen queries for perspective and orthographic lenses, both handedness conventions, and resized viewports.

The facade exposes this contract through `worldrunner::scene::camera` with `default-features = false, features = ["scene"]`. The existing `gfx::worldrunner_camera` path remains available with `gfx`. The [facade consumer tests](../worldrunner/tests/resolved_camera.rs) execute shared planar queries, FreeFly/RTS reconstruction, and governed SceneView construction with scene-only features. Enabling `scene-viewer` additionally exercises Orbit drag, zoom, resize, inverse queries and ray intersection through the same facade.

The resolver owns camera mathematics. Window lifecycle and capture scheduling belong to the host and renderer. See the [compatibility architecture](../../docs/unity-compatibility-kit-architecture.md) for the player integration.
