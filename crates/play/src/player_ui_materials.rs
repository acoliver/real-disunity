//! UI draws whose material is a game's own. No path draws a game's shader yet, so the player
//! leaves each such draw out of the overlay and names its material once.
use std::collections::{BTreeMap, BTreeSet};
use worldrunner_world_simulation::graphics::MaterialId;
use worldrunner_world_simulation::ui::presentation::UiPresentation;

/// The shader name a draw is counted under when the host sent no name for its material.
const UNNAMED: &str = "unnamed";

#[derive(Default)]
pub(crate) struct GameMaterials {
    named: BTreeSet<MaterialId>,
    /// The draws the last frame left out, by shader name.
    by_shader: BTreeMap<String, usize>,
}

impl GameMaterials {
    /// Removes the draws that name a material from `plan`, reports each material the first time
    /// it is left out, and counts the removed draws by shader for the run report.
    pub(crate) fn leave_out(&mut self, plan: &mut UiPresentation, mut report: impl FnMut(String)) {
        self.by_shader.clear();
        let shaders = std::mem::take(&mut plan.shaders);
        plan.draws.retain(|draw| {
            let Some(material) = draw.material else {
                return true;
            };
            let shader = shaders.get(&material).map_or(UNNAMED, String::as_str);
            *self.by_shader.entry(shader.to_owned()).or_default() += 1;
            if self.named.insert(material.id) {
                report(material.refusal(shader));
            }
            false
        });
    }

    /// The draws the last frame left out for a game material, by shader name.
    pub(crate) fn by_shader(&self) -> &BTreeMap<String, usize> {
        &self.by_shader
    }
}
