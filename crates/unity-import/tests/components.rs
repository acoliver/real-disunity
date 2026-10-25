use std::fmt::Write;

#[path = "support/fixture.rs"]
mod fixture;
#[path = "support/objects.rs"]
mod objects;

use fixture::Fixture;
use objects::{game_object, import, transform};
use worldrunner_unity_import::{
    Combine, ObjectId, PhysicsMaterial, Reference, RendererMaterial, Scene,
};

const TILE: &str = "c0000000000000000000000000000001";
const SHEET: &str = "c0000000000000000000000000000002";
const BOUNCY: &str = "c0000000000000000000000000000003";
const LEGACY: &str = "c0000000000000000000000000000004";
const CUSTOM: &str = "c0000000000000000000000000000005";
const ABSENT: &str = "c0000000000000000000000000000006";
const SPRITES_DEFAULT: &str = "{fileID: 10754, guid: 0000000000000000f000000000000000, type: 0}";

fn fixture() -> Fixture {
    let fixture = Fixture::new("6000.3.5f2");
    fixture.asset(
        "Assets/brick.asset",
        TILE,
        "NativeFormatImporter",
        "--- !u!114 &11400000\nMonoBehaviour:\n  m_Name: brick\n",
    );
    fixture.asset("Assets/sheet.png", SHEET, "TextureImporter", "png");
    fixture.asset("Assets/Bouncy.physicsMaterial2D", BOUNCY, "NativeFormatImporter", "--- !u!62 &6200000\nPhysicsMaterial2D:\n  friction: 0.25\n  bounciness: 0.5\n  m_FrictionCombine: 3\n  m_BounceCombine: 0\n");
    fixture.asset(
        "Assets/Old.physicsMaterial2D",
        LEGACY,
        "NativeFormatImporter",
        "--- !u!62 &6200000\nPhysicsMaterial2D:\n  friction: 0\n  bounciness: 1\n",
    );
    fixture.asset(
        "Assets/Custom.mat",
        CUSTOM,
        "NativeFormatImporter",
        "--- !u!21 &2100000\nMaterial:\n  m_Name: Custom\n",
    );
    fixture
}

fn component(class: u32, name: &str, id: u64, object: u64, fields: &str) -> String {
    format!("--- !u!{class} &{id}\n{name}:\n  m_GameObject: {{fileID: {object}}}\n{fields}")
}

fn material(guid: &str) -> String {
    format!("  m_Material: {{fileID: 6200000, guid: {guid}, type: 2}}\n")
}

fn renderer(id: u64, object: u64, materials: &[&str]) -> String {
    let list = if materials.is_empty() {
        " []\n".to_owned()
    } else {
        let mut list = String::new();
        for m in materials {
            write!(list, "\n  - {m}").unwrap();
        }
        list + "\n"
    };
    component(
        212,
        "SpriteRenderer",
        id,
        object,
        &format!("  m_Materials:{list}"),
    )
}

/// Tiles as `(x, y, asset, sprite, matrix, color)` indices into two-entry lookup arrays.
fn tilemap(id: u64, object: u64, tiles: &[(i64, i64, u32, u32, u32, u32)], extra: &str) -> String {
    let mut text = String::from("  m_Tiles:\n");
    for (x, y, asset, sprite, matrix, color) in tiles {
        write!(
            text,
            "  - first: {{x: {x}, y: {y}, z: 0}}\n    second:\n      m_TileIndex: {asset}\n      m_TileSpriteIndex: {sprite}\n      m_TileMatrixIndex: {matrix}\n      m_TileColorIndex: {color}\n      m_TileObjectToInstantiateIndex: 65535\n      m_AllTileFlags: 1073741825\n"
        ).unwrap();
    }
    write!(
        text,
        "  m_AnimatedTiles: {{}}\n  m_TileAssetArray:\n  - m_RefCount: 3\n    m_Data: {{fileID: 11400000, guid: {TILE}, type: 2}}\n  m_TileSpriteArray:\n  - m_RefCount: 2\n    m_Data: {{fileID: 5, guid: {SHEET}, type: 3}}\n  - m_RefCount: 1\n    m_Data: {{fileID: -6, guid: {SHEET}, type: 3}}\n"
    ).unwrap();
    text += "  m_TileMatrixArray:\n  - m_RefCount: 2\n    m_Data:\n      e00: 1\n      e11: 1\n      e22: 1\n      e33: 1\n  - m_RefCount: 1\n    m_Data:\n      e00: 2\n      e11: 2\n      e22: 1\n      e33: 1\n";
    text += "  m_TileColorArray:\n  - m_RefCount: 2\n    m_Data: {r: 1, g: 1, b: 1, a: 1}\n  - m_RefCount: 1\n    m_Data: {r: 1, g: 0, b: 0, a: 0.5}\n";
    text += "  m_Color: {r: 1, g: 1, b: 1, a: 0.5}\n  m_TileAnchor: {x: 0.5, y: 0.25, z: 0}\n  m_TileOrientation: 0\n";
    component(1_839_735_485, "Tilemap", id, object, &(text + extra))
}

