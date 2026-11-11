//! The lights of the last prepared frame. `disunity_presentation_prepare` takes them with the rest
//! of the frame, so a host reads one consistent set however the world changes afterwards.
use crate::{
    DisunityResult, callbacks::mutate, light::DisunityLight, presentation::DisunityMatrix,
};
use worldrunner_world_simulation::presentation::Light;

/// Writes how many lights the last prepared frame shows. Fails when no frame is prepared.
///
/// # Safety
/// `count` is writable for one count.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_presentation_light_count(
    world: u64,
    count: *mut u32,
) -> DisunityResult {
    if count.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        if s.presentation.is_none() {
            return Err("presentation is not prepared".into());
        }
        let lights = u32::try_from(s.lights.len()).map_err(|_| "light count overflow")?;
        unsafe {
            *count = lights;
        }
        Ok(())
    })
}

/// Copies out light `index` of the last prepared frame: its settings (`enabled` is always 1, a
/// frame shows only enabled lights), its object's identity (0 for an object the ABI has not
/// named) and its matrix from the light's coordinates to the frame's view basis, whose third
/// column is the light's forward axis. Nothing is written when the index is past the frame.
///
/// # Safety
/// `output`, `object` and `matrix` are each writable for one record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_presentation_light(
    world: u64,
    index: u32,
    output: *mut DisunityLight,
    object: *mut u64,
    matrix: *mut DisunityMatrix,
) -> DisunityResult {
    if output.is_null() || object.is_null() || matrix.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        if s.presentation.is_none() {
            return Err("presentation is not prepared".into());
        }
        let view = s
            .lights
            .get(index as usize)
            .ok_or("presentation light index out of range")?;
        let light = Light {
            enabled: true,
            settings: view.settings,
        };
        unsafe {
            *output = DisunityLight::from_native(light);
            *object = s.objects.id_of(view.object).unwrap_or(0);
            *matrix = DisunityMatrix::from(view.local_to_world);
        }
        Ok(())
    })
}

#[cfg(test)]
#[path = "presentation_lights_tests.rs"]
mod tests;
