//! UI draws whose material is a game's own. The player draws them through the game's shader into
//! the layer atlas; this counts them by shader for the run report and names, once, a material
//! whose run cannot be drawn.
use std::collections::{BTreeMap, BTreeSet};
use worldrunner_world_simulation::graphics::MaterialId;
use worldrunner_world_simulation::ui::presentation::{UiLayers, UiMaterial, UiPresentation};

/// The shader name a draw is counted under when the host sent no name for its material.
const UNNAMED: &str = "unnamed";

#[derive(Default)]
pub(crate) struct GameMaterials {
    named: BTreeSet<MaterialId>,
    /// The shader name of each material the last frame drew.
    shaders: BTreeMap<UiMaterial, String>,
    /// The draws of the last frame, by shader name.
    by_shader: BTreeMap<String, usize>,
}

impl GameMaterials {
    /// Counts the draws of `plan` that name a material by shader, and keeps the shader names the
    /// host sent, which are not part of the plan the player compares.
    pub(crate) fn count(&mut self, plan: &mut UiPresentation) {
        self.by_shader.clear();
        self.shaders = std::mem::take(&mut plan.shaders);
        for material in plan.draws.iter().filter_map(|draw| draw.material) {
            let shader = self.shaders.get(&material).map_or(UNNAMED, String::as_str);
            *self.by_shader.entry(shader.to_owned()).or_default() += 1;
        }
    }

    /// Reports each material whose run does not fit the layer atlas, the first time it is left out.
    pub(crate) fn name_too_large(
        &mut self,
        plan: &UiPresentation,
        layers: &UiLayers,
        mut report: impl FnMut(String),
    ) {
        for range in &layers.too_large {
            for material in plan.draws[range.clone()].iter().filter_map(|d| d.material) {
                if self.named.insert(material.id) {
                    let shader = self.shaders.get(&material).map_or(UNNAMED, String::as_str);
                    report(material.refusal(shader, "its run does not fit the layer atlas"));
                }
            }
        }
    }

    /// The draws of the last frame that name a game material, by shader name.
    pub(crate) fn by_shader(&self) -> &BTreeMap<String, usize> {
        &self.by_shader
    }
}
