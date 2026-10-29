//! Textures, sprites and sprite atlases in a player's resource plan.

use std::path::Path;

use serde_json::json;
use worldrunner_unity_import::{
    PlayerError,
    player_resources::{ResourceAsset, ResourcesPlan, plan_player_resources},
    player_sprites::TextureData,
};
use worldrunner_unity_serialized::{
    ClassId, External, SerializedType,
    object::PPtr,
    script_data::ScriptLayouts,
    write::{FileContents, ObjectData, ObjectWriter},
};

const TYPES: [ClassId; 4] = [
    ClassId::RESOURCE_MANAGER,
    ClassId::TEXTURE_2D,
    ClassId::SPRITE,
    ClassId::SPRITE_ATLAS,
];

const PIXELS: [u8; 16] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
const STREAM: [u8; 24] = [
    90, 91, 92, 93, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10,
];

const fn ptr(path_id: i64) -> PPtr {
    PPtr {
        file_id: 0,
        path_id,
    }
}

fn object(path_id: i64, class_id: ClassId, data: Vec<u8>) -> ObjectData {
    let type_index = TYPES.iter().position(|&c| c == class_id).unwrap();
    ObjectData {
        path_id,
        type_index: i32::try_from(type_index).unwrap(),
        data,
    }
}

fn write_file(dir: &Path, name: &str, objects: Vec<ObjectData>, externals: &[&str]) {
    let class = |class_id: ClassId| SerializedType {
        class_id,
        stripped: false,
        script_type_index: -1,
        script_id: None,
        old_type_hash: [0; 16],
        type_tree: None,
        dependencies: Vec::new(),
        script_class: None,
    };
    let contents = FileContents {
        version: 22,
        big_endian: false,
        unity_version: "6000.3.15f1".into(),
        target_platform: 19,
        type_trees: false,
        types: TYPES.into_iter().map(class).collect(),
        objects,
        scripts: Vec::new(),
        externals: externals
            .iter()
            .map(|path| External {
                guid: [0; 16],
                kind: 0,
                path: (*path).into(),
            })
            .collect(),
        ref_types: Vec::new(),
        user_information: String::new(),
    };
    std::fs::write(dir.join(name), contents.write().unwrap()).unwrap();
}

/// Where a texture keeps its pixels.
enum Pixels {
    Inline(&'static [u8]),
    /// A byte range of the named `.resS` file.
    Streamed(&'static str, u64, u32),
}

/// A 2x2 RGBA32 texture of one mip level with the sampler settings bilinear, 4x aniso, a bias
/// of -0.5 and wrap modes repeat, clamp and mirror, in the sRGB color space (value 1).
fn texture(name: &str, readable: bool, images: i32, dimension: i32, pixels: &Pixels) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.string(name).unwrap().u8(0).align();
    w.i32(2).i32(2).u32(16).i32(0).i32(4).i32(1);
    w.u8(u8::from(readable)).u8(0).u8(0).align();
    w.string("").unwrap().u8(0).align().i32(0).align();
    w.i32(images).i32(dimension);
    w.i32(1).i32(4).f32(-0.5).i32(0).i32(1).i32(2);
    w.i32(6).i32(1).byte_array(&[]).unwrap();
    match pixels {
        Pixels::Inline(bytes) => {
            w.byte_array(bytes)
                .unwrap()
                .u64(0)
                .u32(0)
                .string("")
                .unwrap();
        }
        Pixels::Streamed(path, offset, size) => {
            w.byte_array(&[])
                .unwrap()
                .u64(*offset)
                .u32(*size)
                .string(path)
                .unwrap();
        }
    }
    w.finish()
}

/// A sprite named `name` of a 16x8 rectangle at (2, 4) with a half pivot, 50 pixels per unit and
/// a border, drawn from `texture`, packed (settings bit 0), rotation 3 and mesh type 1.
fn sprite(name: &str, texture: i64, bones: bool) -> Vec<u8> {
    sprite_in(name, texture, 0, bones)
}

/// A sprite as [`sprite`], packed into the atlas object `atlas` (0 for none).
fn sprite_in(name: &str, texture: i64, atlas: i64, bones: bool) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.string(name).unwrap();
    w.floats(&[2.0, 4.0, 16.0, 8.0]).floats(&[0.0, 0.0]);
    w.floats(&[1.0, 2.0, 3.0, 4.0])
        .f32(50.0)
        .floats(&[0.5, 0.25]);
    w.u32(1).u8(0).align();
    w.u32(1).u32(2).u32(3).u32(4).i64(-7);
    w.i32(0).pptr(ptr(atlas));
    w.pptr(ptr(texture)).pptr(ptr(0));
    w.i32(0).i32(0).byte_array(&[]).unwrap();
    w.u32(0).i32(0).byte_array(&[]).unwrap().align();
    w.i32(0).align();
    w.floats(&[3.0, 5.0, 14.0, 6.0])
        .floats(&[1.0, 0.5])
        .floats(&[-1.0, -1.0]);
    w.u32(1 | (3 << 2) | (1 << 6))
        .floats(&[50.0, 8.0, 50.0, 2.0])
        .f32(1.0);
    w.i32(0).align();
    if bones {
        w.i32(1).string("root").unwrap().string("guid").unwrap();
        w.floats(&[0.0; 3])
            .floats(&[0.0, 0.0, 0.0, 1.0])
            .f32(1.0)
            .i32(-1)
            .u32(0);
    } else {
        w.i32(0);
    }
    w.align().pptrs(&[]).unwrap();
    w.finish()
}

