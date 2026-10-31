//! Sorting group settings. The rules live in world-simulation; this file converts the record.
use crate::{DisunityResult, callbacks::mutate, errors::AbiError, physics::component};
use worldrunner_world_simulation::{ComponentDesc, ComponentHandle, SortingGroup};

/// A sorting group's settings; booleans are 0 or 1. A write rejects a boolean above 1 and
/// changes nothing. Sorting layer 0 is the only layer the engine registers.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunitySortingGroup {
    pub enabled: u32,
    pub sorting_layer: i32,
    pub sorting_order: i32,
    pub sort_at_root: u32,
}

impl DisunitySortingGroup {
    fn from_native(group: SortingGroup) -> Self {
        Self {
            enabled: u32::from(group.enabled),
            sorting_layer: group.sorting_layer,
            sorting_order: group.sorting_order,
            sort_at_root: u32::from(group.sort_at_root),
        }
    }

    fn to_native(self) -> Result<SortingGroup, AbiError> {
        if self.enabled > 1 || self.sort_at_root > 1 {
            return Err("sorting group boolean settings must be 0 or 1".into());
        }
        Ok(SortingGroup {
            enabled: self.enabled == 1,
            sorting_layer: self.sorting_layer,
            sorting_order: self.sorting_order,
            sort_at_root: self.sort_at_root == 1,
        })
    }
}

fn group(
    session: &crate::store::Session,
    handle: u64,
) -> Result<(ComponentHandle, SortingGroup), AbiError> {
    let id = component(session, handle)?;
    match session.native.component(id)?.desc {
        ComponentDesc::SortingGroup(group) => Ok((id, group)),
        _ => Err("component is not a SortingGroup".into()),
    }
}

/// # Safety
/// Output is writable for one SortingGroup record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_sorting_group_read(
    world: u64,
    handle: u64,
    output: *mut DisunitySortingGroup,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let (_, value) = group(s, handle)?;
        unsafe { *output = DisunitySortingGroup::from_native(value) };
        Ok(())
    })
}

/// Validates every setting before changing the group; a rejected write changes nothing.
/// # Safety
/// Value is readable for one SortingGroup record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_sorting_group_write(
    world: u64,
    handle: u64,
    value: *const DisunitySortingGroup,
) -> DisunityResult {
    if value.is_null() {
        return DisunityResult::invalid_argument();
    }
    let value = unsafe { *value };
    mutate(world, |s| {
        let (id, _) = group(s, handle)?;
        s.native
            .set_component(id, ComponentDesc::SortingGroup(value.to_native()?))?;
        Ok(())
    })
}
