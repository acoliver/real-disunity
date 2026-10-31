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

pub(super) fn new_resident(
    context: &GpuContext,
    scene: &worldrunner_render_plan::RenderScene,
    shadows: Option<worldrunner_render_scene::shadows::DirectionalShadowSettings>,
    profile: crate::FrameProfile,
    presentation: &mut Presentation,
) -> Result<Resident, ViewerError> {
    let reservation = presentation.prepare_scene(context, scene)?;
    let mut resident = Resident::new(context, scene, shadows, profile)?;
    resident
        .scene_mut()
        .retain_resource_reservation(reservation.resident);
    install(context, &mut resident, presentation, scene.effects(), false)?;
    drop(reservation.transient);
    Ok(resident)
}