fn grid(id: u64, object: u64, layout: u32) -> String {
    component(
        156_049_354,
        "Grid",
        id,
        object,
        &format!(
            "  m_CellSize: {{x: 2, y: 1, z: 0}}\n  m_CellGap: {{x: 0.5, y: 0, z: 0}}\n  m_CellLayout: {layout}\n  m_CellSwizzle: 0\n"
        ),
    )
}

const TILES: &[(i64, i64, u32, u32, u32, u32)] = &[
    (0, 0, 0, 0, 0, 0),
    (1, -2, 0, 1, 1, 1),
    (2, 0, 0, 65535, 0, 0),
];

fn scene_text() -> String {
    let none = "  m_Material: {fileID: 0}\n";
    [
        game_object(1, "Grid", &[2, 3]),
        transform(2, 1, 0.0, 0),
        grid(3, 1, 0),
        game_object(10, "Bricks", &[11, 12, 13]),
        transform(11, 10, 0.0, 2),
        tilemap(12, 10, TILES, ""),
        component(19_719_996, "TilemapCollider2D", 13, 10, none),
        game_object(20, "Ball", &[21, 22, 23, 24]),
        transform(21, 20, 0.0, 0),
        component(50, "Rigidbody2D", 22, 20, &material(BOUNCY)),
        component(58, "CircleCollider2D", 23, 20, none),
        renderer(24, 20, &[SPRITES_DEFAULT]),
        game_object(30, "Own", &[31, 32]),
        transform(31, 30, 0.0, 21),
        component(61, "BoxCollider2D", 32, 30, &material(LEGACY)),
        game_object(40, "Inherits", &[41, 42]),
        transform(41, 40, 0.0, 21),
        component(61, "BoxCollider2D", 42, 40, none),
        game_object(50, "Renderers", &[51, 52, 53, 54, 55]),
        transform(51, 50, 0.0, 0),
        renderer(
            52,
            50,
            &[&format!("{{fileID: 2100000, guid: {ABSENT}, type: 2}}")],
        ),
        renderer(
            53,
            50,
            &[&format!("{{fileID: 2100000, guid: {CUSTOM}, type: 2}}")],
        ),
        renderer(54, 50, &[SPRITES_DEFAULT, SPRITES_DEFAULT]),
        renderer(55, 50, &[]),
    ]
    .concat()
}

fn by_file_id(scene: &Scene, file_id: i64) -> ObjectId {
    scene
        .objects()
        .find(|(_, object)| object.source.asset.is_none() && object.source.file_id == file_id)
        .map_or_else(|| panic!("no object {file_id}"), |(id, _)| id)
}

fn sheet(file_id: i64) -> Reference {
    Reference::External {
        guid: SHEET.parse().unwrap(),
        file_id,
    }
}

#[test]
fn grids_and_tilemaps_resolve_cells_through_their_lookup_arrays() {
    let fixture = fixture();
    let scene = import(&fixture, "Assets/Main.unity", &scene_text()).unwrap();
    let grid = scene.grid(by_file_id(&scene, 3)).unwrap();
    assert_eq!(
        grid.cell_size.map(f64::to_bits),
        [2.0, 1.0, 0.0].map(f64::to_bits)
    );
    assert_eq!(
        grid.cell_gap.map(f64::to_bits),
        [0.5, 0.0, 0.0].map(f64::to_bits)
    );
    let map = scene.tilemap(by_file_id(&scene, 12)).unwrap();
    assert_eq!(
        map.tile_anchor.map(f64::to_bits),
        [0.5, 0.25, 0.0].map(f64::to_bits)
    );
    assert_eq!(
        map.color.map(f64::to_bits),
        [1.0, 1.0, 1.0, 0.5].map(f64::to_bits)
    );
    let cells: Vec<_> = map.tiles.iter().map(|tile| tile.cell).collect();
    assert_eq!(cells, [[0, 0, 0], [1, -2, 0], [2, 0, 0]]);
    let Reference::Object(asset) = map.tiles[0].asset else {
        panic!("tile asset {:?}", map.tiles[0].asset)
    };
    assert_eq!(scene.object(asset).unwrap().type_name, "MonoBehaviour");
    assert!(
        map.tiles
            .iter()
            .all(|tile| tile.asset == map.tiles[0].asset)
    );
    assert_eq!(map.tiles[0].sprite, sheet(5));
    assert_eq!(map.tiles[1].sprite, sheet(-6));
    assert_eq!(map.tiles[2].sprite, Reference::Null, "65535 is no sprite");
    assert_eq!(map.tiles[1].transform[0].to_bits(), 2.0_f64.to_bits());
    assert_eq!(map.tiles[0].transform[0].to_bits(), 1.0_f64.to_bits());
    assert_eq!(
        map.tiles[1].color.map(f64::to_bits),
        [1.0, 0.0, 0.0, 0.5].map(f64::to_bits)
    );
    assert_eq!(map.tiles[0].flags, 1_073_741_825);
}

