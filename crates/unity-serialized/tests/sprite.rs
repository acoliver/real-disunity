#[path = "support/decode.rs"]
mod decode;
mod support;

use decode::decode_one;
use worldrunner_unity_serialized::{
    ClassId, SerializedError,
    mesh::{Aabb, ChannelInfo, SubMesh, VertexData},
    object::PPtr,
    sprite::{
        RenderDataKey, SecondaryTexture, Sprite, SpriteAtlas, SpriteAtlasData, SpriteBone,
        SpriteRenderData,
    },
    write::ObjectWriter,
};

const fn ptr(path_id: i64) -> PPtr {
    PPtr {
        file_id: 0,
        path_id,
    }
}

const INDICES: [u8; 6] = [0, 0, 1, 0, 2, 0];
const VERTICES: [u8; 7] = [10, 11, 12, 13, 14, 15, 16];
const BINDPOSE: [f32; 16] = [
    0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0,
];

fn count(w: &mut ObjectWriter, n: i32) -> &mut ObjectWriter {
    w.i32(n)
}

/// Writes the render data, with every array holding at least one entry.
fn render_data(w: &mut ObjectWriter) {
    w.pptr(ptr(20)).pptr(ptr(21));
    count(w, 1)
        .pptr(ptr(22))
        .string("_NormalMap")
        .unwrap()
        .align();
    count(w, 1).u32(12).u32(6).i32(2).u32(1).u32(5).u32(3);
    w.floats(&[0.5, 0.25, 0.0, 1.5, 2.5, 0.0]).align();
    w.byte_array(&INDICES).unwrap();
    w.u32(3);
    count(w, 1).bytes(&[1, 2, 3, 4]).align();
    w.byte_array(&VERTICES).unwrap().align();
    count(w, 1).floats(&BINDPOSE).align();
    w.floats(&[1.0, 2.0, 30.0, 40.0])
        .floats(&[0.5, 0.75])
        .floats(&[3.0, 4.0]);
    w.u32(0x45).floats(&[0.1, 0.2, 0.3, 0.4]).f32(1.25);
}

fn sprite_bytes(big_endian: bool, polygon: bool) -> Vec<u8> {
    let mut w = ObjectWriter::new(big_endian);
    w.string("leaf").unwrap();
    w.floats(&[1.0, 2.0, 32.0, 48.0]).floats(&[-0.5, 0.5]);
    w.floats(&[4.0, 5.0, 6.0, 7.0])
        .f32(100.0)
        .floats(&[0.5, 0.25]);
    w.u32(2).u8(u8::from(polygon)).align();
    w.u32(0x1111)
        .u32(0x2222)
        .u32(0x3333)
        .u32(0x4444)
        .i64(-21_300_000);
    count(&mut w, 2)
        .string("ui")
        .unwrap()
        .string("map")
        .unwrap()
        .align();
    w.pptr(ptr(30));
    render_data(&mut w);
    count(&mut w, 2);
    count(&mut w, 1).floats(&[0.0, 1.0]).align();
    count(&mut w, 2).floats(&[2.0, 3.0, 4.0, 5.0]).align();
    w.align();
    count(&mut w, 1)
        .string("root")
        .unwrap()
        .string("guid-1")
        .unwrap();
    w.floats(&[1.0, 2.0, 3.0])
        .floats(&[0.0, 0.0, 0.0, 1.0])
        .f32(9.0)
        .i32(-1)
        .u32(0xff00_ff00);
    w.align();
    w.pptrs(&[ptr(40)]).unwrap();
    w.finish()
}

fn expected_render_data() -> SpriteRenderData {
    SpriteRenderData {
        texture: ptr(20),
        alpha_texture: ptr(21),
        secondary_textures: vec![SecondaryTexture {
            texture: ptr(22),
            name: "_NormalMap".into(),
        }],
        sub_meshes: vec![SubMesh {
            first_byte: 12,
            index_count: 6,
            topology: 2,
            base_vertex: 1,
            first_vertex: 5,
            vertex_count: 3,
            local_aabb: Aabb {
                center: [0.5, 0.25, 0.0],
                extent: [1.5, 2.5, 0.0],
            },
        }],
        // Ranges are checked against the written bytes separately.
        index_buffer: 0..0,
        vertex_data: VertexData {
            vertex_count: 3,
            channels: vec![ChannelInfo {
                stream: 1,
                offset: 2,
                format: 3,
                dimension: 4,
            }],
            data: 0..0,
        },
        bindposes: vec![BINDPOSE],
        texture_rect: [1.0, 2.0, 30.0, 40.0],
        texture_rect_offset: [0.5, 0.75],
        atlas_rect_offset: [3.0, 4.0],
        settings: 0x45,
        uv_transform: [0.1, 0.2, 0.3, 0.4],
        downscale_multiplier: 1.25,
    }
}

