#[path = "support/decode.rs"]
mod decode;
mod support;

use decode::decode_one;
use support::{object, scene};
use worldrunner_unity_serialized::{
    ClassId, SerializedError, SerializedFile,
    classes::{GameObject, MonoBehaviour, MonoScript, RectLayout, RectTransform, Transform},
    object::{PPtr, decode, object_bytes},
    write::ObjectWriter,
};

const fn local(path_id: i64) -> PPtr {
    PPtr {
        file_id: 0,
        path_id,
    }
}

fn game_object_bytes(big_endian: bool, name: &str) -> Vec<u8> {
    let mut w = ObjectWriter::new(big_endian);
    w.pptrs(&[
        local(2),
        PPtr {
            file_id: 1,
            path_id: -5,
        },
    ])
    .unwrap();
    w.u32(5).string(name).unwrap().u16(3).u8(1);
    w.finish()
}

#[test]
fn decodes_game_objects_in_both_byte_orders() {
    for big_endian in [false, true] {
        let decoded: GameObject = decode_one(
            ClassId::GAME_OBJECT,
            game_object_bytes(big_endian, "Canvas"),
            big_endian,
        )
        .unwrap();
        let expected = GameObject {
            components: vec![
                local(2),
                PPtr {
                    file_id: 1,
                    path_id: -5,
                },
            ],
            layer: 5,
            name: "Canvas".into(),
            tag: 3,
            active: true,
        };
        assert_eq!(decoded, expected);
    }
}

fn transform_bytes(rect: bool) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.pptr(local(1))
        .floats(&[0.0, 0.0, 0.0, 1.0])
        .floats(&[1.0, 2.0, 3.0]);
    w.floats(&[1.0, 1.0, 1.0])
        .align()
        .pptrs(&[local(9)])
        .unwrap()
        .pptr(local(0));
    if rect {
        w.floats(&[0.0, 0.0, 1.0, 1.0, 4.0, 5.0, 100.0, 50.0, 0.5, 0.5]);
    }
    w.finish()
}

#[test]
fn decodes_transforms_and_rect_transforms() {
    let transform: Transform =
        decode_one(ClassId::TRANSFORM, transform_bytes(false), false).unwrap();
    assert_eq!(transform.game_object, local(1));
    assert_eq!(
        transform.local_rotation.map(f32::to_bits),
        [0.0_f32, 0.0, 0.0, 1.0].map(f32::to_bits)
    );
    assert_eq!(
        transform.local_position.map(f32::to_bits),
        [1.0_f32, 2.0, 3.0].map(f32::to_bits)
    );
    assert_eq!(transform.children, [local(9)]);
    assert!(transform.father.is_null());
    assert_eq!(transform.rect, None);

    let RectTransform(rect) =
        decode_one(ClassId::RECT_TRANSFORM, transform_bytes(true), false).unwrap();
    let layout = RectLayout {
        anchor_min: [0.0, 0.0],
        anchor_max: [1.0, 1.0],
        anchored_position: [4.0, 5.0],
        size_delta: [100.0, 50.0],
        pivot: [0.5, 0.5],
    };
    assert_eq!(rect.rect, Some(layout));
    assert_eq!(rect.children, transform.children);
}

#[test]
fn decodes_scripts_and_script_components() {
    let mut w = ObjectWriter::new(false);
    w.string("Spinner").unwrap().i32(-100).bytes(&[7; 16]);
    w.string("Spinner")
        .unwrap()
        .string("Game.Toys")
        .unwrap()
        .string("Assembly-CSharp")
        .unwrap();
    let script: MonoScript = decode_one(ClassId::MONO_SCRIPT, w.finish(), false).unwrap();
    assert_eq!(script.execution_order, -100);
    assert_eq!(script.properties_hash, [7; 16]);
    assert_eq!(
        (script.namespace.as_str(), script.assembly.as_str()),
        ("Game.Toys", "Assembly-CSharp")
    );

    let mut w = ObjectWriter::new(false);
    w.pptr(local(3)).u8(1).align().pptr(PPtr {
        file_id: 1,
        path_id: 2134,
    });
    w.string("").unwrap().f32(2.5).u32(9);
    let behaviour: MonoBehaviour = decode_one(ClassId::MONO_BEHAVIOUR, w.finish(), false).unwrap();
    assert!(behaviour.enabled);
    assert_eq!(
        behaviour.script,
        PPtr {
            file_id: 1,
            path_id: 2134
        }
    );
    assert_eq!(behaviour.script_data, 32..40);
}

#[test]
fn fails_when_the_layout_does_not_consume_the_object_exactly() {
    let mut bytes = game_object_bytes(false, "Go");
    bytes.push(0);
    let error = decode_one::<GameObject>(ClassId::GAME_OBJECT, bytes.clone(), false).unwrap_err();
    assert_eq!(
        error,
        SerializedError::TrailingBytes {
            path_id: 7,
            class: ClassId::GAME_OBJECT,
            read: 43,
            size: 44
        }
    );
    bytes.truncate(bytes.len() - 2);
    let error = decode_one::<GameObject>(ClassId::GAME_OBJECT, bytes, false).unwrap_err();
    assert!(
        matches!(
            error,
            SerializedError::Truncated {
                offset: 42,
                needed: 1,
                ..
            }
        ),
        "{error}"
    );
}

