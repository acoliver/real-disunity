# Original sample admission observations

These observations describe the pinned originals selected by `worldrunner-original-games`. Check [profiles and fixture sidecars](README.md#profiles-and-sidecars) again when the [original-game pins](../original-games/README.md) change. The importer accepts updated sources; source hashes identify imported content.

## Snake scene and missing visual

`Assets/Scenes/GameScene.unity` references `GameConfig.asset`, two prefab assets, and four scene MonoBehaviours. The serialized config grid is 40 by 20, with wrapping enabled and eight ticks per second. These values differ from some C# field initializers, so serialized values take precedence.

Both `Assets/Visuals/FoodVisual.prefab` and `Assets/Visuals/SneakSegment.prefab` reference missing sprite GUID `311925a002f4447b3a28927169b83ea6`, file ID `7482667652216324306`. The source checkouts contain no corresponding texture. Fixtures may explicitly substitute a separately authored visual. The importer leaves the reference unresolved by default.

A fixture-owned sidecar can declare the replacement identity below. Its host must supply actual authored visual content for the label before rendering. This declaration supplies no pixels:

```json
{
  "reference_substitutions": [{
    "guid": "311925a002f4447b3a28927169b83ea6",
    "file_id": 7482667652216324306,
    "replacement": {"$builtin": "fixture.authored-snake-unit-square"},
    "reason": "Original referenced sprite is absent; fixture supplies separately authored visual"
  }]
}
```

## Breakout field and tag metadata

`Assets/Prefabs/Ball.prefab` contains `startSpeed: 6`, while original `Assets/Scripts/Ball.cs` exposes `speed`. `profiles/source-field-aliases.json` maps that stale field to `speed` for script GUID `496b9770dcb4e9ecab5379f35c859b35`. The alias applies only if `startSpeed` exists. A current serialized `speed` field takes precedence.

The selected original Ball prefab already has `m_TagString: Ball`, matching the original collision callbacks. No tag override is required for this revision. A different fixture revision with a stale tag can declare a reasoned source GUID/file-ID `m_TagString` sidecar override after checking the actual source metadata. Never apply a tag correction based on project name alone.

The missing material GUID `a97c105638bdf8b4a8650670310a4cd3`, file ID `2100000`, appears on source SpriteRenderer/TilemapRenderer records. Scene MonoBehaviours also reference missing package script GUIDs `a79441f348de89743a2939f4d699eac1` and `073797afb82c5a1438f328866b10b3f0`. These remain diagnostic dependencies; importing the selected gameplay graph does not reject the entire project.

## Pong 2017 correspondence and package types

The original project version is 2017.1.0f3. Scene Prefab records use `m_ParentPrefab`; stripped records use `m_PrefabParentObject` and `m_PrefabInternal`. Scene modifications set axis strings, transforms, goal parents, and persistent UnityEvent targets/methods.

The original Goal prefab retains a stale `team` field while original Goal.cs declares `scoresTo`. The source-specific profile maps `team` to `scoresTo` for script GUID `fec2b0e28e0bb6e4f930b2db3fb4f7c9`. A scene-authored `scoresTo` override takes precedence. Behavioral tests verify that the two scene goal values remain 0 and 1 and that their `ScorePoint` event targets resolve to the expanded MatchManager component.

UI DLL GUID `f70555f144d8491a825f0804e09c671c` identifies multiple classes through different file IDs. Actual serialized Text records use file ID `708705254`; `profiles/pong-source.json` declares that type and explicit host builtin font/material identities. The profile also maps file ID `1980459831` to `UnityEngine.UI.CanvasScaler`. Three other scene UI DLL file IDs remain unresolved by this profile: `1077351063`, `-619905303`, and `1301386320`. Their fields remain available for a future supported UI adapter. CanvasRenderer, Behaviour, and AudioListener components also retain diagnostics.

Builtin font `0000000000000000e000000000000000:10102` and sprite material `0000000000000000f000000000000000:10754` require generic host implementations. The profile names these requirements and supplies no replacement renderer. GUIDs containing only digits or exponent-looking hexadecimal text remain strings throughout YAML decoding.

## Default import counts

With no source corrections or builtin/type profile, the original selected scenes produce:

| Scene | Scene objects | Template objects | Components | Script/config bindings | Assets | Documents |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Snake GameScene | 6 | 2 | 18 | 5 | 9 | 32 |
| Breakout SampleScene | 10 | 2 | 39 | 8 | 15 | 57 |
| Pong Main | 18 | 8 | 87 | 19 | 17 | 138 |

Diagnostics are deduplicated by source path/file ID/field/code/message. Identical defects in a prefab template and its scene copy share a source-local diagnostic. Snake has two missing-GUID, three builtin-replacement, and one unsupported-component diagnostics. Breakout has eight missing-GUID, two builtin-replacement, five unsupported-component, one invalid-tile-collider (no default bounds selection), and two unavailable-script-type diagnostics. Pong has six missing-GUID, six builtin-replacement, eleven unsupported-component, and six unavailable-script-type diagnostics.

The imported graph preserves field paths, source GUIDs, file IDs and component names. Counts establish import behavior. Native admission, script execution and rendering require host tests.

## Native admission mappings

Reasoned overlays are present in `overlays/{snake,breakout,pong}.json`; source and native admission profiles are in `profiles`. Snake's authored square includes bounded actual RGBA generation through `FallbackSprite::rgba8()` and leaves missing-original diagnostics visible. Breakout's absent 2D material explicitly selects the host unlit material while retaining original texels. Applied stale source field aliases emit diagnostics.

Pong's original Canvas root RectTransform `2f4cefa539f44df40a7fe27b9fab654a:1901759440` serializes zero scale before engine-owned screen UI layout. The explicit overlay sets its three local scale axes to one because native TRS rejects noninvertible scale. Native UI computes evaluated root position/scale from the orthographic camera, viewport and CanvasScaler. Importer copies authored anchors and applies them after TRS so native position setters cannot overwrite their initial values. This is an explicit compatibility overlay, independent of game logic.

The [native transport test](tests/native_transport.py) checks Snake 8 objects / 16 components, Breakout 12 / 33, and Pong 26 / 77, with template nodes native-inactive. Exact omitted component IDs/reasons remain in each materialization result. Breakout's policy admits typed TilemapCollider2D. The source-bound overlay explicitly selects sprite_bounds for all 48 wall cells because original sprite physicsShape is empty. The material resolves to bounciness 1/friction 0. The [native tile collision tests](tests/native_tile_collision.py) check repeated wall bounces, covered centers, clear interior and affine/clone behavior. Native Tilemap receives all 48 authored cells, sprite references, source grid dimensions and renderer metadata. Pong's Text, RectTransform, Canvas and CanvasScaler use typed native fields.

Original reachable texture paths are `Assets/Images/space.png`, `Assets/Images/coconut.jpg`, `Assets/Images/spritesheet-breakout.png`, and Pong `Assets/Sprites/{Ball,Player}.png`. PNG dimensions and sprite metadata are imported; JPEG bytes/path are retained without a native JPEG pixel decoder in this crate. The host owns payload decoding and GPU upload.

## Current Breakout source profile exclusions

Original Brick.cs declares frameTime and uses PowerUp.Instance.Apply(). Its serialized prefab retains powerUpChance=0.1 and widePlatformDuration=8. The exact source script GUID 57fc46ce82be2cd0284f27309fe6c2a6 has reasoned field_exclusions for those stale names. Both values remain in component/document source fields; script/native binding fields omit them and emit two source_field_excluded diagnostics. PowerUp.cs owns its current chance and duration fields. Ball.startSpeed -> speed remains unchanged. Fixture metadata selects these source mappings through the committed profile.