fn atlas(name: &str, packed: &[PPtr]) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.string(name).unwrap().pptrs(packed).unwrap();
    w.i32(0).i32(0);
    w.string("ui-tag").unwrap().u8(1).align();
    w.finish()
}

/// An atlas packing the sprite at `packed`, whose render data key is the one [`sprite`] writes,
/// into the texture at `texture`.
fn atlas_placing(name: &str, packed: i64, texture: i64) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.string(name).unwrap().pptrs(&[ptr(packed)]).unwrap();
    w.i32(1).string("packed").unwrap().align();
    w.i32(1).u32(1).u32(2).u32(3).u32(4).i64(-7);
    w.pptr(ptr(texture)).pptr(ptr(0));
    w.floats(&[6.0, 7.0, 8.0, 9.0])
        .floats(&[0.25, 0.75])
        .floats(&[-1.0, -1.0])
        .floats(&[50.0, 8.0, 50.0, 2.0]);
    w.f32(1.0).u32(1 | (2 << 2)).i32(0);
    w.string("packer-tag").unwrap().u8(0).align();
    w.finish()
}

fn resource_manager() -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    let entries = [
        ("art/sheet", ptr(2)),
        ("art/sheet", ptr(3)),
        ("art/sheet", ptr(4)),
        ("art/atlas", ptr(5)),
        ("art/cube", ptr(10)),
    ];
    w.i32(i32::try_from(entries.len()).unwrap());
    for (path, target) in entries {
        w.string(path).unwrap();
        w.pptr(PPtr {
            file_id: 1,
            path_id: target.path_id,
        });
    }
    w.i32(0);
    w.finish()
}

/// Writes a player whose container lists the sheet's texture and two sprites under one path, an
/// atlas of the first sprite and a sprite the container does not list (drawn from a streamed
/// texture it does not list either), and a cube map.
fn write_player(dir: &Path, stream_file: bool) {
    write_file(
        dir,
        "globalgamemanagers",
        vec![object(1, ClassId::RESOURCE_MANAGER, resource_manager())],
        &["resources.assets"],
    );
    let streamed = Pixels::Streamed("archive:/CAB-1/resources.assets.resS", 4, 16);
    write_file(
        dir,
        "resources.assets",
        vec![
            object(
                2,
                ClassId::TEXTURE_2D,
                texture("sheet", true, 1, 2, &Pixels::Inline(&PIXELS)),
            ),
            object(3, ClassId::SPRITE, sprite("first", 2, false)),
            object(4, ClassId::SPRITE, sprite("second", 2, true)),
            object(
                5,
                ClassId::SPRITE_ATLAS,
                atlas("icons", &[ptr(3), ptr(8), ptr(0)]),
            ),
            object(8, ClassId::SPRITE, sprite("hidden", 9, false)),
            object(
                9,
                ClassId::TEXTURE_2D,
                texture("far", false, 1, 2, &streamed),
            ),
            object(
                10,
                ClassId::TEXTURE_2D,
                texture("cube", true, 6, 4, &Pixels::Inline(&[])),
            ),
        ],
        &[],
    );
    if stream_file {
        std::fs::write(dir.join("resources.assets.resS"), STREAM).unwrap();
    }
}

