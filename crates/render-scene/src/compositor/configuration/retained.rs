//! The inputs a replacement takes over from the generation it replaces.
use super::FrameRenderer;

pub(super) fn retain_water_inputs(
    next: &mut FrameRenderer,
    previous: Option<&crate::water::Water>,
) {
    if let (Some(next), Some(previous)) = (&mut next.water, previous) {
        next.environment.clone_from(&previous.environment);
        next.environment_intensity_micro = previous.environment_intensity_micro;
        if next.settings == previous.settings {
            next.surface_inputs.clone_from(&previous.surface_inputs);
        }
    }
}

pub(super) struct ReflectionInputs {
    intensity_micro: u32,
    environment: [wgpu::TextureView; 2],
    composition: worldrunner_render_lighting::reflections::Composition,
    selected: worldrunner_render_plan::SceneSsrComposition,
}
impl ReflectionInputs {
    pub(super) fn new(ssr: &crate::ssr::Ssr) -> Self {
        Self {
            environment: ssr.environment.clone(),
            intensity_micro: ssr.environment_intensity_micro,
            composition: ssr.composition,
            selected: ssr.settings.composition,
        }
    }
}
pub(super) fn retain_reflection_inputs(
    device: &wgpu::Device,
    next: &mut FrameRenderer,
    previous: Option<&ReflectionInputs>,
) {
    if let (Some(ssr), Some(screen), Some(previous)) = (&mut next.ssr, &mut next.ssao, previous) {
        ssr.environment.clone_from(&previous.environment);
        ssr.environment_intensity_micro = previous.intensity_micro;
        if ssr.settings.composition == previous.selected {
            ssr.composition = previous.composition;
        }
        let group = crate::ssr::forward::group(
            device,
            screen,
            &previous.environment,
            previous.intensity_micro,
        );
        screen
            .geometry
            .forward
            .as_mut()
            .expect("forward reflection metadata")
            .1 = group;
    }
}