fn expected_sprite(polygon: bool) -> Sprite {
    Sprite {
        name: "leaf".into(),
        rect: [1.0, 2.0, 32.0, 48.0],
        offset: [-0.5, 0.5],
        border: [4.0, 5.0, 6.0, 7.0],
        pixels_to_units: 100.0,
        pivot: [0.5, 0.25],
        extrude: 2,
        is_polygon: polygon,
        render_data_key: RenderDataKey {
            guid: [0x1111, 0x2222, 0x3333, 0x4444],
            local_id: -21_300_000,
        },
        atlas_tags: vec!["ui".into(), "map".into()],
        sprite_atlas: ptr(30),
        render_data: expected_render_data(),
        physics_shape: vec![vec![[0.0, 1.0]], vec![[2.0, 3.0], [4.0, 5.0]]],
        bones: vec![SpriteBone {
            name: "root".into(),
            guid: "guid-1".into(),
            position: [1.0, 2.0, 3.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            length: 9.0,
            parent: -1,
            color: 0xff00_ff00,
        }],
        scriptable_objects: vec![ptr(40)],
    }
}

#[test]
fn decodes_sprites_with_every_array_filled_in_both_byte_orders() {
    for big_endian in [false, true] {
        for polygon in [false, true] {
            let bytes = sprite_bytes(big_endian, polygon);
            let mut decoded: Sprite =
                decode_one(ClassId::SPRITE, bytes.clone(), big_endian).unwrap();
            let rd = &mut decoded.render_data;
            assert_eq!(bytes[rd.index_buffer.clone()], INDICES);
            assert_eq!(bytes[rd.vertex_data.data.clone()], VERTICES);
            rd.index_buffer = 0..0;
            rd.vertex_data.data = 0..0;
            assert_eq!(decoded, expected_sprite(polygon), "polygon {polygon}");
        }
    }
}

#[test]
fn every_truncated_sprite_fails() {
    let bytes = sprite_bytes(false, true);
    for end in 0..bytes.len() {
        let error =
            decode_one::<Sprite>(ClassId::SPRITE, bytes[..end].to_vec(), false).unwrap_err();
        assert!(
            matches!(
                error,
                SerializedError::Truncated { .. } | SerializedError::InvalidCount { .. }
            ),
            "prefix {end}: {error}"
        );
    }
}

#[test]
fn rejects_trailing_bytes() {
    let mut bytes = sprite_bytes(false, false);
    bytes.extend([0; 4]);
    let error = decode_one::<Sprite>(ClassId::SPRITE, bytes, false).unwrap_err();
    assert!(
        matches!(error, SerializedError::TrailingBytes { .. }),
        "{error}"
    );
}

fn atlas_bytes(big_endian: bool, variant: bool) -> Vec<u8> {
    let mut w = ObjectWriter::new(big_endian);
    w.string("icons")
        .unwrap()
        .pptrs(&[ptr(50), ptr(51)])
        .unwrap();
    count(&mut w, 2)
        .string("gold")
        .unwrap()
        .string("iron")
        .unwrap();
    count(&mut w, 1).u32(5).u32(6).u32(7).u32(8).i64(-99);
    w.pptr(ptr(60)).pptr(ptr(61)).floats(&[1.0, 2.0, 3.0, 4.0]);
    w.floats(&[5.0, 6.0])
        .floats(&[-1.0, -2.0])
        .floats(&[100.0, 9.0, 101.0, 10.0]);
    w.f32(0.5).u32(0x41);
    count(&mut w, 1).pptr(ptr(62)).string("_MaskTex").unwrap();
    w.string("ui-tag").unwrap().u8(u8::from(variant)).align();
    w.finish()
}

fn expected_atlas(variant: bool) -> SpriteAtlas {
    SpriteAtlas {
        name: "icons".into(),
        packed_sprites: vec![ptr(50), ptr(51)],
        packed_sprite_names: vec!["gold".into(), "iron".into()],
        render_data: vec![(
            RenderDataKey {
                guid: [5, 6, 7, 8],
                local_id: -99,
            },
            SpriteAtlasData {
                texture: ptr(60),
                alpha_texture: ptr(61),
                texture_rect: [1.0, 2.0, 3.0, 4.0],
                texture_rect_offset: [5.0, 6.0],
                atlas_rect_offset: [-1.0, -2.0],
                uv_transform: [100.0, 9.0, 101.0, 10.0],
                downscale_multiplier: 0.5,
                settings: 0x41,
                secondary_textures: vec![SecondaryTexture {
                    texture: ptr(62),
                    name: "_MaskTex".into(),
                }],
            },
        )],
        tag: "ui-tag".into(),
        is_variant: variant,
    }
}

#[test]
fn decodes_sprite_atlases_in_both_byte_orders() {
    for big_endian in [false, true] {
        for variant in [false, true] {
            let bytes = atlas_bytes(big_endian, variant);
            let decoded: SpriteAtlas =
                decode_one(ClassId::SPRITE_ATLAS, bytes, big_endian).unwrap();
            assert_eq!(decoded, expected_atlas(variant), "variant {variant}");
        }
    }
}

#[test]
fn every_truncated_sprite_atlas_fails() {
    let bytes = atlas_bytes(false, true);
    for end in 0..bytes.len() {
        let error = decode_one::<SpriteAtlas>(ClassId::SPRITE_ATLAS, bytes[..end].to_vec(), false)
            .unwrap_err();
        assert!(
            matches!(
                error,
                SerializedError::Truncated { .. } | SerializedError::InvalidCount { .. }
            ),
            "prefix {end}: {error}"
        );
    }
}
