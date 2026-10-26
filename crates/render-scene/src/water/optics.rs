use worldrunner_render_plan::SceneWaterBody;

pub(super) fn uniforms(body: &SceneWaterBody) -> [f32; 28] {
    let scalar = |v: u32| crate::view::meters_scalar(v) / 1000.0;
    let mut data = [0.0; 28];
    if let Some(optics) = body.material.optics {
        data[0] = scalar(optics.fresnel_micro);
        data[1] = scalar(optics.specular_micro);
        data[2] = 1.0;
        data[4..7].copy_from_slice(&optics.scattering_micro.map(scalar));
        if let Some(tint) = optics.tint {
            data[16..19].copy_from_slice(&tint.colors_micro[0].map(scalar));
            data[19] = 1.0;
            data[20..23].copy_from_slice(&tint.colors_micro[1].map(scalar));
            data[24..26].copy_from_slice(&tint.decay_micro.map(scalar));
            data[26..28].copy_from_slice(
                &tint
                    .offsets_micro
                    .map(|v| super::number(i64::from(v)) / 1000.0),
            );
        }
    }
    if let Some(shore) = body.material.shoreline {
        data[8] = super::number(i64::from(shore.depth_millimeters));
        data[9] = super::number(i64::from(shore.normal_gain_micro)) / 1000.0;
        data[10] = scalar(shore.opacity_micro);
        data[11] = 1.0;
    }
    if let Some(bound) = body.material.refraction_millimeters {
        data[12] = super::number(i64::from(bound));
        data[13] = 1.0;
    }
    data
}