fn plan(dir: &Path) -> Result<ResourcesPlan, PlayerError> {
    plan_player_resources(dir, &ScriptLayouts::new(Vec::new()), &dir.join("out"))
}

fn planned() -> (tempfile::TempDir, ResourcesPlan) {
    let dir = tempfile::tempdir().unwrap();
    write_player(dir.path(), true);
    let plan = plan(dir.path()).unwrap();
    (dir, plan)
}

#[test]
fn plans_a_texture_with_inline_pixels_into_a_bin_file() {
    let (dir, plan) = planned();
    let ResourceAsset::Texture2D {
        name,
        width,
        height,
        format,
        mip_count,
        readable,
        color_space,
        filter_mode,
        wrap_u,
        wrap_v,
        wrap_w,
        aniso_level,
        mip_bias,
        data,
    } = &plan.assets[0]
    else {
        panic!("the first asset is the sheet texture: {:?}", plan.assets[0])
    };
    assert_eq!(
        (
            name.as_str(),
            *width,
            *height,
            *format,
            *mip_count,
            *readable,
            *color_space
        ),
        ("sheet", 2, 2, 4, 1, true, 1)
    );
    assert_eq!(
        (
            *filter_mode,
            *wrap_u,
            *wrap_v,
            *wrap_w,
            *aniso_level,
            *mip_bias
        ),
        (1, 0, 1, 2, 4, -0.5)
    );
    assert_eq!((data.offset, data.size), (0, 16));
    let bytes = std::fs::read(dir.path().join("out").join(&data.file)).unwrap();
    assert_eq!(bytes, PIXELS);
}

#[test]
fn plans_a_streamed_texture_by_its_range_in_the_resource_stream() {
    let (dir, plan) = planned();
    let ResourceAsset::Texture2D {
        name,
        readable,
        data,
        ..
    } = &plan.assets[5]
    else {
        panic!("the sixth asset is the far texture: {:?}", plan.assets[5])
    };
    assert_eq!((name.as_str(), *readable), ("far", false));
    let expected = std::path::absolute(dir.path().join("resources.assets.resS")).unwrap();
    let TextureData { file, offset, size } = data;
    assert_eq!((file, *offset, *size), (&expected, 4, 16));
    let bytes = std::fs::read(file).unwrap();
    assert_eq!(&bytes[4..20], &STREAM[4..20]);
}

#[test]
fn fails_to_plan_a_texture_whose_streamed_data_is_missing() {
    let dir = tempfile::tempdir().unwrap();
    write_player(dir.path(), false);
    let message = plan(dir.path()).unwrap_err().to_string();
    assert!(message.contains("texture far"), "{message}");
    assert!(message.contains("resources.assets.resS"), "{message}");
}

#[test]
fn plans_a_sprite_with_its_texture_and_geometry() {
    let (_dir, plan) = planned();
    let ResourceAsset::Sprite {
        name,
        rect,
        pivot,
        pixels_per_unit,
        border,
        extrude,
        texture,
        texture_rect,
        texture_rect_offset,
        packed,
        packing_rotation,
        mesh_type,
    } = &plan.assets[1]
    else {
        panic!("the second asset is the first sprite: {:?}", plan.assets[1])
    };
    assert_eq!(name, "first");
    assert_eq!(
        (*rect, *pivot, *pixels_per_unit),
        ([2.0, 4.0, 16.0, 8.0], [0.5, 0.25], 50.0)
    );
    assert_eq!((*border, *extrude), ([1.0, 2.0, 3.0, 4.0], 1));
    assert_eq!(serde_json::to_value(texture).unwrap(), json!({"$asset": 0}));
    assert_eq!(
        (*texture_rect, *texture_rect_offset),
        ([3.0, 5.0, 14.0, 6.0], [1.0, 0.5])
    );
    assert_eq!((*packed, *packing_rotation, *mesh_type), (true, 3, 1));
}

