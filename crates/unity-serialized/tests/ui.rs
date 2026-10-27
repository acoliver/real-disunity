#[path = "support/decode.rs"]
mod decode;
mod support;

use decode::decode_one;
use worldrunner_unity_serialized::{
    ClassId, SerializedError,
    object::{Layout, PPtr},
    text_asset::TextAsset,
    ui::{Canvas, CanvasGroup, CanvasRenderer, CanvasSorting, GroupInput, PixelPerfect},
    write::ObjectWriter,
};

const fn ptr(path_id: i64) -> PPtr {
    PPtr {
        file_id: 0,
        path_id,
    }
}

/// The four consecutive canvas flags: pixel perfect, receives events, override sorting and
/// override pixel perfect. One-hot values make a swapped pair fail.
type CanvasFlags = [bool; 4];

/// The canvas flags apart from the four above: enabled and vertex color gamma space. The
/// tests use opposite values so a swap or a constant fails.
type OuterFlags = [bool; 2];

const OUTER: [OuterFlags; 2] = [[true, false], [false, true]];

fn canvas_bytes(big_endian: bool, flags: CanvasFlags, outer: OuterFlags) -> Vec<u8> {
    let [enabled, gamma] = outer.map(u8::from);
    let mut w = ObjectWriter::new(big_endian);
    w.pptr(ptr(4))
        .u8(enabled)
        .align()
        .i32(2)
        .pptr(ptr(9))
        .f32(100.0);
    w.bytes(&flags.map(u8::from)).f32(0.25).u8(gamma).align();
    w.i32(25).i32(1).i32(-7).i16(-3).bytes(&[0xff]);
    w.finish()
}

fn expected_canvas(flags: CanvasFlags, outer: OuterFlags) -> Canvas {
    let [pixel, receives_events, override_sorting, override_pixel] = flags;
    let [enabled, vertex_color_always_gamma_space] = outer;
    Canvas {
        game_object: ptr(4),
        enabled,
        render_mode: 2,
        camera: ptr(9),
        plane_distance: 100.0,
        pixel_perfect: PixelPerfect {
            enabled: pixel,
            override_parent: override_pixel,
        },
        receives_events,
        sorting: CanvasSorting {
            override_parent: override_sorting,
            bucket_normalized_size: 0.25,
            layer_id: -7,
            order: -3,
        },
        vertex_color_always_gamma_space,
        additional_shader_channels: 25,
        update_rect_transform_for_standalone: 1,
        target_display: -1,
    }
}

fn one_hot<const N: usize>(index: usize) -> [bool; N] {
    std::array::from_fn(|i| i == index)
}

#[test]
fn decodes_canvases_with_each_flag_alone_in_both_byte_orders() {
    for big_endian in [false, true] {
        for (flags, outer) in (0..5).map(one_hot::<4>).flat_map(|f| OUTER.map(|o| (f, o))) {
            let bytes = canvas_bytes(big_endian, flags, outer);
            let decoded: Canvas = decode_one(ClassId::CANVAS, bytes, big_endian).unwrap();
            assert_eq!(
                decoded,
                expected_canvas(flags, outer),
                "{flags:?} {outer:?}"
            );
        }
    }
}

fn group_bytes(big_endian: bool, enabled: bool, input: [bool; 3]) -> Vec<u8> {
    let mut w = ObjectWriter::new(big_endian);
    w.pptr(ptr(6)).u8(u8::from(enabled)).align().f32(0.5);
    w.bytes(&input.map(u8::from)).align();
    w.finish()
}

#[test]
fn decodes_canvas_groups_with_each_input_flag_alone() {
    for big_endian in [false, true] {
        for (input, enabled) in (0..4)
            .map(one_hot::<3>)
            .flat_map(|i| [(i, false), (i, true)])
        {
            let bytes = group_bytes(big_endian, enabled, input);
            let decoded: CanvasGroup =
                decode_one(ClassId::CANVAS_GROUP, bytes, big_endian).unwrap();
            let [interactable, blocks_raycasts, ignore_parent_groups] = input;
            let expected = CanvasGroup {
                game_object: ptr(6),
                enabled,
                alpha: 0.5,
                input: GroupInput {
                    interactable,
                    blocks_raycasts,
                    ignore_parent_groups,
                },
            };
            assert_eq!(decoded, expected, "input {input:?}");
        }
    }
}

