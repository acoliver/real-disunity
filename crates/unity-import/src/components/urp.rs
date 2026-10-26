//! URP components: 2D lights and the additional camera data.

use super::ComponentError;
use crate::{
    package::PackageScript,
    scene::{
        ObjectId, Scene, Value,
        read::{RGBA, flag, hex_int32s, integer, number, variant, vector},
    },
};

const LIGHT_TYPES: [&str; 5] = ["Parametric", "Freeform", "Sprite", "Point", "Global"];
const GLOBAL: &str = "Global";

/// A URP global 2D light. Other light types carry shape data this view does not hold, so
/// importing them fails.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct Light2D {
    /// Linear RGBA color.
    pub color: [f64; 4],
    pub intensity: f64,
    /// IDs of the sorting layers the light applies to.
    pub target_sorting_layers: Vec<i64>,
    /// Index into the 2D renderer's blend styles.
    pub blend_style: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum CameraRenderType {
    Base,
    Overlay,
}

/// A per-camera override of a render pipeline setting.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum CameraOverride {
    Off,
    On,
    UsePipelineSettings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum Antialiasing {
    None,
    Fxaa,
    Smaa,
    Taa,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct UrpPostProcessing {
    pub enabled: bool,
    pub antialiasing: Antialiasing,
    pub stop_nan: bool,
    pub dithering: bool,
}

/// URP's `UniversalAdditionalCameraData`. Which settings render is the engine's decision; this
/// view only translates them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct UrpCamera {
    pub render_type: CameraRenderType,
    /// `-1` selects the pipeline's default renderer.
    pub renderer_index: i64,
    pub render_shadows: bool,
    pub clear_depth: bool,
    pub depth_texture: CameraOverride,
    pub opaque_texture: CameraOverride,
    pub post_processing: UrpPostProcessing,
}

const OVERRIDES: [CameraOverride; 3] = [
    CameraOverride::Off,
    CameraOverride::On,
    CameraOverride::UsePipelineSettings,
];

impl Scene {
    pub fn light_2d(&self, id: ObjectId) -> Result<Light2D, ComponentError> {
        let fields = self.expect_script(id, PackageScript::Light2D)?;
        let light_type = self.field(
            id,
            "m_LightType",
            variant(fields, "m_LightType", &LIGHT_TYPES, 3),
        )?;
        if light_type != GLOBAL {
            return Err(self.component_error(
                id,
                format!("{light_type} 2D lights are not supported; only Global lights import"),
            ));
        }
        Ok(Light2D {
            color: self.field(id, "m_Color", vector(fields, "m_Color", RGBA, [1.0; 4]))?,
            intensity: self.field(id, "m_Intensity", number(fields, "m_Intensity", 1.0))?,
            target_sorting_layers: self.field(
                id,
                "m_ApplyToSortingLayers",
                hex_int32s(fields, "m_ApplyToSortingLayers"),
            )?,
            blend_style: self.field(
                id,
                "m_BlendStyleIndex",
                integer(fields, "m_BlendStyleIndex", 0),
            )?,
        })
    }

    pub fn urp_camera(&self, id: ObjectId) -> Result<UrpCamera, ComponentError> {
        let fields = self.expect_script(id, PackageScript::UniversalAdditionalCameraData)?;
        if fields
            .get("m_Cameras")
            .and_then(Value::as_sequence)
            .is_some_and(|stack| !stack.is_empty())
        {
            return Err(self.component_error(id, "camera stacks are not supported"));
        }
        let render_types = [CameraRenderType::Base, CameraRenderType::Overlay];
        Ok(UrpCamera {
            render_type: self.field(
                id,
                "m_CameraType",
                variant(fields, "m_CameraType", &render_types, 0),
            )?,
            renderer_index: self.field(
                id,
                "m_RendererIndex",
                integer(fields, "m_RendererIndex", -1),
            )?,
            render_shadows: self.field(
                id,
                "m_RenderShadows",
                flag(fields, "m_RenderShadows", true),
            )?,
            clear_depth: self.field(id, "m_ClearDepth", flag(fields, "m_ClearDepth", true))?,
            depth_texture: self.camera_override(id, fields, "m_RequiresDepthTextureOption")?,
            opaque_texture: self.camera_override(id, fields, "m_RequiresOpaqueTextureOption")?,
            post_processing: self.post_processing(id, fields)?,
        })
    }

    fn camera_override(
        &self,
        id: ObjectId,
        fields: &Value,
        key: &str,
    ) -> Result<CameraOverride, ComponentError> {
        self.field(id, key, variant(fields, key, &OVERRIDES, 2))
    }

    fn post_processing(
        &self,
        id: ObjectId,
        fields: &Value,
    ) -> Result<UrpPostProcessing, ComponentError> {
        let modes = [
            Antialiasing::None,
            Antialiasing::Fxaa,
            Antialiasing::Smaa,
            Antialiasing::Taa,
        ];
        Ok(UrpPostProcessing {
            enabled: self.field(
                id,
                "m_RenderPostProcessing",
                flag(fields, "m_RenderPostProcessing", false),
            )?,
            antialiasing: self.field(
                id,
                "m_Antialiasing",
                variant(fields, "m_Antialiasing", &modes, 0),
            )?,
            stop_nan: self.field(id, "m_StopNaN", flag(fields, "m_StopNaN", false))?,
            dithering: self.field(id, "m_Dithering", flag(fields, "m_Dithering", false))?,
        })
    }
}
