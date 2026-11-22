//! UI draws whose material is a game's own, counted by the shader's name for the run report.
use std::collections::BTreeMap;
use worldrunner_world_simulation::ui::presentation::UiPresentation;

/// The shader name a draw is counted under when the host sent no name for its material.
const UNNAMED: &str = "unnamed";

#[derive(Default)]
pub(crate) struct GameMaterials {
    /// The draws the last frame named a material for, by shader name.
    by_shader: BTreeMap<String, usize>,
}

impl GameMaterials {
    /// Counts the draws of `plan` that name a material, by the name of the material's shader.
    pub(crate) fn count(&mut self, plan: &UiPresentation) {
        self.by_shader.clear();
        for material in plan.draws.iter().filter_map(|draw| draw.material) {
            let shader = plan.shaders.get(&material).map_or(UNNAMED, String::as_str);
            *self.by_shader.entry(shader.to_owned()).or_default() += 1;
        }
    }

    /// The draws the last frame named a material for, by shader name.
    pub(crate) fn by_shader(&self) -> &BTreeMap<String, usize> {
        &self.by_shader
    }
}
