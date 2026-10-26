use crate::{DisunityResult, callbacks::mutate, physics::component};
use worldrunner_world_simulation::ComponentDesc;
/// # Safety
/// Output is writable for one scalar camera setting.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_camera_data_read(
    world: u64,
    handle: u64,
    property: u32,
    output: *mut u32,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let ComponentDesc::UniversalAdditionalCameraData(ref data) =
            s.native.component(component(s, handle)?)?.desc
        else {
            return Err("UniversalAdditionalCameraData required".into());
        };
        let value = data.get(property).ok_or("unknown camera setting")?;
        unsafe {
            *output = value;
        }
        Ok(())
    })
}
/// Validates the named camera setting before publishing any state.
#[unsafe(no_mangle)]
pub extern "C" fn disunity_camera_data_write(
    world: u64,
    handle: u64,
    property: u32,
    value: u32,
) -> DisunityResult {
    mutate(world, |s| {
        let id = component(s, handle)?;
        let ComponentDesc::UniversalAdditionalCameraData(mut data) =
            s.native.component(id)?.desc.clone()
        else {
            return Err("UniversalAdditionalCameraData required".into());
        };
        data.set(property, value)?;
        s.native
            .set_component(id, ComponentDesc::UniversalAdditionalCameraData(data))?;
        Ok(())
    })
}
/// # Safety
/// Layers is readable for count signed sorting-layer IDs, or null when count is zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_light2d_layers(
    world: u64,
    handle: u64,
    layers: *const i32,
    count: u32,
) -> DisunityResult {
    if count > 32768 || (count > 0 && layers.is_null()) {
        return DisunityResult::invalid_argument();
    }
    let layers = if count == 0 {
        vec![]
    } else {
        unsafe { std::slice::from_raw_parts(layers, count as usize) }.to_vec()
    };
    mutate(world, |s| {
        let id = component(s, handle)?;
        let ComponentDesc::GlobalLight2d(mut light) = s.native.component(id)?.desc.clone() else {
            return Err("Global Light2D required".into());
        };
        light.sorting_layers = layers;
        s.native
            .set_component(id, ComponentDesc::GlobalLight2d(light))?;
        Ok(())
    })
}

/// Sets all four channels atomically after native component validation.
#[unsafe(no_mangle)]
pub extern "C" fn disunity_light2d_color(
    world: u64,
    handle: u64,
    r: f32,
    g: f32,
    b: f32,
    a: f32,
) -> DisunityResult {
    mutate(world, |s| {
        let id = component(s, handle)?;
        let ComponentDesc::GlobalLight2d(mut light) = s.native.component(id)?.desc.clone() else {
            return Err("Global Light2D required".into());
        };
        light.color = [r, g, b, a];
        s.native
            .set_component(id, ComponentDesc::GlobalLight2d(light))?;
        Ok(())
    })
}
/// # Safety
/// Written is writable. Output is writable for capacity layer IDs, or null for a size query.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_light2d_layers_read(
    world: u64,
    handle: u64,
    output: *mut i32,
    capacity: u32,
    written: *mut u32,
) -> DisunityResult {
    if written.is_null() || (capacity > 0 && output.is_null()) {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let ComponentDesc::GlobalLight2d(ref light) =
            s.native.component(component(s, handle)?)?.desc
        else {
            return Err("Global Light2D required".into());
        };
        unsafe {
            *written = light.sorting_layers.len() as u32;
        }
        if capacity == 0 {
            return Ok(());
        }
        if (capacity as usize) < light.sorting_layers.len() {
            return Err("sorting layer capacity is too small".into());
        }
        unsafe {
            std::ptr::copy_nonoverlapping(
                light.sorting_layers.as_ptr(),
                output,
                light.sorting_layers.len(),
            );
        }
        Ok(())
    })
}