#[test]
fn fails_on_the_wrong_class_and_malformed_fields() {
    let error =
        decode_one::<Transform>(ClassId::GAME_OBJECT, game_object_bytes(false, "Go"), false)
            .unwrap_err();
    assert_eq!(
        error,
        SerializedError::WrongClass {
            path_id: 7,
            expected: ClassId::TRANSFORM,
            found: ClassId::GAME_OBJECT
        }
    );
    let mut bytes = game_object_bytes(false, "Go");
    *bytes.last_mut().unwrap() = 2;
    let error = decode_one::<GameObject>(ClassId::GAME_OBJECT, bytes, false).unwrap_err();
    assert_eq!(
        error,
        SerializedError::InvalidBool {
            offset: 42,
            value: 2
        }
    );
    let mut bytes = game_object_bytes(false, "Go");
    bytes[36] = 0xff;
    let error = decode_one::<GameObject>(ClassId::GAME_OBJECT, bytes, false).unwrap_err();
    assert_eq!(error, SerializedError::InvalidString { offset: 36 });
}

#[test]
fn rejects_counts_longer_than_the_object() {
    let mut w = ObjectWriter::new(false);
    w.i32(1_000_000).u32(0);
    let error = decode_one::<GameObject>(ClassId::GAME_OBJECT, w.finish(), false).unwrap_err();
    assert!(
        matches!(
            error,
            SerializedError::InvalidCount {
                what: "component",
                count: 1_000_000,
                ..
            }
        ),
        "{error}"
    );
}

#[test]
fn decodes_an_independently_written_game_object_and_string_padding() {
    // Synthetic bytes, independent of ObjectWriter: no components, layer 5, name X, tag 3.
    let bytes = vec![0, 0, 0, 0, 5, 0, 0, 0, 1, 0, 0, 0, b'X', 0, 0, 0, 3, 0, 1];
    let value: GameObject = decode_one(ClassId::GAME_OBJECT, bytes, false).unwrap();
    assert_eq!(
        (value.name.as_str(), value.layer, value.tag, value.active),
        ("X", 5, 3, true)
    );
    for text in ["", "a", "ab", "abc", "abcd", "abcde", "日本語"] {
        for endian in [false, true] {
            let value: GameObject = decode_one(
                ClassId::GAME_OBJECT,
                game_object_bytes(endian, text),
                endian,
            )
            .unwrap();
            assert_eq!(value.name, text);
        }
    }
}

#[test]
fn reports_supplied_object_size_mismatches_in_both_directions() {
    let bytes = game_object_bytes(false, "Go");
    let mut contents = scene(22);
    contents.objects = vec![object(7, 0, bytes.clone())];
    let file_bytes = contents.write().unwrap();
    let file = SerializedFile::parse(&file_bytes).unwrap();
    let info = file.object(7).unwrap();
    for actual in [bytes.len() - 1, bytes.len() + 1] {
        let error = decode::<GameObject>(&vec![0; actual], info, false).unwrap_err();
        assert_eq!(
            error,
            SerializedError::ObjectSizeMismatch {
                path_id: 7,
                expected: info.size,
                actual
            }
        );
    }
    let mut outside = *info;
    outside.offset = u64::MAX;
    assert!(matches!(
        object_bytes(&file_bytes, &outside),
        Err(SerializedError::Truncated { .. })
    ));
}

#[test]
fn rejects_negative_or_oversized_string_lengths_and_each_truncated_transform() {
    for count in [-1_i32, 1_000_000] {
        let mut bytes = game_object_bytes(false, "Go");
        bytes[32..36].copy_from_slice(&count.to_le_bytes());
        let error = decode_one::<GameObject>(ClassId::GAME_OBJECT, bytes, false).unwrap_err();
        assert!(
            matches!(error, SerializedError::InvalidCount { .. }),
            "{error}"
        );
    }
    for (class, bytes) in [
        (ClassId::TRANSFORM, transform_bytes(false)),
        (ClassId::RECT_TRANSFORM, transform_bytes(true)),
    ] {
        for end in 0..bytes.len() {
            let prefix = bytes[..end].to_vec();
            let error = if class == ClassId::TRANSFORM {
                decode_one::<Transform>(class, prefix, false).unwrap_err()
            } else {
                decode_one::<RectTransform>(class, prefix, false).unwrap_err()
            };
            assert!(
                matches!(
                    error,
                    SerializedError::Truncated { .. } | SerializedError::InvalidCount { .. }
                ),
                "{end}: {error}"
            );
        }
    }
}
