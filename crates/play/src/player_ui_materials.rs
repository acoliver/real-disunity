//! UI draws whose material is a game's own. The player draws them through the game's shader into
//! the layer atlas; this counts them by shader for the run report and names, once, a material
//! whose run cannot be drawn.
use std::collections::{BTreeMap, BTreeSet};
use worldrunner_world_simulation::graphics::MaterialId;
use worldrunner_world_simulation::ui::{
    UiTexture,
    presentation::{UiLayers, UiMaterial, UiPresentation},
};

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
    /// Takes the shader names the host sent out of `plan`, which are not part of the plan the
    /// player compares, and removes the draws a game's shader cannot draw yet: those that read a
    /// render texture. Each such material is named once. The draws left that name a material are
    /// counted by shader for the run report. Calling it again on its own result changes nothing.
    pub(crate) fn admit(&mut self, plan: &mut UiPresentation, mut report: impl FnMut(String)) {
        if !plan.shaders.is_empty() {
            self.shaders = std::mem::take(&mut plan.shaders);
        }
        plan.draws.retain(|draw| {
            let Some(material) = draw.material else {
                return true;
            };
            if !matches!(draw.texture, Some(UiTexture::Target(_))) {
                return true;
            }
            if self.named.insert(material.id) {
                let shader = self.shaders.get(&material).map_or(UNNAMED, String::as_str);
                report(material.refusal(shader, "its image is a render texture"));
            }
            false
        });
        self.by_shader.clear();
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
        for refused in &layers.too_large {
            let [width, height] = refused.size;
            let reason = format!(
                "its run of {width}x{height} pixels does not fit the layer atlas, which grows to at most {} pixels a side",
                layers.max_side
            );
            for material in plan.draws[refused.draws.clone()]
                .iter()
                .filter_map(|d| d.material)
            {
                if self.named.insert(material.id) {
                    let shader = self.shaders.get(&material).map_or(UNNAMED, String::as_str);
                    report(material.refusal(shader, &reason));
                }
            }
        }
    }

    /// The draws of the last frame that name a game material, by shader name.
    pub(crate) fn by_shader(&self) -> &BTreeMap<String, usize> {
        &self.by_shader
    }
}
