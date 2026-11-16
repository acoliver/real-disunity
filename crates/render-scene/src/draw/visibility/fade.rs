use super::{MeshSelection, Vec3};
use crate::instances::fade::{Distances, Fade};
use worldrunner_render_plan::SceneObject;

impl MeshSelection {
    /// Whether every fragment the placement could draw at `lod`, including the preceding
    /// level's handoff, is faded out. `fades` is the material table.
    pub(super) fn fade_empty(
        &self,
        fades: &[Fade],
        object: SceneObject,
        lod: usize,
        eye: Vec3,
    ) -> bool {
        // Materials of one placement share its distance per fade reference.
        let mut distances = Distances::default();
        self.level_fade_empty(fades, object, lod, eye, &mut distances)
            && (!self.transitions[lod]
                || self.level_fade_empty(fades, object, lod - 1, eye, &mut distances))
    }

    fn level_fade_empty(
        &self,
        fades: &[Fade],
        object: SceneObject,
        lod: usize,
        eye: Vec3,
        distances: &mut Distances,
    ) -> bool {
        if let Some(slots) = &self.material_slots[lod] {
            slots
                .iter()
                .all(|slot| fades[slot.material().raw() as usize].empty(object, eye, distances))
        } else {
            fades[object.material().raw() as usize].empty(object, eye, distances)
        }
    }
}
