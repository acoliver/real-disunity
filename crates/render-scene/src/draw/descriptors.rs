//! Capability admission is checked before any upload or allocation.
use crate::frame::FrameError;
use worldrunner_render_plan::{
    RenderScene, SceneAmbientOcclusion, SceneMaterialFeatures, SceneTint,
};

pub(crate) fn validate(scene: &RenderScene) -> Result<(), FrameError> {
    for material in scene.resources().materials() {
        if let Some(features) = material.features() {
            validate_material(features)?;
        }
    }
    for object in scene.geometry().objects() {
        if object.tint() != SceneTint::WHITE {
            return Err(FrameError::UnsupportedEffect("instance tint"));
        }
    }
    for mesh in scene.geometry().meshes() {
        for lod in mesh.lods() {
            if lod.card().is_some() {
                return Err(FrameError::UnsupportedEffect(
                    "card dimensions and transitions",
                ));
            }
        }
    }
    Ok(())
}

fn validate_material(features: &SceneMaterialFeatures) -> Result<(), FrameError> {
    for (requested, name) in [
        (features.tint != SceneTint::WHITE, "material tint"),
        (
            features.ambient_occlusion != SceneAmbientOcclusion::UNOCCLUDED,
            "material occlusion",
        ),
        (features.wind.is_some(), "wind deformation"),
        (features.layer.is_some(), "material layer blending"),
    ] {
        if requested {
            return Err(FrameError::UnsupportedEffect(name));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use worldrunner_render_plan::{
        MaterialHandle, SceneDirection, SceneLayerBlend, SceneLayerMode, SceneWind,
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
        for features in [
            SceneMaterialFeatures {
                tint: SceneTint::new([0; 3]).unwrap(),
                ..neutral()
            },
            SceneMaterialFeatures {
                ambient_occlusion: SceneAmbientOcclusion::new(1_000_000, true).unwrap(),
                ..neutral()
            },
            SceneMaterialFeatures {
                wind: Some(
                    SceneWind::new(SceneDirection::new([0, 0, 1_000_000]).unwrap(), 1, 1000, 0)
                        .unwrap(),
                ),
                ..neutral()
            },
            SceneMaterialFeatures {
                layer: Some(
                    SceneLayerBlend::new(MaterialHandle::from_raw(0), 0, SceneLayerMode::Linear)
                        .unwrap(),
                ),
                ..neutral()
            },
        ] {
            assert!(matches!(
                validate_material(&features),
                Err(FrameError::UnsupportedEffect(_))
            ));
        }
    }
}
