#[path = "../../world-simulation/tests/ui_font/mod.rs"]
mod test_font;

use worldrunner_disunity_ffi::{abi::*, text_style::*, ui::*};

struct Text {
    world: u64,
    handle: u64,
    font: u64,
}

fn text() -> Text {
    let (mut world, mut object, mut font, mut handle) = (0, 0, 0, 0);
    let bytes = test_font::font();
    unsafe {
        assert_eq!(disunity_world_create(&mut world).code, 0);
        assert_eq!(disunity_gameobject_create(world, &mut object).code, 0);
        let length = u32::try_from(bytes.len()).unwrap();
        assert_eq!(
            disunity_font_register(world, bytes.as_ptr(), length, &mut font).code,
            0
        );
        assert_eq!(
            disunity_ui_create(world, object, 2, font, &mut handle).code,
            0
        );
    }
    Text {
        world,
        handle,
        font,
    }
}

impl Text {
    fn read(&self) -> DisunityTextStyle {
        let mut value = DisunityTextStyle::default();
        let result = unsafe { disunity_text_style_read(self.world, self.handle, &mut value) };
        assert_eq!(result.code, 0);
        value
    }
    fn write(&self, value: DisunityTextStyle) -> DisunityResult {
        unsafe { disunity_text_style_write(self.world, self.handle, &value) }
    }
    fn set_text(&self, value: &str) -> DisunityResult {
        let length = u32::try_from(value.len()).unwrap();
        unsafe { disunity_text_write(self.world, self.handle, 20., value.as_ptr(), length) }
    }
    /// Pong's score text: middle centre, wrap, truncate, white, no rich text.
    fn pong(&self) -> DisunityTextStyle {
        DisunityTextStyle {
            font: self.font,
            alignment: 4,
            line_spacing: 1.,
            r: 1.,
            g: 1.,
            b: 1.,
            a: 1.,
            ..DisunityTextStyle::default()
        }
    }
}

impl Drop for Text {
    fn drop(&mut self) {
        unsafe { assert_eq!(disunity_world_destroy(self.world).code, 0) };
    }
}

#[test]
fn unity_defaults_and_serialized_styles_round_trip() {
    let text = text();
    let defaults = text.read();
    assert_eq!(defaults.font, text.font);
    assert_eq!(
        (
            defaults.alignment,
            defaults.horizontal_overflow,
            defaults.vertical_overflow
        ),
        (0, 0, 0),
        "upper left, wrap, truncate"
    );
    assert_eq!(defaults.rich_text, 1, "Unity enables rich text by default");
    assert_eq!(text.write(text.pong()).code, 0);
    let stored = text.read();
    assert_eq!((stored.alignment, stored.rich_text), (4, 0));
    let styled = DisunityTextStyle {
        alignment: 8,
        horizontal_overflow: 1,
        vertical_overflow: 1,
        line_spacing: 1.5,
        r: 0.25,
        ..text.pong()
    };
    assert_eq!(text.write(styled).code, 0);
    let stored = text.read();
    assert_eq!(
        (
            stored.alignment,
            stored.horizontal_overflow,
            stored.vertical_overflow
        ),
        (8, 1, 1)
    );
    assert_eq!((stored.line_spacing, stored.r), (1.5, 0.25));
}

#[test]
fn unsupported_and_invalid_styles_are_rejected_without_changing_the_text() {
    let text = text();
    assert_eq!(text.write(text.pong()).code, 0);
    let pong = text.pong();
    let typed = [
        DisunityTextStyle {
            font_style: 1,
            ..pong
        },
        DisunityTextStyle {
            font_style: 3,
            ..pong
        },
        DisunityTextStyle {
            best_fit: 1,
            ..pong
        },
    ];
    for value in typed {
        let result = text.write(value);
        assert_eq!(
            result.detail,
            DisunityErrorReason::InvalidComponent as u64,
            "{value:?}"
        );
    }
    let invalid = [
        DisunityTextStyle {
            alignment: 9,
            ..pong
        },
        DisunityTextStyle {
            horizontal_overflow: 2,
            ..pong
        },
        DisunityTextStyle {
            vertical_overflow: 2,
            ..pong
        },
        DisunityTextStyle {
            rich_text: 2,
            ..pong
        },
        DisunityTextStyle {
            line_spacing: 0.,
            ..pong
        },
        DisunityTextStyle { r: 2., ..pong },
        DisunityTextStyle {
            font: text.font + 99,
            ..pong
        },
    ];
    for value in invalid {
        assert_ne!(text.write(value).code, 0, "{value:?}");
    }
    let stored = text.read();
    assert_eq!((stored.alignment, stored.rich_text, stored.r), (4, 0, 1.));
    let result = unsafe { disunity_text_style_write(text.world, text.handle, std::ptr::null()) };
    assert_eq!(result.code, result_code::INVALID_ARGUMENT);
}

#[test]
fn markup_is_rejected_only_while_rich_text_is_on() {
    let text = text();
    assert_eq!(text.write(text.pong()).code, 0);
    assert_eq!(
        text.set_text("<b>7</b>").code,
        0,
        "markup is literal without rich text"
    );
    let rich = DisunityTextStyle {
        rich_text: 1,
        ..text.pong()
    };
    assert_ne!(
        text.write(rich).code,
        0,
        "rich text cannot render this markup"
    );
    assert_eq!(text.read().rich_text, 0);
    assert_eq!(text.set_text("7").code, 0);
    assert_eq!(text.write(rich).code, 0);
    assert_ne!(text.set_text("<b>7</b>").code, 0);
}
