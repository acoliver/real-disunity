#[path = "support/decode.rs"]
mod decode;
mod support;

use decode::decode_one;
use worldrunner_unity_serialized::{
    ClassId, SerializedError,
    managers::{InputAxisSettings, InputManager},
    write::ObjectWriter,
};

/// Writes one axis in the order the Old World player stores them.
fn write_axis(w: &mut ObjectWriter, axis: &InputAxisSettings) {
    w.string(&axis.name).unwrap();
    w.string("descriptive").unwrap();
    w.string("descriptive negative").unwrap();
    w.string(&axis.negative_button).unwrap();
    w.string(&axis.positive_button).unwrap();
    w.string(&axis.alt_negative_button).unwrap();
    w.string(&axis.alt_positive_button).unwrap();
    w.f32(axis.gravity).f32(axis.dead).f32(axis.sensitivity);
    w.u8(u8::from(axis.snap)).u8(u8::from(axis.invert)).align();
    w.i32(axis.kind).i32(axis.axis).i32(axis.joystick);
}

fn horizontal() -> InputAxisSettings {
    InputAxisSettings {
        name: "Horizontal".into(),
        negative_button: "left".into(),
        positive_button: "right".into(),
        alt_negative_button: "a".into(),
        alt_positive_button: "d".into(),
        gravity: 3.0,
        dead: 0.001,
        sensitivity: 3.0,
        snap: true,
        invert: false,
        kind: 0,
        axis: 0,
        joystick: 0,
    }
}

fn mouse_y() -> InputAxisSettings {
    InputAxisSettings {
        name: "Mouse Y".into(),
        negative_button: String::new(),
        positive_button: String::new(),
        alt_negative_button: String::new(),
        alt_positive_button: String::new(),
        gravity: 0.0,
        dead: 0.0,
        sensitivity: 0.1,
        snap: false,
        invert: true,
        kind: 1,
        axis: 1,
        joystick: 2,
    }
}

fn bytes(big_endian: bool, axes: &[InputAxisSettings], physical_keys: bool) -> Vec<u8> {
    let mut w = ObjectWriter::new(big_endian);
    w.i32(i32::try_from(axes.len()).unwrap());
    for axis in axes {
        write_axis(&mut w, axis);
    }
    w.u8(u8::from(physical_keys)).align();
    w.finish()
}

#[test]
fn decodes_every_field_of_axes_of_different_kinds() {
    for big_endian in [false, true] {
        let axes = [horizontal(), mouse_y()];
        let data = bytes(big_endian, &axes, true);
        let decoded: InputManager = decode_one(ClassId::INPUT_MANAGER, data, big_endian).unwrap();
        assert_eq!(decoded.axes, axes);
        assert!(decoded.use_physical_keys);
    }
}

#[test]
fn a_manager_without_axes_decodes_to_none() {
    let decoded: InputManager =
        decode_one(ClassId::INPUT_MANAGER, bytes(false, &[], false), false).unwrap();
    assert!(decoded.axes.is_empty());
}

#[test]
fn the_object_must_be_consumed_exactly() {
    let whole = bytes(false, &[horizontal(), mouse_y()], false);
    for end in 0..whole.len() {
        let error =
            decode_one::<InputManager>(ClassId::INPUT_MANAGER, whole[..end].to_vec(), false)
                .unwrap_err();
        assert!(
            matches!(
                error,
                SerializedError::Truncated { .. } | SerializedError::InvalidCount { .. }
            ),
            "prefix {end}: {error}"
        );
    }
    let mut longer = whole;
    longer.push(0);
    let error = decode_one::<InputManager>(ClassId::INPUT_MANAGER, longer, false).unwrap_err();
    assert!(
        matches!(error, SerializedError::TrailingBytes { .. }),
        "{error}"
    );
}
