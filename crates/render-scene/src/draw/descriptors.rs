//! Capability admission is checked before any upload or allocation.
use crate::frame::FrameError;
use worldrunner_render_plan::{RenderScene, SceneMaterialFeatures};

pub(crate) fn validate(scene: &RenderScene) -> Result<(), FrameError> {
    for material in scene.resources().materials() {
        if let Some(features) = material.features() {
            validate_material(features)?;
        }
    }
    if let Some(terrain) = scene.terrain() {
        for surface in terrain.surfaces() {
            let material = &scene.resources().materials()[surface.material().raw() as usize];
            if let Some(features) = material.features() {
                validate_terrain_material(features)?;
            }
            if material.translucency_micro() != 0 || material.terrain_contact().is_some() {
                return Err(FrameError::UnsupportedEffect(
                    "terrain material surface features",
                ));
            }
        }
    }
    for material in scene.resources().materials() {
        if material.terrain_contact().is_some() && scene.terrain().is_none() {
            return Err(FrameError::UnsupportedEffect(
                "terrain contact requires a heightfield",
            ));
        }
    }
    Ok(())
}

fn validate_material(features: &SceneMaterialFeatures) -> Result<(), FrameError> {
    if features.layer.is_some() {
        return Err(FrameError::UnsupportedEffect("material layer blending"));
    }
    Ok(())
}

fn validate_terrain_material(features: &SceneMaterialFeatures) -> Result<(), FrameError> {
    use worldrunner_render_plan::{SceneAmbientOcclusion, SceneTint};
    if features.tint != SceneTint::WHITE
        || features.ambient_occlusion != SceneAmbientOcclusion::UNOCCLUDED
        || features.wind.is_some()
    {
        return Err(FrameError::UnsupportedEffect(
            "terrain material geometry features",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mesh_features_remain_rejected_for_terrain_materials() {
        assert!(validate_terrain_material(&neutral()).is_ok());
        let features = SceneMaterialFeatures {
            tint: SceneTint::new([0; 3]).unwrap(),
            ..neutral()
        };
        assert!(validate_material(&features).is_ok());
        assert!(matches!(
            validate_terrain_material(&features),
            Err(FrameError::UnsupportedEffect(
                "terrain material geometry features"
            ))
        ));
    }

    use worldrunner_render_plan::{
        MaterialHandle, SceneAmbientOcclusion, SceneLayerBlend, SceneLayerMode, SceneTint,
    };
    fn neutral() -> SceneMaterialFeatures {
        SceneMaterialFeatures {
            tint: SceneTint::WHITE,
            ambient_occlusion: SceneAmbientOcclusion::UNOCCLUDED,
            wind: None,
            layer: None,
        }
    }
    #[test]
    fn unsupported_material_requests_fail_before_upload() {
        assert!(validate_material(&neutral()).is_ok());
        let features = SceneMaterialFeatures {
            layer: Some(
                SceneLayerBlend::new(MaterialHandle::from_raw(0), 0, SceneLayerMode::Linear)
                    .unwrap(),
            ),
            ..neutral()
        };
        assert!(matches!(
            validate_material(&features),
            Err(FrameError::UnsupportedEffect("material layer blending"))
        ));
    }
}