#[test]
fn collider_materials_prefer_their_own_then_the_attached_rigidbody_then_the_default() {
    let fixture = fixture();
    let scene = import(&fixture, "Assets/Main.unity", &scene_text()).unwrap();
    let bouncy = PhysicsMaterial {
        friction: 0.25,
        bounciness: 0.5,
        friction_combine: Combine::Minimum,
        bounce_combine: Combine::Average,
    };
    let legacy = PhysicsMaterial {
        friction: 0.0,
        bounciness: 1.0,
        ..PhysicsMaterial::DEFAULT
    };
    let material = |file_id| {
        scene
            .collider_material(by_file_id(&scene, file_id))
            .unwrap()
    };
    assert_eq!(material(23), bouncy, "the ball's rigidbody material");
    assert_eq!(material(32), legacy, "the collider's own material wins");
    assert_eq!(
        material(42),
        bouncy,
        "a child collider uses its parent's rigidbody"
    );
    assert_eq!(material(13), PhysicsMaterial::DEFAULT);
    assert_eq!(legacy.friction_combine, Combine::Mean);
    assert_eq!(legacy.bounce_combine, Combine::Maximum);
}

#[test]
fn renderers_accept_only_the_built_in_sprite_material() {
    let fixture = fixture();
    let scene = import(&fixture, "Assets/Main.unity", &scene_text()).unwrap();
    let material = |file_id| scene.renderer_material(by_file_id(&scene, file_id));
    assert_eq!(material(24).unwrap(), RendererMaterial::SpritesDefault);
    let message = |file_id| material(file_id).unwrap_err().to_string();
    let absent = message(52);
    assert!(
        absent.contains("SpriteRenderer 52 in Assets/Main.unity")
            && absent.contains("not in the project"),
        "{absent}"
    );
    assert!(message(53).contains("custom material"), "{}", message(53));
    assert!(message(54).contains("has 2 materials"), "{}", message(54));
    assert!(message(55).contains("has 0 materials"), "{}", message(55));
}

fn component_error(scene_text: &str, check: impl Fn(&Scene, ObjectId) -> String) -> String {
    let fixture = fixture();
    let scene = import(&fixture, "Assets/Bad.unity", scene_text).unwrap();
    check(&scene, by_file_id(&scene, 12))
}

#[test]
fn unsupported_tilemap_and_grid_settings_fail_with_their_object() {
    let tilemap_error =
        |text: &str| component_error(text, |scene, id| scene.tilemap(id).unwrap_err().to_string());
    let animated =
        tilemap(12, 10, TILES, "").replace("m_AnimatedTiles: {}", "m_AnimatedTiles:\n    0: 1");
    assert!(tilemap_error(&animated).contains("animated tiles"));
    let spawning = tilemap(12, 10, &TILES[..1], "").replace(
        "ObjectToInstantiateIndex: 65535",
        "ObjectToInstantiateIndex: 0",
    );
    assert!(tilemap_error(&spawning).contains("instantiate GameObjects"));
    let outside = tilemap(12, 10, &[(0, 0, 0, 7, 0, 0)], "");
    let message = tilemap_error(&outside);
    assert!(
        message.contains("Tilemap 12") && message.contains("tile at [0, 0, 0]: index 7"),
        "{message}"
    );
    let rotated =
        tilemap(12, 10, TILES, "").replace("m_TileOrientation: 0", "m_TileOrientation: 1");
    assert!(tilemap_error(&rotated).contains("XY tile orientation"));
    let hexagon = component_error(&grid(12, 10, 2), |scene, id| {
        scene.grid(id).unwrap_err().to_string()
    });
    assert!(hexagon.contains("rectangle cell layout"), "{hexagon}");
    let wrong = component_error(&grid(12, 10, 0), |scene, id| {
        scene.tilemap(id).unwrap_err().to_string()
    });
    assert!(wrong.contains("is not a Tilemap"), "{wrong}");
}
