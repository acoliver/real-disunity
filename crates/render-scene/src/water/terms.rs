use worldrunner_render_plan::SceneWaterBody;

pub(super) fn mask(bodies: &[SceneWaterBody]) -> u32 {
    bodies.iter().fold(0, |mask, body| {
        mask | u32::from(body.material.lighting_normal_micro.is_some())
            | (u32::from(body.material.metallic_micro.is_some()) << 1)
            | (u32::from(body.material.normal_mips) << 2)
            | (u32::from(body.material.environment.is_some()) << 3)
            | (u32::from(body.material.direct_surface) << 4)
            | (u32::from(body.material.sun_disk.is_some()) << 5)
            | (u32::from(body.material.flow_coordinates.is_some()) << 6)
            | (u32::from(
                body.material
                    .wind
                    .is_some_and(|wind| wind.amplitude_micro != 0),
            ) << 7)
    })
}
pub(super) fn uniforms(body: &SceneWaterBody, time_milliseconds: u32) -> [f32; 32] {
    let mut data = [0.0; 32];
    if let Some(value) = body.material.lighting_normal_micro {
        data[0] = super::number(i64::from(value)) / 1000.0;
        data[1] = 1.0;
    }
    if let Some(value) = body.material.metallic_micro {
        data[2] = super::number(i64::from(value)) / 1000.0;
    }
    data[3] = f32::from(u8::from(body.material.normal_mips));
    if let Some(environment) = body.material.environment {
        data[4] = super::number(i64::from(environment.amplitude_micro)) / 1000.0;
        data[5] = super::number(i64::from(environment.edge_frequency_micro)) / 1000.0;
        data[6] = f32::from(u8::from(environment.depth_self_occlusion));
        data[7] = match environment.zero_frequency {
            worldrunner_render_plan::SceneWaterZeroFrequency::Skip => 1.0,
            worldrunner_render_plan::SceneWaterZeroFrequency::ConstantHalf => 2.0,
        };
    }
    data[8] = f32::from(u8::from(body.material.direct_surface));
    data[9] = f32::from(u8::from(body.material.metallic_micro.is_some()));
    if let Some(disk) = body.material.sun_disk {
        if disk.angular_radius_microradians != 2700 {
            data[14] = super::number(i64::from(disk.angular_radius_microradians) * 1000);
        }
        data[11] = 1.0;
        data[12] = super::number(i64::from(disk.amplitude_micro)) / 1000.0;
        data[13] = super::number(i64::from(disk.peak_micro)) / 1000.0;
    }
    if let Some(coordinates) = body.material.flow_coordinates {
        data[10] = 1.0;
        data[16..18].copy_from_slice(
            &coordinates
                .origin_millimeters
                .map(|v| super::number(i64::from(v))),
        );
        data[18..20].copy_from_slice(
            &coordinates
                .tile_millimeters
                .map(|v| 1.0 / super::number(i64::from(v))),
        );
    }
    data[20..32].copy_from_slice(&wind_uniforms(body, time_milliseconds));
    data
}
fn wind_uniforms(body: &SceneWaterBody, time_milliseconds: u32) -> [f32; 12] {
    let mut data = [0.0; 12];
    if let Some(wind) = body.material.wind {
        data[..2].copy_from_slice(
            &wind
                .field
                .origin_millimeters()
                .map(|v| super::number(i64::from(v))),
        );
        data[2..4].copy_from_slice(
            &wind
                .field
                .extent_millimeters()
                .map(|v| 1.0 / super::number(i64::from(v))),
        );
        data[4] = super::number(i64::from(wind.band.angular_frequency_micro())) / 1000.0;
        data[5] = super::number(i64::from(wind.band.perturb_micro())) / 1000.0;
        data[6] = super::number(i64::from(wind.band.spatial_phase_micro())) / 1000.0;
        data[7] = super::number(i64::from(wind.amplitude_micro)) / 1000.0;
        data[8] = 1.0;
        if wind.amplitude_micro != 0 {
            data[9] = f32::from_bits(time_milliseconds);
        }
    }
    data
}

pub(super) fn constants(options: [u32; 5]) -> Vec<(&'static str, f64)> {
    let mut constants = crate::renderer::specular_constants([
        options[0], options[1], options[2], options[3], 0, 0, 0,
    ])
    .to_vec();
    if options[4] != 0 {
        constants.push(("WATER_TERMS", f64::from(options[4])));
    }
    constants
}
pub(super) fn vertex_constants(options: [u32; 5]) -> Vec<(&'static str, f64)> {
    if options[4] & 128 != 0 {
        vec![("WATER_TERMS", f64::from(options[4]))]
    } else {
        Vec::new()
    }
}
