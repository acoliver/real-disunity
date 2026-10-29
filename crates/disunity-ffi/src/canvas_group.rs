//! Canvas group component settings. The rules live in world-simulation; this file converts records.
use crate::{DisunityResult, callbacks::mutate, errors::AbiError, physics::component};
use worldrunner_world_simulation::{
    ComponentDesc, ComponentHandle,
    ui::{CanvasGroup, CanvasGroupFlags},
};

/// A canvas group's settings; booleans are 0 or 1. A write brings `alpha` into 0 through 1 and
/// rejects a NaN alpha or a boolean above 1, leaving the group unchanged.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct DisunityCanvasGroup {
    pub enabled: u32,
    pub alpha: f32,
    pub interactable: u32,
    pub blocks_raycasts: u32,
    pub ignore_parent_groups: u32,
}

impl Default for DisunityCanvasGroup {
    /// The settings of a new canvas group.
    fn default() -> Self {
        Self::from_native(CanvasGroup::default())
    }
}

impl DisunityCanvasGroup {
    fn from_native(group: CanvasGroup) -> Self {
        Self {
            enabled: u32::from(group.enabled),
            alpha: group.alpha,
            interactable: u32::from(group.flags.interactable),
            blocks_raycasts: u32::from(group.flags.blocks_raycasts),
            ignore_parent_groups: u32::from(group.flags.ignore_parent_groups),
        }
    }

    fn to_native(self) -> Result<CanvasGroup, AbiError> {
        let flags = [
            self.enabled,
            self.interactable,
            self.blocks_raycasts,
            self.ignore_parent_groups,
        ];
        if flags.iter().any(|flag| *flag > 1) {
            return Err("canvas group boolean settings must be 0 or 1".into());
        }
        let group = CanvasGroup {
            enabled: self.enabled == 1,
            alpha: self.alpha,
            flags: CanvasGroupFlags {
                interactable: self.interactable == 1,
                blocks_raycasts: self.blocks_raycasts == 1,
                ignore_parent_groups: self.ignore_parent_groups == 1,
            },
        };
        Ok(group
            .normalized()
            .map_err(worldrunner_world_simulation::WorldError::from)?)
    }
}

fn group(
    session: &crate::store::Session,
    handle: u64,
) -> Result<(ComponentHandle, CanvasGroup), AbiError> {
    let id = component(session, handle)?;
    match session.native.component(id)?.desc {
        ComponentDesc::CanvasGroup(group) => Ok((id, group)),
        _ => Err("component is not a CanvasGroup".into()),
    }
}

/// Creates a canvas group with the engine's default settings on a live object.
/// # Safety
/// Output is writable for one component identity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_canvas_group_create(
    world: u64,
    object: u64,
    output: *mut u64,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    let desc = ComponentDesc::CanvasGroup(CanvasGroup::default());
    match crate::physics::create(world, object, desc) {
        Ok(id) => {
            unsafe { *output = id };
            DisunityResult::ok()
        }
        Err(e) => crate::abi::translate_error(Some(world), &e),
    }
}

/// # Safety
/// Output is writable for one CanvasGroup record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_canvas_group_read(
    world: u64,
    handle: u64,
    output: *mut DisunityCanvasGroup,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let (_, value) = group(s, handle)?;
        unsafe { *output = DisunityCanvasGroup::from_native(value) };
        Ok(())
    })
}

/// Validates every setting before changing the group; a rejected write changes nothing.
/// # Safety
/// Value is readable for one CanvasGroup record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_canvas_group_write(
    world: u64,
    handle: u64,
    value: *const DisunityCanvasGroup,
) -> DisunityResult {
    if value.is_null() {
        return DisunityResult::invalid_argument();
    }
    let value = unsafe { *value };
    mutate(world, |s| {
        let (id, _) = group(s, handle)?;
        s.native
            .set_component(id, ComponentDesc::CanvasGroup(value.to_native()?))?;
        Ok(())
    })
}
