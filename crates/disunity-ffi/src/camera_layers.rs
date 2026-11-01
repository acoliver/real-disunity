use crate::{
    DisunityResult,
    callbacks::mutate,
    physics::{component, create},
};
use worldrunner_world_simulation::ComponentDesc;

fn description(kind: u32, enabled: bool) -> Option<ComponentDesc> {
    match kind {
        114 => Some(ComponentDesc::GlobalLight2d(
            worldrunner_world_simulation::presentation::GlobalLight2d {
                enabled,
                color: [1.; 4],
                intensity: 1.,
                sorting_layers: vec![0],
                ..Default::default()
            },
        )),
        115 => Some(ComponentDesc::UniversalAdditionalCameraData(
            Default::default(),
        )),
        81 => Some(ComponentDesc::AudioListener { enabled }),
        92 => Some(ComponentDesc::GUILayer { enabled }),
        95 => Some(ComponentDesc::Animator { enabled }),
        124 => Some(ComponentDesc::FlareLayer { enabled }),
        198 => Some(ComponentDesc::ParticleSystem),
        210 => Some(ComponentDesc::SortingGroup(Default::default())),
        _ => None,
    }
}
/// # Safety
/// Output is writable for one component identity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_camera_layer_create(
    world: u64,
    object: u64,
    kind: u32,
    output: *mut u64,
) -> DisunityResult {
    let Some(desc) = description(kind, true) else {
        return DisunityResult::invalid_argument();
    };
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    match create(world, object, desc) {
        Ok(id) => {
            unsafe {
                *output = id;
            }
            DisunityResult::ok()
        }
        Err(e) => crate::abi::translate_error(Some(world), &e),
    }
}
/// # Safety
/// Output is writable for one enabled flag.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_camera_layer_read(
    world: u64,
    handle: u64,
    output: *mut u32,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let id = component(s, handle)?;
        let enabled = match s.native.component(id)?.desc {
            ComponentDesc::GlobalLight2d(ref light) => light.enabled,
            ComponentDesc::UniversalAdditionalCameraData(ref data) => data.enabled,
            ComponentDesc::AudioListener { enabled }
            | ComponentDesc::GUILayer { enabled }
            | ComponentDesc::FlareLayer { enabled }
            | ComponentDesc::Animator { enabled } => enabled,
            _ => return Err("camera-side component required".into()),
        };
        unsafe {
            *output = u32::from(enabled);
        }
        Ok(())
    })
}
/// Writes native enabled state for a camera-side component.
#[unsafe(no_mangle)]
pub extern "C" fn disunity_camera_layer_write(
    world: u64,
    handle: u64,
    enabled: u32,
) -> DisunityResult {
    if enabled > 1 {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let id = component(s, handle)?;
        let desc = match s.native.component(id)?.desc.clone() {
            ComponentDesc::GlobalLight2d(ref light) => {
                let mut light = light.clone();
                light.enabled = enabled == 1;
                ComponentDesc::GlobalLight2d(light)
            }
            ComponentDesc::UniversalAdditionalCameraData(mut data) => {
                data.enabled = enabled == 1;
                ComponentDesc::UniversalAdditionalCameraData(data)
            }
            ComponentDesc::AudioListener { .. } => ComponentDesc::AudioListener {
                enabled: enabled == 1,
            },
            ComponentDesc::GUILayer { .. } => ComponentDesc::GUILayer {
                enabled: enabled == 1,
            },
            ComponentDesc::FlareLayer { .. } => ComponentDesc::FlareLayer {
                enabled: enabled == 1,
            },
            ComponentDesc::Animator { .. } => ComponentDesc::Animator {
                enabled: enabled == 1,
            },
            _ => return Err("camera-side component required".into()),
        };
        s.native.set_component(id, desc)?;
        Ok(())
    })
}
/// # Safety
/// Output is writable for one active listener count.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_audio_listener_count(
    world: u64,
    output: *mut u32,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let count = u32::try_from(s.native.active_audio_listeners())
            .map_err(|_| "listener count overflow")?;
        unsafe {
            *output = count;
        }
        Ok(())
    })
}

/// # Safety
/// Output is writable for one floating point light property.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_light2d_read(
    world: u64,
    handle: u64,
    property: u32,
    output: *mut f32,
) -> DisunityResult {
    if output.is_null() || property > 4 {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let ComponentDesc::GlobalLight2d(ref light) =
            s.native.component(component(s, handle)?)?.desc
        else {
            return Err("Global Light2D required".into());
        };
        unsafe {
            *output = if property == 4 {
                light.intensity
            } else {
                light.color[property as usize]
            };
        }
        Ok(())
    })
}
/// Writes one native global light property, validating the complete component before publication.
#[unsafe(no_mangle)]
pub extern "C" fn disunity_light2d_write(
    world: u64,
    handle: u64,
    property: u32,
    value: f32,
) -> DisunityResult {
    if property > 4 {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let id = component(s, handle)?;
        let ComponentDesc::GlobalLight2d(mut light) = s.native.component(id)?.desc.clone() else {
            return Err("Global Light2D required".into());
        };
        if property == 4 {
            light.intensity = value;
        } else {
            light.color[property as usize] = value;
        }
        s.native
            .set_component(id, ComponentDesc::GlobalLight2d(light))?;
        Ok(())
    })
}
