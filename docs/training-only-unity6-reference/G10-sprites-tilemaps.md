# G10 — Sprites and tilemaps

**Training-derived, unverified, unofficial; no external source consultation. Training provenance and completeness cannot be certified.** No exact Unity 6.3 guarantee. All code/signatures are illustrative authored subsets from memory, not full declarations or compile-tested examples. See U001–U003 and U031–U033.

**Boundary:** engine sprite rendering and tilemap modules, plus optional 2D packages/tooling. `UnityEngine`, `UnityEngine.Tilemaps`, `UnityEngine.U2D`, and `UnityEngine.Rendering` contain relevant types. Editor palette tooling, 2D animation, SpriteShape, PSD importing and extras are not all one runtime module.

## G10.T01 — Sprite assets and creation

A `Sprite` is a view of a texture region plus pivot, pixels-per-unit, mesh/UVs, border and optional physics shape data. It is not simply a Texture2D with a different class name.

```csharp
// Selected Sprite shapes.
static Sprite Create(Texture2D texture, Rect rect, Vector2 pivot);
static Sprite Create(Texture2D texture, Rect rect, Vector2 pivot, float pixelsPerUnit);
static Sprite Create(Texture2D texture, Rect rect, Vector2 pivot, float pixelsPerUnit,
                     uint extrude, SpriteMeshType meshType, Vector4 border);
Texture2D texture { get; }
Rect rect { get; }
Vector2 pivot { get; }
float pixelsPerUnit { get; }
Vector4 border { get; }
Bounds bounds { get; }
Vector2[] vertices { get; }
ushort[] triangles { get; }
Vector2[] uv { get; }
void OverrideGeometry(Vector2[] vertices, ushort[] triangles);
```

The longest Create overload family may include a fallback-physics-shape option; exact overloads are U031. Create's pivot argument is normalized within the supplied rectangle, whereas a Sprite's pivot property is remembered in pixel units. Rect is a source texture pixel region. PixelsPerUnit converts pixels into local world units; it does not resize the source texture.

`SpriteMeshType.FullRect` and `Tight` select simple or fitted geometry. `textureRect`, `textureRectOffset`, packing state/mode/rotation, and associated alpha-split textures are relevant for packed sprites, but some geometry/packing combinations restrict direct textureRect access. UVs for an atlas sprite cannot be derived by assuming the original rect remains unpacked at the same coordinates.

`GetPhysicsShapeCount`, `GetPhysicsShape`, and `OverridePhysicsShape` families are remembered. A sprite's stored physics outline does not automatically create a Collider2D on every renderer. Runtime Sprite.Create does not reproduce all editor importer behavior, outlines, compression, or secondary-texture setup.

## G10.T02 — SpriteRenderer and sorting

`SpriteRenderer : Renderer` uses a Sprite and Material. Common properties include sprite, color, flipX, flipY, drawMode, size, tileMode, maskInteraction, and spriteSortPoint, alongside inherited sortingLayerID/sortingLayerName/sortingOrder and rendering state.

`SpriteDrawMode.Simple`, `Sliced`, `Tiled` are recalled. Sliced/tiled modes use sprite border and renderer size rather than treating size as a universal Transform scale. `SpriteTileMode.Continuous` and `Adaptive` are remembered. `SpriteSortPoint.Center`/`Pivot` influences sorting position. `SpriteMaskInteraction.None`, `VisibleInsideMask`, `VisibleOutsideMask` controls mask participation in supported pipelines.

flipX/flipY mirror the rendered sprite; they do not generally mirror the GameObject hierarchy, collider geometry, or Transform scale. Renderer color is a rendering tint/alpha input whose effect depends on the shader. Transparent sorting combines sorting layer/order, groups, camera sorting mode/axis and distance; Z alone is not a universal draw-order rule.

`SortingGroup` groups renderers for sorting. `SpriteMask` uses sprite-shaped masking with sorting range controls. Pipeline support, lit versus unlit 2D materials, normal maps and 2D renderer lighting are separate from the existence of SpriteRenderer. URP's 2D Renderer/Light2D workflow is a package feature, not the built-in sprite shader model.

## G10.T03 — Tilemap cell/world mapping

`GridLayout`/`Grid` define discrete cell coordinates and placement. `Tilemap` derives from GridLayout in remembered API hierarchy and commonly lives under a Grid. Coordinates use `Vector3Int`, including Z even for primarily 2D content.

```csharp
// Representative GridLayout/Tilemap conversion shapes.
Vector3 CellToWorld(Vector3Int cellPosition);
Vector3Int WorldToCell(Vector3 worldPosition);
Vector3 CellToLocal(Vector3Int cellPosition);
Vector3Int LocalToCell(Vector3 localPosition);
// Tilemap:
Vector3 GetCellCenterWorld(Vector3Int position);
Vector3 GetCellCenterLocal(Vector3Int position);
BoundsInt cellBounds { get; }
Bounds localBounds { get; }
```