fn renderer_bytes(big_endian: bool, cull: bool) -> Vec<u8> {
    let mut w = ObjectWriter::new(big_endian);
    w.pptr(ptr(11)).u8(u8::from(cull));
    w.finish()
}

#[test]
fn decodes_canvas_renderers_and_rejects_trailing_bytes_and_bad_flags() {
    for (big_endian, cull) in [(false, true), (true, true), (false, false), (true, false)] {
        let bytes = renderer_bytes(big_endian, cull);
        let decoded: CanvasRenderer =
            decode_one(ClassId::CANVAS_RENDERER, bytes, big_endian).unwrap();
        let expected = CanvasRenderer {
            game_object: ptr(11),
            cull_transparent_mesh: cull,
        };
        assert_eq!(decoded, expected);
    }
    let mut longer = renderer_bytes(false, true);
    longer.push(0);
    let error = decode_one::<CanvasRenderer>(ClassId::CANVAS_RENDERER, longer, false).unwrap_err();
    assert!(
        matches!(error, SerializedError::TrailingBytes { .. }),
        "{error}"
    );
    let mut flag = renderer_bytes(false, true);
    flag[12] = 2;
    let error = decode_one::<CanvasRenderer>(ClassId::CANVAS_RENDERER, flag, false).unwrap_err();
    assert_eq!(
        error,
        SerializedError::InvalidBool {
            offset: 12,
            value: 2
        }
    );
}

fn text_bytes(big_endian: bool, contents: &[u8]) -> Vec<u8> {
    let mut w = ObjectWriter::new(big_endian);
    w.string("notes").unwrap().byte_array(contents).unwrap();
    w.finish()
}

#[test]
fn text_assets_locate_contents_that_need_not_be_text() {
    // Not UTF-8, and a length that needs padding.
    let contents = [0xef, 0xbb, 0xbf, 0xff, 0x00];
    for big_endian in [false, true] {
        let bytes = text_bytes(big_endian, &contents);
        assert_eq!(bytes.len(), 24);
        let decoded: TextAsset =
            decode_one(ClassId::TEXT_ASSET, bytes.clone(), big_endian).unwrap();
        // The name takes 4 + 5 bytes padded to 12, then the 4-byte length.
        assert_eq!(
            decoded,
            TextAsset {
                name: "notes".into(),
                contents: 16..21
            }
        );
        assert_eq!(&bytes[decoded.contents], contents);
    }
    let empty: TextAsset = decode_one(ClassId::TEXT_ASSET, text_bytes(false, &[]), false).unwrap();
    assert_eq!(empty.contents, 16..16);
}

fn every_prefix_fails<T: Layout + std::fmt::Debug>(class: ClassId, bytes: &[u8]) {
    for end in 0..bytes.len() {
        let error = decode_one::<T>(class, bytes[..end].to_vec(), false).unwrap_err();
        assert!(
            matches!(
                error,
                SerializedError::Truncated { .. } | SerializedError::InvalidCount { .. }
            ),
            "{class} prefix {end}: {error}"
        );
    }
}

#[test]
fn every_truncated_ui_object_fails() {
    every_prefix_fails::<Canvas>(ClassId::CANVAS, &canvas_bytes(false, [true; 4], [true; 2]));
    every_prefix_fails::<CanvasGroup>(ClassId::CANVAS_GROUP, &group_bytes(false, true, [true; 3]));
    every_prefix_fails::<CanvasRenderer>(ClassId::CANVAS_RENDERER, &renderer_bytes(false, true));
    every_prefix_fails::<TextAsset>(ClassId::TEXT_ASSET, &text_bytes(false, &[1, 2, 3, 4, 5]));
}
