use worldrunner_render_plan::{SceneWaterBody, SceneWaterShadowOffset, SceneWaterShadowResponse};

pub(super) const SPECIALIZATION: u32 = 1024;

pub(super) fn key(body: &SceneWaterBody) -> u32 {
    u32::from(
        body.material
            .shadow_response
            .is_some_and(|v| v != SceneWaterShadowResponse::default()),
    ) * SPECIALIZATION
}

pub(super) fn uniforms(body: &SceneWaterBody, data: &mut [f32; 32]) {
    if key(body) == 0 {
        return;
    }
    let response = body.material.shadow_response.expect("enabled response");
    data[15] = match response.offset {
        SceneWaterShadowOffset::ShadingNormal => 1.0,
        SceneWaterShadowOffset::GeometricNormal => 2.0,
        SceneWaterShadowOffset::None => 3.0,
    };
    data[30] = super::number(i64::from(response.diffuse_floor_micro)) / 1000.0;
    data[31] =
        f32::from(u8::from(response.depth_relief) + 2 * u8::from(response.disk_raw_visibility));
}
