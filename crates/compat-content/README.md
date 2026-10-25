# Native Unity text content admission

`worldrunner-compat-content` reads an explicitly selected original scene and its reachable serialized assets. It emits a serde graph for the host to admit into the native world. The crate has no runtime, managed, renderer, or game-identity dependency. Files in the selected project are read only.

## Import one scene

```rust
use std::path::Path;
use worldrunner_compat_content::{import_project, ImportProfile, ImportSidecar};
use worldrunner_original_games::{checkout, Game};

let root = checkout(Game::Pong)?;
let graph = import_project(
    &root,
    Path::new("Assets/Scenes/Main.unity"),
    &ImportProfile::default(),
    &ImportSidecar::default(),
)?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

The example executable writes JSON to stdout. Resolve the pinned original through the shared fetcher, then run from the repository root:

```sh
PONG=$(cargo run -p worldrunner-original-games --bin original-games -- path pong)
cargo run -p worldrunner-compat-content --example import -- \
  "$PONG" Assets/Scenes/Main.unity
```

Optional third and fourth arguments are profile and sidecar JSON paths. `profiles/pong-source.json` declares the older correspondence names and explicit host-owned builtin/font/Text requirements. Hosts must supply the implementations named by those declarations. The profile creates no replacement pixels or managed classes.

## Graph and world admission

`SourceRef { guid, file_id, instance }` identifies every original or expanded document. `SourceRef::key()` encodes a stable graph ID. IDs remain stable across field edits. Expanded prefab copies use the containing instance's source key as their scope. `aliases` maps stripped scene source records to their expanded records.

`nodes` contains GameObjects, active/tag/layer data, local TRS, GameObject parent/children IDs, component IDs, and a `template` flag. Scene objects have `template=false`. Reachable prefab assets also supply `template=true` nodes for later instantiation. A native host must keep templates outside the active initial world. `components` contains source kind/class ID, owner, enabled state, all original fields, optional script binding, native kind, and native protocol fields.

`documents` retains settings, material/config data, and prefab provenance. `assets` carries GUID, canonical `unity.<guid>` logical ID, source path, kind, hashes, original meta data, PNG dimensions, and typed sprite records. Each sprite has its file ID, name, pixel rectangle, pivot, border, and pixels per unit. Pixel rectangle origin is bottom-left. A renderer resolves the complete GUID/file-ID pair, then reads `root + asset.path`.

References in live fields become `{"$ref":"stable-key","guid":"...","file_id":123}`. Null references become JSON null. Missing references become `$missing` objects and source-local diagnostics. Explicit profile replacements use `$builtin`; configured package script identities use `$type`. Original metadata/provenance references remain in their serialized representation.

The host creates object/component handles in two passes, resolves graph references to those handles, sets local parents/TRS, and binds serialized script/config fields before Awake. Native state remains authoritative. Source XY coordinates and [x,y,z,w] quaternions are unchanged. Runtime/render coordinate conversion belongs at the extraction boundary.

Script bindings retain application fields including private serialized fields, nested arrays/structs, config references, and persistent UnityEvent target/method/argument metadata. Original `.cs` filenames and namespace declarations identify project script types. Package/DLL types require profile entries keyed by `guid:fileID`, with GUID-wide entries as a fallback. The managed loader supplies actual reflection-based field types and event semantics. Importing does not compile or execute C#.

## Profiles and sidecars

Prefab source, correspondence, and instance field names are profile data. The default accepts both modern and 2017 names. Source-script field aliases are keyed by script GUID. Aliases apply when the stale field exists; an explicitly serialized current field wins and emits `field_alias_superseded`. `field_exclusions` maps a script GUID and stale field name to an explicit reason. It excludes only binding/native Script fields, preserves component/document source fields, and emits `source_field_excluded`.

Observed source-specific conditions are recorded in [SOURCE-OBSERVATIONS.md](SOURCE-OBSERVATIONS.md). Check them again when the pinned originals or fixture mappings change. The importer has no branches that recognize project names.

A sidecar field override matches source GUID/file ID/property path and requires a reason. Paths support nested mappings, `Array.size`, and `Array.data[N]`. Source prefab modifications apply first, then sidecar fields, then reference resolution. A reference substitution matches a missing/original GUID and optional file ID; its explicit replacement and reason appear in diagnostics. Hosts own any authored visual replacement associated with a `$builtin` label. Original source art remains unchanged. Explicit fallback declarations can supply separately authored pixels.

## Native world materialization

```rust,ignore
let materialized = materialize(&graph, &admission_policy, |request| {
    host.request(world, &request)
})?;
let mut fixture = materialized.binding_payload(world, &graph);
fixture["assemblies"] = serde_json::json!([assembly_path]);
fixture["profile"] = managed_profile;
host.load_fixture(&fixture)?;
```

The caller owns the world and its lifetime. The closure returns unwrapped protocol response data, matching `CompatibilityHost::request`, and propagates native semantic errors. The crate has no static FFI dependency and never creates another world store. `materialization_plan` exposes serializable `NativeCommand` values for inspection or host transport. On an execution failure, discard the partial world. The executor never retries requests.

All objects allocate inactive. Implicit Transform component handles are read from object creation snapshots. RectTransform upgrades that implicit component while retaining its handle. Other components allocate next. Typed field references bind before hierarchy operations; local parenting/TRS applies next; RectTransform authored anchors bind after TRS because native transform writes can change anchored positions. Native UI computes evaluated rectangle poses. Final activation excludes templates.

`MaterializedScene.handles` supplies the binder's unified stable-ID to unsigned handle map, including aliases. `object_ids`, `component_ids`, `transform_objects`, `template_active`, and `skipped_components` provide operation-specific views. Transform component IDs differ from their owner object handles. Config/asset references remain stable strings for managed binding; native world references become numeric `$ref` values. The original imported graph remains unchanged.

`AdmissionPolicy::default()` rejects unsupported participants. `ignored_components` maps exact component IDs or source kind names to omission reasons. `ignored_scripts` maps original script GUID:fileID to omission reasons. Required unresolved or dangling references, absent owners and invalid parent graphs fail preflight. A known asset GUID alone cannot validate a nonexistent serialized subasset ID. Every omitted component and reason is returned explicitly.

Source-specific inputs are data in `profiles/{snake,breakout,pong}-source.json`, `profiles/{snake,breakout,pong}-admission.json` and `overlays/{snake,breakout,pong}.json`. The Breakout policy admits typed TilemapCollider2D collision. Its overlay explicitly selects sprite bounds for the original empty sprite physicsShape and labels the geometry fidelity. Unavailable render-pipeline/light extensions remain explicitly omitted. Native collision tests exercise the solver independently of managed gameplay. Pong explicitly overrides the serialized zero-scale Canvas sentinel to unit scale for native invertible TRS. Original source files remain unchanged.

Native component preparation includes body enum/mass/drag/gravity/angular/constraint fields and resolved physics material values. Tilemap fields include all original serialization plus `tiles` entries with `position`, `tile`, `sprite`, `color`, `matrix`, `object`, and `flags`, together with `grid`, `renderer`, `tile_anchor`, and `color`. TilemapCollider2D is prepared from the normalized Tilemap on the same owner, source tile collider enum, sprite subasset metadata and resolved PhysicsMaterial2D. Native fields are exactly `cell_size`, `cell_gap`, `tile_anchor`, `offset`, `is_trigger`, `material`, `cells`. Source enum 0/1/2 maps to none/sprite/grid. Cells retain integer XY positions and affine `[e00,e10,e01,e11,e03,e13]`. Nonplanar/perspective/singular source matrices reject admission. Authored physicsShape pixel vertices are divided by PPU into sprite-local world coordinates; simple concave outlines become convex pieces. Malformed shapes reject without fallback. When shapes are empty, `ImportSidecar.tile_collider_fallbacks` must explicitly select `geometry: "sprite_bounds"` for source collider GUID/file ID with a nonempty reason. The graph emits one `tile_collision_geometry_fallback` diagnostic with affected cell count and fidelity label. Bounds use original rect/PPU/pivot and perform no alpha tracing. Cell and generated-convex-shape budgets are 4096 each. Typed Text/Canvas/CanvasScaler/RectTransform fields match the native UI protocol. Hosts register an actual font under the imported Text `font` name, such as `host.builtin-font`.

## Authored fallback sprites and source census

`ImportSidecar.fallback_sprites` declares separately authored solid-color pixels and sprite metrics. Each declaration requires a name, reason, bounded texture dimensions, RGBA color, and `SpriteAsset` rect/pivot/PPU/file ID. The graph retains these declarations. `FallbackSprite::rgba8()` returns bounded actual pixels; the host can upload them through its existing texture path. Snake's explicit replacement is an 8x8 solid white unit square. Original source tint remains authoritative. No replacement pixels are inserted into original projects.

Reference substitutions retain the original GUID/file ID on builtin labels. A `source_reference_missing` diagnostic records an absent original even when an explicit substitution supplies admission data. Applied stale field aliases emit `source_field_alias`; an explicitly current field still wins.

`script_census(root, Path::new("Assets/Scripts"), &profile)` returns sorted source path, SHA256 and byte length for every selected runtime C# file, including nonserialized helpers and interfaces. Original selected runtime counts are 8, 7 and 20. Use the separate `census` example to emit JSON; the host owns compilation and assembly admission.

## Canonical packaging

`ImportedScene::package_inputs(sources, variant, source_package)` accepts current source bytes keyed by every `source_hashes` path. It rejects a stale or missing snapshot. Reimport accepts changed sources and produces new hashes normally.

The method returns the existing package crate's `AssetInput` records for original assets and a `unity.<scene-guid>.graph` Data artifact. Records carry canonical identities, dependency IDs, content hashes, and third-party source provenance. These inputs work with the existing `PackageBuilder` and `CatalogBuilder`. Raw image/mesh payloads still require the appropriate runtime decoder/import stage. Script helper compilation and full source revision hashing remain harness responsibilities.

## Bounds and diagnostics

Profiles bound file bytes, total reachable bytes, catalog entries, reachable assets/documents, expanded documents, and prefab nesting. Paths escaping the project are rejected. Symlink catalog entries are skipped. YAML decoding uses `serde_yaml`; Unity document headers and hexadecimal GUID scalars receive lexical normalization. The crate forbids unsafe code. The YAML dependency uses its own libyaml binding.

Importing collects missing references and unsupported component diagnostics while preserving the graph. The host chooses admission policy. Unreachable malformed YAML and unrelated duplicate GUIDs do not reject a selected scene. Reachable ambiguous GUIDs, malformed selected YAML, resource limits, and invalid sidecar targets return errors.

Supported prefab operations include recursive instance expansion, original 2017/modern correspondence, local override references, nested/array property overrides, parent overrides, and removed components. Added/removed GameObject operations and added-component lists produce explicit unsupported-operation diagnostics. Full engine asset bundles, arbitrary binary scene formats, package resolution/downloads, animation, UI layout, and arbitrary mesh conversion are outside this crate.

## Owned source snapshots

`import_content` returns a `ImportedContent` holding the imported graph, the exact source bytes read, and its import profile and sidecar. `graph()` and `sources()` expose immutable views. `verify_current(root)` reimports with those same inputs and rejects changed source bytes or source inventory. A fresh import accepts current sources. `package_current` checks freshness before returning package inputs. `import_content_with_source` accepts an explicit `ContentSource` for controlled source reads.

Materialization uses `AdmissionPolicy` directly and rejects unresolved metadata references before issuing native commands.

## Verification

Run the Rust behavior tests and strict lints:

```sh
cargo +1.97.0 test -p worldrunner-original-games -p worldrunner-compat-content --all-features -- --include-ignored --test-threads=1
cargo +1.97.0 clippy -p worldrunner-original-games -p worldrunner-compat-content --all-targets --all-features -- -D warnings
cargo +1.97.0 fmt -p worldrunner-original-games -p worldrunner-compat-content -- --check
```

Tests resolve the read-only originals through `worldrunner_original_games::checkout`. Error and changed-source tests copy selected assets into disposable temporary directories. They establish parsing, prefab expansion, graph references, canonical packaging, source drift rejection and materialization behavior.

The Python tests build their Rust examples, original-game fetcher and native shared library with two Cargo jobs. They fetch pinned originals through the CLI, import fresh graphs, and create temporary transport files under `CARGO_TARGET_DIR` (default `target/content`). Each test cleans up its files and native worlds. They require the Rust 1.97.0 toolchain and cached Cargo dependencies:

```sh
python3 -m unittest discover -s crates/compat-content/tests -p 'native_*.py' -v
```

The native transport test checks all three imported worlds, authored transforms, template inactivity, resolved references and typed component fields. Tile collision tests check all 48 wall centers, the clear interior, deduplicated ray queries, continuous bounces including a 15-unit swept move, source affine conversion, independent clone ownership and world cleanup. These tests execute no managed callbacks and make no gameplay or GPU coverage claim.
