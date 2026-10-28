#[path = "support/decode.rs"]
mod decode;
mod support;

use decode::decode_one;
use worldrunner_unity_serialized::{
    ClassId, SerializedError,
    font::{CharacterInfo, Font, FontSpacing, Kerning},
    object::PPtr,
    write::ObjectWriter,
};

const fn ptr(path_id: i64) -> PPtr {
    PPtr {
        file_id: 0,
        path_id,
    }
}

/// Not a real font file, and a length that needs padding.
const FONT_FILE: [u8; 5] = [0x00, 0x01, 0x00, 0x00, 0x7f];

/// The two trailing flags: legacy bounds calculation and rounding advances.
type Flags = [bool; 2];

fn character(w: &mut ObjectWriter, index: u32, base: f32, flipped: bool) {
    w.u32(index)
        .floats(&[base, base + 0.1, base + 0.2, base + 0.3]);
    w.floats(&[base + 1.0, base + 2.0, base + 3.0, base + 4.0])
        .f32(base + 5.0);
    w.u8(u8::from(flipped)).align();
}

fn font_bytes(big_endian: bool, flags: Flags) -> Vec<u8> {
    let mut w = ObjectWriter::new(big_endian);
    w.string("Body")
        .unwrap()
        .f32(1.5)
        .pptr(ptr(3))
        .f32(16.0)
        .pptr(ptr(4));
    w.i32(32).f32(0.25).i32(2).i32(1).i32(-2);
    w.i32(2);
    character(&mut w, 65, 10.0, true);
    character(&mut w, 66, 20.0, false);
    w.i32(1).u16(65).u16(86).f32(-0.75);
    w.f32(0.5).byte_array(&FONT_FILE).unwrap();
    w.f32(12.0).f32(-3.0).u32(2);
    w.i32(2)
        .string("Arial")
        .unwrap()
        .string("Noto Sans")
        .unwrap();
    w.pptrs(&[ptr(9)]).unwrap();
    w.i32(4).bytes(&flags.map(u8::from));
    w.finish()
}

fn expected_character(index: u32, base: f32, flipped: bool) -> CharacterInfo {
    CharacterInfo {
        index,
        uv: [base, base + 0.1, base + 0.2, base + 0.3],
        vert: [base + 1.0, base + 2.0, base + 3.0, base + 4.0],
        advance: base + 5.0,
        flipped,
    }
}

fn expected_font(flags: Flags) -> Font {
    let [use_legacy_bounds_calculation, should_round_advance_value] = flags;
    Font {
        name: "Body".into(),
        default_material: ptr(3),
        font_size: 16.0,
        texture: ptr(4),
        ascii_start_offset: 32,
        spacing: FontSpacing {
            line_spacing: 1.5,
            tracking: 0.25,
            character_spacing: 2,
            character_padding: 1,
        },
        convert_case: -2,
        characters: vec![
            expected_character(65, 10.0, true),
            expected_character(66, 20.0, false),
        ],
        kerning: vec![Kerning {
            first: 65,
            second: 86,
            adjustment: -0.75,
        }],
        pixel_scale: 0.5,
        // Checked against the written bytes separately.
        font_data: 0..0,
        ascent: 12.0,
        descent: -3.0,
        default_style: 2,
        font_names: vec!["Arial".into(), "Noto Sans".into()],
        fallback_fonts: vec![ptr(9)],
        rendering_mode: 4,
        use_legacy_bounds_calculation,
        should_round_advance_value,
    }
}

#[test]
fn decodes_fonts_with_every_list_filled_in_both_byte_orders() {
    for big_endian in [false, true] {
        for flags in [[true, false], [false, true]] {
            let bytes = font_bytes(big_endian, flags);
            let mut decoded: Font = decode_one(ClassId::FONT, bytes.clone(), big_endian).unwrap();
            assert_eq!(bytes[decoded.font_data.clone()], FONT_FILE);
            decoded.font_data = 0..0;
            assert_eq!(decoded, expected_font(flags), "flags {flags:?}");
        }
    }
}

#[test]
fn every_truncated_font_fails_and_trailing_bytes_are_rejected() {
    let bytes = font_bytes(false, [true, true]);
    for end in 0..bytes.len() {
        let error = decode_one::<Font>(ClassId::FONT, bytes[..end].to_vec(), false).unwrap_err();
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
    let error = decode_one::<Font>(ClassId::FONT, longer, false).unwrap_err();
    assert!(
        matches!(error, SerializedError::TrailingBytes { .. }),
        "{error}"
    );
}
