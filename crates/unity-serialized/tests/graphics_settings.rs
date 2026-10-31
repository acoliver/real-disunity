#[path = "support/decode.rs"]
mod decode;
mod support;

use decode::decode_one;
use worldrunner_unity_serialized::{
    ClassId, SerializedError,
    graphics_settings::GraphicsSettings,
    object::{Layout, PPtr},
    write::ObjectWriter,
};

const fn external(path_id: i64) -> PPtr {
    PPtr {
        file_id: 1,
        path_id,
    }
}

const NONE: PPtr = PPtr {
    file_id: 0,
    path_id: 0,
};

/// A `GraphicsSettings` as the Old World player stores it (908 bytes), with the fields the
/// decoder skips filled with a marker so a wrong skip length misreads the references after it.
fn graphics_bytes(big_endian: bool, pipeline: PPtr, global_settings: PPtr) -> Vec<u8> {
    let mut w = ObjectWriter::new(big_endian);
    for mode in 0..7 {
        w.i32(mode).pptr(external(10 + i64::from(mode)));
    }
    w.i32(2);
    let shaders: Vec<PPtr> = (0..48).map(|i| external(100 + i)).collect();
    w.pptrs(&shaders).unwrap();
    w.pptrs(&[]).unwrap();
    w.i32(-1);
    w.pptr(external(2));
    w.pptr(pipeline);
    w.i32(0).floats(&[0.0, 0.0, 1.0]).i32(1).i32(1);
    w.bytes(&[0xAB; 132]);
    w.pptr(global_settings);
    w.bytes(&[0; 12]);
    w.finish()
}

fn decode_graphics(bytes: Vec<u8>, big_endian: bool) -> Result<GraphicsSettings, SerializedError> {
    decode_one(ClassId::GRAPHICS_SETTINGS, bytes, big_endian)
}

#[test]
fn decodes_the_pipeline_and_the_global_settings() {
    for big_endian in [false, true] {
        let bytes = graphics_bytes(big_endian, external(4988), external(4990));
        assert_eq!(
            bytes.len(),
            908,
            "the size of the Old World player's object"
        );
        assert_eq!(
            decode_graphics(bytes, big_endian).unwrap(),
            GraphicsSettings {
                always_included_shaders: (0..48).map(|i| external(100 + i)).collect(),
                render_pipeline: external(4988),
                global_settings: external(4990),
            }
        );
    }
}

#[test]
fn a_player_without_a_pipeline_has_null_references() {
    let settings = decode_graphics(graphics_bytes(false, NONE, NONE), false).unwrap();
    assert!(settings.render_pipeline.is_null());
    assert!(settings.global_settings.is_null());
}

#[test]
fn the_pipeline_and_the_global_settings_are_told_apart() {
    let settings = decode_graphics(graphics_bytes(false, external(1), NONE), false).unwrap();
    assert_eq!(settings.render_pipeline, external(1));
    assert!(settings.global_settings.is_null());
    let settings = decode_graphics(graphics_bytes(false, NONE, external(2)), false).unwrap();
    assert!(settings.render_pipeline.is_null());
    assert_eq!(settings.global_settings, external(2));
}

#[test]
fn the_object_must_be_filled_exactly() {
    let bytes = graphics_bytes(false, external(4988), external(4990));
    for end in 0..bytes.len() {
        let error = decode_graphics(bytes[..end].to_vec(), false).unwrap_err();
        assert!(
            matches!(
                error,
                SerializedError::Truncated { .. } | SerializedError::InvalidCount { .. }
            ),
            "prefix {end}: {error}"
        );
    }
    let mut longer = bytes;
    longer.push(0);
    assert!(decode_graphics(longer, false).is_err());
}

#[test]
fn the_layout_belongs_to_graphics_settings() {
    assert_eq!(GraphicsSettings::CLASS, ClassId::GRAPHICS_SETTINGS);
}
