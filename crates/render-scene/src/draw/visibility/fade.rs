use super::{Instances, MeshSelection, Vec3};
use worldrunner_render_plan::SceneObject;

impl MeshSelection {
    pub(super) fn fade_empty(
        &self,
        instances: &Instances,
        object: SceneObject,
        lod: usize,
        eye: Vec3,
    ) -> bool {
        #[cfg(test)]
        if instances.fade_cull_disabled {
            return false;
        }
        self.level_fade_empty(instances, object, lod, eye)
            && (!self.transitions[lod] || self.level_fade_empty(instances, object, lod - 1, eye))
    }

    fn level_fade_empty(
        &self,
        instances: &Instances,
        object: SceneObject,
        lod: usize,
        eye: Vec3,
    ) -> bool {
        if let Some(slots) = &self.material_slots[lod] {
            slots
                .iter()
                .all(|slot| instances.fades[slot.material().raw() as usize].empty(object, eye))
        } else {
            instances.fades[object.material().raw() as usize].empty(object, eye)
        }
    }
}
