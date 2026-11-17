use super::{MeshSelection, Vec3};
use crate::instances::fade::{Distances, Fade};
use crate::instances::slot_bounds::Slot;
use worldrunner_render_plan::MaterialHandle;

impl MeshSelection {
    /// Whether every fragment a placement drawn with `material` and the level inputs of `slot`
    /// could draw at `lod`, including the preceding level's handoff, is faded out. `fades` is
    /// the material table.
    pub(super) fn fade_empty(
        &self,
        fades: &[Fade],
        material: MaterialHandle,
        slot: &Slot<'_>,
        lod: usize,
        eye: Vec3,
    ) -> bool {
        // Materials of one placement share its distance per fade reference.
        let mut distances = Distances::new(eye, slot.fade.origin, slot.references, slot.anchors);
        let band = slot.fade.band;
        self.level_fade_empty(fades, material, band, lod, &mut distances)
            && (!self.transitions[lod]
                || self.level_fade_empty(fades, material, band, lod - 1, &mut distances))
    }

    fn level_fade_empty(
        &self,
        fades: &[Fade],
        material: MaterialHandle,
        band: Option<[f32; 2]>,
        lod: usize,
        distances: &mut Distances<'_>,
    ) -> bool {
        let fade = |material: MaterialHandle| fades[material.raw() as usize];
        if let Some(slots) = &self.material_slots[lod] {
            // One material that keeps fragments at every distance keeps the level, so the
            // others need not measure.
            slots.iter().all(|slot| fade(slot.material()).can_empty())
                && slots
                    .iter()
                    .all(|slot| fade(slot.material()).empty(band, distances))
        } else {
            fade(material).empty(band, distances)
        }
    }
}

#[cfg(test)]
impl MeshSelection {
    /// `fade_empty` for one placement whose inputs are computed on the spot.
    pub(super) fn fade_empty_alone(
        &self,
        fades: &[Fade],
        object: worldrunner_render_plan::SceneObject,
        lod: usize,
        eye: Vec3,
    ) -> bool {
        use crate::instances::fade::Anchor;
        let mut references = Vec::new();
        for reference in fades.iter().filter_map(|fade| fade.reference()) {
            if !references.contains(&reference) {
                references.push(reference);
            }
        }
        let anchors: Vec<_> = references
            .iter()
            .map(|&reference| Anchor::referenced(object.transform(), reference))
            .collect();
        let slot = Slot {
            ceilings: &[],
            fade: crate::instances::slot_bounds::SlotFade {
                band: object.fade().map(crate::geometry::fade_band_meters),
                origin: crate::geometry::millimeters_to_meters(
                    object.transform().position_millimeters,
                )
                .to_array(),
            },
            references: &references,
            anchors: &anchors,
        };
        self.fade_empty(fades, object.material(), &slot, lod, eye)
    }
}
