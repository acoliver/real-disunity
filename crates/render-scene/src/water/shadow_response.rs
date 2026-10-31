use worldrunner_render_plan::{
    SceneWaterBody, SceneWaterShadowFilter, SceneWaterShadowOffset, SceneWaterShadowResponse,
};

pub(super) const SPECIALIZATION: u32 = 1024;

pub(super) fn key(body: &SceneWaterBody) -> u32 {
    let response = body.material.shadow_response.unwrap_or_default();
    let response_key = u32::from(response != SceneWaterShadowResponse::default()) * SPECIALIZATION;
    response_key
        | match response.filter {
            SceneWaterShadowFilter::ReceiverDefault => 0,
            SceneWaterShadowFilter::WeightedPcf5 => super::shadow_filter::SPECIALIZATION,
        }
}

pub(super) fn specialize(source: &str, terms: u32, shadows: bool) -> std::borrow::Cow<'_, str> {
    if !shadows || terms & super::shadow_filter::SPECIALIZATION == 0 {
        return std::borrow::Cow::Borrowed(source);
    }
    std::borrow::Cow::Owned(super::shadow_filter::source(source))
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
    data[31] = f32::from(
        u8::from(response.depth_relief)
            + 2 * u8::from(response.disk_raw_visibility)
            + 4 * u8::from(response.filter == SceneWaterShadowFilter::WeightedPcf5),
    );
}
