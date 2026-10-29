use super::{RetainedHost, RetainedUpdate, ViewerError};
use worldrunner_render_plan::SceneDecal;

/// Volume edits composite in ascending caller ID order. Initial IDs are authored indices.
#[derive(Clone, Debug)]
pub enum DecalUpdate {
    Add { id: u64, decal: SceneDecal },
    Replace { id: u64, decal: SceneDecal },
    Remove(u64),
}
impl RetainedHost {
    pub(super) fn apply_decal(&mut self, update: DecalUpdate) -> Result<(), ViewerError> {
        let mut decals = self.decals.clone();
        match &update {
            DecalUpdate::Add { id, decal } => {
                if decals.insert(*id, *decal).is_some() {
                    return Err(ViewerError::Driver("duplicate decal ID".into()));
                }
            }
            DecalUpdate::Replace { id, decal } => {
                *decals
                    .get_mut(id)
                    .ok_or_else(|| ViewerError::Driver("unknown decal ID".into()))? = *decal;
            }
            DecalUpdate::Remove(id) => {
                decals
                    .remove(id)
                    .ok_or_else(|| ViewerError::Driver("unknown decal ID".into()))?;
            }
        }
        let scene = self
            .scene
            .with_decals(decals.values().copied().collect())
            .map_err(|error| ViewerError::Driver(error.to_string()))?;
        crate::validate(&scene, &self.options)?;
        worldrunner_render_scene::draw::validate_decal_materials(&scene)?;
        self.scene = scene;
        self.decals = decals;
        self.enqueue(RetainedUpdate::Decal(update), false);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use worldrunner_render_plan::{
        MaterialHandle, SceneDecalAxis, SceneDecalReceivers, SceneRotation, SceneTransform,
    };
    fn decal() -> SceneDecal {
        SceneDecal {
            priority: 0,
            transform: SceneTransform {
                position_millimeters: [0; 3],
                rotation: SceneRotation::IDENTITY,
                scale_millionths: 1_000_000,
            },
            dimensions_millimeters: [1000; 3],
            material: MaterialHandle::from_raw(0),
            axis: SceneDecalAxis::Z,
            edge_fade_micro: 0,
            angle_cutoff_micro: 0,
            angle_fade_micro: 0,
            receivers: SceneDecalReceivers::ALL,
            mapping: worldrunner_render_plan::SceneDecalMapping::default(),
            write_mask: worldrunner_render_plan::SceneDecalWriteMask::default(),
            alpha_test_cutoff_micro: None,
            color_modifier: worldrunner_render_plan::SceneDecalColorModifier::default(),
        }
    }
    #[test]
    fn invalid_write_masks_and_cutoffs_reject_before_retained_acceptance() {
        let scene = crate::renderer::retained_tests::source();
        let mut host = RetainedHost::new(scene.clone(), crate::tests::options()).unwrap();
        for (bits, cutoff) in [(8, None), (u32::MAX, None), (7, Some(1_000_001))] {
            let mut invalid = decal();
            invalid.write_mask = worldrunner_render_plan::SceneDecalWriteMask::from_bits(bits);
            invalid.alpha_test_cutoff_micro = cutoff;
            assert!(
                host.apply(RetainedUpdate::Decal(DecalUpdate::Add {
                    id: 7,
                    decal: invalid,
                }))
                .is_err()
            );
            assert_eq!(host.scene, scene);
            assert_eq!(host.accepted_generation(), 0);
            assert_eq!(host.preparations_in_flight(), 0);
            assert!(host.readiness.queue.is_empty());
        }
        host.apply(RetainedUpdate::Decal(DecalUpdate::Add {
            id: 7,
            decal: decal(),
        }))
        .unwrap();
        let previous = host.scene.clone();
        let mut invalid = decal();
        invalid.write_mask = worldrunner_render_plan::SceneDecalWriteMask::from_bits(8);
        assert!(
            host.apply(RetainedUpdate::Decal(DecalUpdate::Replace {
                id: 7,
                decal: invalid,
            }))
            .is_err()
        );
        assert_eq!(host.scene, previous);
        assert_eq!(host.accepted_generation(), 1);
        assert_eq!(host.readiness.queue.len(), 1);
    }
    #[test]
    fn retained_decal_edits_are_ordered_atomic_without_scene_preparation() {
        let scene = crate::renderer::retained_tests::source();
        let mut host = RetainedHost::new(scene, crate::tests::options()).unwrap();
        let original = host.scene.clone();
        let edit = |edit| RetainedUpdate::Decal(edit);
        host.apply(edit(DecalUpdate::Add {
            id: 7,
            decal: decal(),
        }))
        .unwrap();
        let mut replacement = decal();
        replacement.edge_fade_micro = 100_000;
        assert!(
            host.apply(edit(DecalUpdate::Add {
                id: 7,
                decal: replacement
            }))
            .is_err()
        );
        assert!(
            host.apply(edit(DecalUpdate::Replace {
                id: 99,
                decal: replacement
            }))
            .is_err()
        );
        replacement.material = MaterialHandle::from_raw(99);
        assert!(
            host.apply(edit(DecalUpdate::Replace {
                id: 7,
                decal: replacement
            }))
            .is_err()
        );
        assert_eq!(host.accepted_generation(), 1);
        assert_eq!(host.scene.decals(), &[decal()]);
        replacement.material = MaterialHandle::from_raw(0);
        host.apply(edit(DecalUpdate::Replace {
            id: 7,
            decal: replacement,
        }))
        .unwrap();
        host.apply(edit(DecalUpdate::Remove(7))).unwrap();
        assert!(host.apply(edit(DecalUpdate::Remove(7))).is_err());
        assert_eq!(host.scene, original);
        assert_eq!(host.readiness.queue.len(), 3);
        assert!(host.readiness.queue.iter().all(|p| p.scene.is_none()));
    }
}
