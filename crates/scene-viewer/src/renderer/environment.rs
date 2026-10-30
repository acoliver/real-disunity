use super::{GpuContext, Presentation, Resident};
use crate::ViewerError;
use worldrunner_render_plan::SceneEffects;

pub(super) fn install(
    context: &GpuContext,
    resident: &mut Resident,
    presentation: &mut Presentation,
    effects: Option<&SceneEffects>,
    reset: bool,
) -> Result<(), ViewerError> {
    if reset {
        presentation.scheduler.clear_environment(context.device());
    }
    let Some(environment) = effects.and_then(SceneEffects::environment) else {
        return Ok(());
    };
    let settings = presentation.scheduler.configuration();
    if environment.consumers.water() && settings.water.is_empty()
        || environment.consumers.reflections() && !settings.ssr.is_some_and(|s| s.enabled)
    {
        return Err(ViewerError::UnsupportedScene(
            "environment consumer is not enabled",
        ));
    }
    let maps = resident.scene_mut().environment_maps(environment)?;
    presentation
        .scheduler
        .set_scene_environment(context.device(), &maps, environment.consumers)?;
    Ok(())
}