#[test]
fn plans_an_atlas_with_its_sprites_in_order_and_the_ones_it_alone_reaches() {
    let (_dir, plan) = planned();
    let ResourceAsset::SpriteAtlas {
        name,
        tag,
        is_variant,
        sprites,
    } = &plan.assets[3]
    else {
        panic!("the fourth asset is the atlas: {:?}", plan.assets[3])
    };
    assert_eq!(
        (name.as_str(), tag.as_str(), *is_variant),
        ("icons", "ui-tag", true)
    );
    let planned: Vec<_> = sprites.iter().map(|s| s.asset).collect();
    assert_eq!(planned, [1, 4]);
    let ResourceAsset::Sprite { name, texture, .. } = &plan.assets[4] else {
        panic!(
            "the atlas's second sprite follows the listed assets: {:?}",
            plan.assets[4]
        )
    };
    assert_eq!(name, "hidden");
    assert_eq!(texture.as_ref().map(|t| t.asset), Some(5));
}

#[test]
fn lists_a_texture_and_its_sprites_under_one_path() {
    let (_dir, plan) = planned();
    let sheet: Vec<_> = plan
        .entries
        .iter()
        .filter(|entry| entry.path == "art/sheet")
        .map(|entry| (entry.class.as_str(), entry.asset))
        .collect();
    assert_eq!(
        sheet,
        [
            ("Texture2D", Some(0)),
            ("Sprite", Some(1)),
            ("Sprite", Some(2))
        ]
    );
}

#[test]
fn keeps_only_the_class_of_a_texture_that_is_not_a_single_2d_image() {
    let (_dir, plan) = planned();
    let cube = plan
        .entries
        .iter()
        .find(|entry| entry.path == "art/cube")
        .unwrap();
    assert_eq!((cube.class.as_str(), cube.asset), ("Texture2D", None));
    assert!(
        plan.differences
            .iter()
            .any(|line| line.contains("texture cube") && line.contains("not built")),
        "{:?}",
        plan.differences
    );
}

#[test]
fn notes_what_a_sprite_has_that_the_engine_does_not_build() {
    let (_dir, plan) = planned();
    let notes: Vec<_> = plan
        .differences
        .iter()
        .filter(|line| line.contains("second"))
        .collect();
    assert_eq!(notes, ["resource sprite second: bones are not built"]);
}

/// Writes a player whose container lists only an atlas that packs a sprite with no texture of
/// its own into a texture.
fn write_packed_player(dir: &Path) {
    let mut w = ObjectWriter::new(false);
    w.i32(1).string("art/packer").unwrap().pptr(PPtr {
        file_id: 1,
        path_id: 12,
    });
    w.i32(0);
    write_file(
        dir,
        "globalgamemanagers",
        vec![object(1, ClassId::RESOURCE_MANAGER, w.finish())],
        &["resources.assets"],
    );
    write_file(
        dir,
        "resources.assets",
        vec![
            object(
                2,
                ClassId::TEXTURE_2D,
                texture("packed-texture", true, 1, 2, &Pixels::Inline(&PIXELS)),
            ),
            object(11, ClassId::SPRITE, sprite_in("packed", 0, 12, false)),
            object(12, ClassId::SPRITE_ATLAS, atlas_placing("packer", 11, 2)),
        ],
        &[],
    );
}

/// A sprite packed into an atlas names no texture of its own; the atlas's entry for the sprite
/// holds the packed texture and the sprite's place in it.
#[test]
fn plans_a_packed_sprite_from_its_atlas_entry() {
    let dir = tempfile::tempdir().unwrap();
    write_packed_player(dir.path());
    let plan = plan(dir.path()).unwrap();
    let sprite = plan
        .assets
        .iter()
        .find_map(|asset| match asset {
            ResourceAsset::Sprite {
                name,
                texture,
                texture_rect,
                texture_rect_offset,
                packing_rotation,
                ..
            } if name == "packed" => Some((
                texture.as_ref().map(|reference| reference.asset),
                *texture_rect,
                *texture_rect_offset,
                *packing_rotation,
            )),
            _ => None,
        })
        .expect("the packed sprite is planned");
    let texture = sprite
        .0
        .expect("the atlas's texture is the sprite's texture");
    assert!(
        matches!(&plan.assets[texture], ResourceAsset::Texture2D { name, .. } if name == "packed-texture"),
        "the sprite refers to the atlas's texture: {:?}",
        plan.assets[texture]
    );
    assert_eq!(
        (sprite.1, sprite.2),
        ([6.0, 7.0, 8.0, 9.0], [0.25, 0.75]),
        "its place and offset in the atlas texture"
    );
    assert_eq!(sprite.3, 2, "the atlas entry's packing settings");
}