CellToWorld identifies a grid cell reference location, not necessarily its visual center. Tilemap.GetCellCenterLocal/GetCellCenterWorld use the Tilemap's tileAnchor in recalled behavior; do not substitute a hard-coded half-cell offset or assume the result is always the geometric center after changing tileAnchor. Grid.GetCellCenterWorld uses the Grid layout's cell-center convention and is a distinct API. Transform scale/rotation and grid layout matter. Hand-written integer division of world XY does not reproduce isometric/hexagonal layouts or negative-coordinate behavior.

Grid properties include cellSize, cellGap, cellLayout and cellSwizzle. Remembered `GridLayout.CellLayout` values include `Rectangle`, `Hexagon`, `Isometric`, `IsometricZAsY`; swizzles include XYZ permutations. Tilemap adds tileAnchor, orientation and orientationMatrix. Exact layout mappings and orientation enum entries remain U032. A minimal rectangular implementation must declare unsupported layouts explicitly.

## G10.T04 — Tiles, colors, transforms, bulk edits

```csharp
// Selected Tilemap members.
TileBase GetTile(Vector3Int position);
T GetTile<T>(Vector3Int position) where T : TileBase;
void SetTile(Vector3Int position, TileBase tile);
void SetTiles(Vector3Int[] positionArray, TileBase[] tileArray);
TileBase[] GetTilesBlock(BoundsInt bounds);
void SetTilesBlock(BoundsInt position, TileBase[] tileArray);
void ClearAllTiles();
void RefreshTile(Vector3Int position);
void RefreshAllTiles();
void SetColor(Vector3Int position, Color color);
void SetTransformMatrix(Vector3Int position, Matrix4x4 transform);
void SetTileFlags(Vector3Int position, TileFlags flags);
void CompressBounds();
```

Setting a null tile clears a cell in familiar behavior. Batch operations reduce overhead but still require correct array lengths and cell ordering; exact flattened block order is U032 and is not invented here. `origin` and `size` describe the cell region; bounds may retain empty historical space until compressed. They are not automatically a sparse occupancy count.

`TileBase : ScriptableObject` is the customization base. Remembered override hooks include `GetTileData(Vector3Int, ITilemap, ref TileData)`, `RefreshTile(Vector3Int, ITilemap)`, `GetTileAnimationData(...)`, and `StartUp(...)`. `Tile` is a common concrete type holding sprite, color, transform, gameObject, flags and colliderType. TileData communicates sprite/color/transform/GameObject/flags/collider data to the map.

`TileFlags.None`, `LockColor`, `LockTransform`, `InstantiateGameObjectRuntimeOnly`, and composite `LockAll` are recalled; exact version availability is U031. A locked color/transform can prevent a per-cell setter from visibly overriding tile data until flags permit it. Tile.ColliderType values include `None`, `Sprite`, `Grid`. Animated tile data includes sprite sequence and speed/start-time-related fields; exact schema is not specified.

## G10.T05 — Tilemap rendering and collision

`TilemapRenderer` handles chunking, sorting and render mode; it is not one SpriteRenderer component per cell. Remembered modes include Chunk and Individual, with later batching-related options uncertain. Material and sorting choices influence whether individual tiles interleave with other renderers. `Tilemap.animationFrameRate` controls animated tile progression in familiar APIs.

`TilemapCollider2D` generates collision geometry from tile collider settings. CompositeCollider2D and a suitable Rigidbody2D can combine tile shapes. Edits can be batched and processed later rather than immediately regenerating geometry on every SetTile. `hasTilemapChanges` and `ProcessTilemapChanges()` are remembered explicit-update hooks; exact update phase/composite refresh integration is U033.

Grid colliders and sprite-outline colliders produce different geometry. Visual gaps, extrusion, tile transforms and collider offsets matter. Disabling a renderer does not necessarily disable the collider or remove cells. Tile-instantiated GameObjects have their own lifecycle and ownership; they should not be treated as reusable shared tile assets.

## G10.T06 — Atlases, packages, runtime subset

`UnityEngine.U2D.SpriteAtlas` provides sprite lookup families such as `GetSprite(string)` and `GetSprites(Sprite[])`; returned sprites and clone/name behavior require review before relying on identity. SpriteAtlasManager has atlas-request/registration-related callbacks. Editor atlas packing, variants, platform texture settings, and build inclusion are tooling contracts rather than runtime packing guarantees.

2D Tilemap Editor, 2D Sprite tooling, 2D Animation, SpriteShape and Tilemap Extras/RuleTile-style functionality have package/version boundaries. RuleTile is not a fundamental member of the engine Tile type. Imported atlas formats and packed texture UVs require asset support beyond reproducing the public class names.

Proposed tests (not run): pivot units and pixels-per-unit sizing; renderer flip leaves collider unchanged; sorting group behavior; rectangular cell-center round trips including negative cells; null tile removal; lock flags versus color/transform changes; block layout order; bounds compression; and collider refresh after tile edits. Hex/isometric geometry, atlas exactness, editor brushes, advanced 2D lighting and tile callback ordering are intentionally incomplete.
