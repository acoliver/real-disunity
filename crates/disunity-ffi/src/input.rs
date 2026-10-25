//! Timestamped input ingress and phase-specific native projections.
use crate::{DisunityResult, callbacks::mutate};
use worldrunner_world_simulation::{
    AxisId, InputAction, InputEvent, InputPhase, KeyCode, TimeSpan,
};

#[unsafe(no_mangle)]
pub extern "C" fn disunity_input_push(
    world: u64,
    num: u64,
    den: u64,
    kind: u32,
    code: u32,
    value: f32,
) -> DisunityResult {
    mutate(world, |session| {
        let timestamp = TimeSpan::new(num, den).map_err(crate::errors::AbiError::from)?;
        let action = match kind {
            0 if code <= u32::from(u16::MAX) && (value == 0.0 || value == 1.0) => InputAction::Key(
                KeyCode(u16::try_from(code).expect("checked key")),
                value == 1.0,
            ),
            1 if code <= u32::from(u16::MAX) => {
                InputAction::Axis(AxisId(u16::try_from(code).expect("checked axis")), value)
            }
            2 if value == 0.0 || value == 1.0 => InputAction::Focus(value == 1.0),
            _ => return Err("invalid input event".into()),
        };
        session
            .native
            .push_input(&[InputEvent { timestamp, action }])
            .map_err(crate::errors::AbiError::from)
    })
}

/// # Safety
/// `keys` and `axis` are writable for one value each.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_input_read(
    world: u64,
    phase: u32,
    code: u32,
    keys: *mut u32,
    axis: *mut f32,
) -> DisunityResult {
    if keys.is_null() || axis.is_null() || phase > 1 || code > u32::from(u16::MAX) {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |session| {
        let view = session.native.input(if phase == 1 {
            InputPhase::Fixed
        } else {
            InputPhase::Dynamic
        });
        let code = u16::try_from(code).expect("checked input code");
        let key = KeyCode(code);
        unsafe {
            *keys = u32::from(view.held.contains(&key))
                | (u32::from(view.down.contains(&key)) << 1)
                | (u32::from(view.up.contains(&key)) << 2);
            *axis = *view.axes.get(&AxisId(code)).unwrap_or(&0.0);
        }
        Ok(())
    })
}
