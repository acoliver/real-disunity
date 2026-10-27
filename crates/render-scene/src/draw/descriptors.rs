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
            validate_terrain_normals(material)?;
            if material.impostor().is_some() {
                return Err(FrameError::UnsupportedEffect("terrain impostor"));
            }
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
fn validate_terrain_normals(
    material: &worldrunner_render_plan::SceneMaterial,
) -> Result<(), FrameError> {
    use worldrunner_render_plan::{SceneNormalEncoding, SceneNormalY};
    if material.normal_encoding() != SceneNormalEncoding::TwoChannelXy
        || material.normal_y() != SceneNormalY::Up
        || material.normal_scale_micro() != 1_000_000
    {
        return Err(FrameError::UnsupportedEffect(
            "terrain material normal settings",
        ));
    }
    Ok(())
}

fn validate_material(features: &SceneMaterialFeatures) -> Result<(), FrameError> {
    if features
        .wind
        .is_some_and(|w| w.field().is_some() && w.foliage().is_none())
    {
        return Err(FrameError::UnsupportedEffect(
            "wind field requires curved foliage",
        ));
    }
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
        || features.translucency.is_some()
    {
        return Err(FrameError::UnsupportedEffect(
            "terrain material geometry features",
        ));
    }
    Ok(())
}

pub(super) fn validate_atlases(
    scene: &RenderScene,
    textures: &[crate::texture::OwnedPreparedTexture],
) -> Result<(), FrameError> {
    for material in scene.resources().materials() {
        validate_linear(material, textures)?;
        let Some(atlas) = material.impostor() else {
            continue;
        };
        let frames = atlas.frames();
        if atlas.relief().is_some() && material.normal().is_none() {
            return Err(FrameError::UnsupportedEffect(
                "relief requires object-space normal atlas",
            ));
        }
        if let Some(handle) = atlas.borders() {
            let image = textures[handle.raw() as usize].as_prepared();
            if [image.width(), image.height()] != frames {
                return Err(FrameError::UnsupportedEffect("impostor border dimensions"));
            }
        }
        for handle in [
            Some(material.albedo()),
            material.normal(),
            material.roughness_metallic(),
            material.ambient_occlusion(),
            material.self_illumination(),
            atlas.relief().map(|v| v.0),
        ]
        .into_iter()
        .flatten()
        {
            let image = textures[handle.raw() as usize].as_prepared();
            if image.width() < frames[0]
                || image.height() < frames[1]
                || !image.width().is_multiple_of(frames[0])
                || !image.height().is_multiple_of(frames[1])
            {
                return Err(FrameError::UnsupportedEffect(
                    "impostor atlas cell dimensions",
                ));
            }
        }
    }
    Ok(())
}

fn validate_linear(
    material: &worldrunner_render_plan::SceneMaterial,
    textures: &[crate::texture::OwnedPreparedTexture],
) -> Result<(), FrameError> {
    let atlas = material.impostor();
    let field = material
        .features()
        .and_then(|f| f.wind)
        .and_then(worldrunner_render_plan::SceneWind::field);
    for handle in [
        atlas.and_then(|a| a.relief().map(|v| v.0)),
        atlas.and_then(worldrunner_render_plan::SceneImpostor::borders),
        field.map(worldrunner_render_plan::ScenePlanarWindField::texture),
        atlas
            .and_then(worldrunner_render_plan::SceneImpostor::relief)
            .and(material.normal()),
    ]
    .into_iter()
    .flatten()
    {
        if textures[handle.raw() as usize]
            .as_prepared()
            .format()
            .is_srgb()
        {
            return Err(FrameError::UnsupportedEffect(
                "impostor and force data must be linear",
            ));
        }
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
            translucency: None,
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
