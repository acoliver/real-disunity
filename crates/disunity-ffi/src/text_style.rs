//! Text styling, the settings Unity's `Text` serializes besides its string and size.
use crate::{DisunityResult, callbacks::mutate, physics::component};
use worldrunner_world_simulation::{
    ComponentDesc, ComponentError, ComponentHandle, FontId, WorldError,
    ui::{HorizontalAlign, HorizontalOverflow, TextRun, VerticalAlign, VerticalOverflow},
};

/// A text's style. `alignment` is Unity's `TextAnchor` (0 upper left to 8 lower right),
/// `horizontal_overflow` its `HorizontalWrapMode` (0 Wrap, 1 Overflow), `vertical_overflow` its
/// `VerticalWrapMode` (0 Truncate, 1 Overflow) and `font_style` its `FontStyle`. Only the normal
/// font style without best fit is supported; `font` is a handle from `disunity_font_register`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunityTextStyle {
    pub font: u64,
    pub alignment: u32,
    pub horizontal_overflow: u32,
    pub vertical_overflow: u32,
    pub font_style: u32,
    pub rich_text: u32,
    pub best_fit: u32,
    pub line_spacing: f32,
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

fn unsupported(field: &'static str) -> crate::errors::AbiError {
    WorldError::InvalidComponent(ComponentError::UnsupportedField(field)).into()
}

fn flag(value: u32, field: &'static str) -> Result<bool, crate::errors::AbiError> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(format!("{field} must be 0 or 1").into()),
    }
}

fn alignment(anchor: u32) -> Result<(HorizontalAlign, VerticalAlign), crate::errors::AbiError> {
    let horizontal = [
        HorizontalAlign::Left,
        HorizontalAlign::Center,
        HorizontalAlign::Right,
    ];
    let vertical = [
        VerticalAlign::Top,
        VerticalAlign::Middle,
        VerticalAlign::Bottom,
    ];
    let index = |n: u32| usize::try_from(n).unwrap_or(usize::MAX);
    match (
        horizontal.get(index(anchor % 3)),
        vertical.get(index(anchor / 3)),
    ) {
        (Some(&h), Some(&v)) => Ok((h, v)),
        _ => Err("unknown text alignment".into()),
    }
}

fn anchor(horizontal: HorizontalAlign, vertical: VerticalAlign) -> u32 {
    let column = match horizontal {
        HorizontalAlign::Left => 0,
        HorizontalAlign::Center => 1,
        HorizontalAlign::Right => 2,
    };
    let row = match vertical {
        VerticalAlign::Top => 0,
        VerticalAlign::Middle => 1,
        VerticalAlign::Bottom => 2,
    };
    row * 3 + column
}

/// Applies `style` to `run`, rejecting the whole style if any field is unsupported or invalid.
fn apply(
    style: DisunityTextStyle,
    font: FontId,
    mut run: TextRun,
) -> Result<TextRun, crate::errors::AbiError> {
    if style.font_style != 0 {
        return Err(unsupported("fontStyle"));
    }
    if flag(style.best_fit, "best fit")? {
        return Err(unsupported("resizeTextForBestFit"));
    }
    (run.horizontal, run.vertical) = alignment(style.alignment)?;
    run.horizontal_overflow = match style.horizontal_overflow {
        0 => HorizontalOverflow::Wrap,
        1 => HorizontalOverflow::Overflow,
        _ => return Err("unknown horizontal overflow".into()),
    };
    run.vertical_overflow = match style.vertical_overflow {
        0 => VerticalOverflow::Truncate,
        1 => VerticalOverflow::Overflow,
        _ => return Err("unknown vertical overflow".into()),
    };
    run.rich_text = flag(style.rich_text, "rich text")?;
    run.line_spacing = style.line_spacing;
    run.color = [style.r, style.g, style.b, style.a];
    run.font = font;
    Ok(run)
}

fn describe(run: &TextRun, font: u64) -> DisunityTextStyle {
    let [r, g, b, a] = run.color;
    DisunityTextStyle {
        font,
        alignment: anchor(run.horizontal, run.vertical),
        horizontal_overflow: u32::from(run.horizontal_overflow == HorizontalOverflow::Overflow),
        vertical_overflow: u32::from(run.vertical_overflow == VerticalOverflow::Overflow),
        font_style: 0,
        rich_text: u32::from(run.rich_text),
        best_fit: 0,
        line_spacing: run.line_spacing,
        r,
        g,
        b,
        a,
    }
}

fn text(
    session: &crate::store::Session,
    handle: u64,
) -> Result<(ComponentHandle, TextRun), crate::errors::AbiError> {
    let id = component(session, handle)?;
    match &session.native.component(id)?.desc {
        ComponentDesc::Text(run) => Ok((id, run.clone())),
        _ => Err("component is not text".into()),
    }
}

/// # Safety
/// Output is writable for one text style record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_text_style_read(
    world: u64,
    handle: u64,
    output: *mut DisunityTextStyle,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let (_, run) = text(s, handle)?;
        let font = s
            .font_ids
            .iter()
            .find_map(|(&handle, &id)| (id == run.font).then_some(handle))
            .ok_or("text font binding missing")?;
        unsafe { *output = describe(&run, font) };
        Ok(())
    })
}

/// Validates every field before changing the text. Bold or italic styles and best fit are
/// reported as unsupported fields.
/// # Safety
/// Value is readable for one text style record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_text_style_write(
    world: u64,
    handle: u64,
    value: *const DisunityTextStyle,
) -> DisunityResult {
    if value.is_null() {
        return DisunityResult::invalid_argument();
    }
    let style = unsafe { *value };
    mutate(world, |s| {
        let (id, run) = text(s, handle)?;
        let font = *s
            .font_ids
            .get(&style.font)
            .ok_or("font is not registered in this world")?;
        s.native
            .set_component(id, ComponentDesc::Text(apply(style, font, run)?))?;
        Ok(())
    })
}
