use super::policy_tests::{authored_table_format, sample_table};
use super::*;

const INTENSITY: u32 = 2_950_000;

// 3baa4c3 single-sample integration: H=+Z, NoV=NoL=VoH=0.5, roughness=0.5.
fn base_generated_coefficients() -> [f32; 2] {
    let a2 = 0.5_f32.powi(4);
    let v = 0.5 * (0.5_f32 * 0.5 * (1.0 - a2) + a2).sqrt();
    let weight = 4.0 * (0.5 / (v + v)) * 0.5 * 0.5;
    let fc = 0.5_f32.powi(5);
    [1.0 - fc, fc].map(|value| value * weight)
}

fn assert_base_output(context: &crate::device::GpuContext, maps: &PreparedEnvironmentMaps) {
    let bytes = read_texture(context.device(), context.submission(), &maps.lut, 0);
    let authored = maps.authored_brdf().is_some();
    let offset = if authored { 256 } else { 0 };
    let actual: [f32; 2] = std::array::from_fn(|channel| {
        let index = offset + channel * 2;
        half::f16::from_le_bytes(bytes[index..index + 2].try_into().unwrap()).to_f32()
    });
    let expected = if authored {
        [0.625, 0.25]
    } else {
        base_generated_coefficients()
    };
    for (actual, expected) in actual.into_iter().zip(expected) {
        let expected = expected * 2.95;
        // Binary16 storage can truncate on Metal or round to nearest on other backends.
        assert!((actual - expected).abs() <= expected * 2.0_f32.powi(-10));
    }
    let lobe = sample_table(context, &maps.views().unwrap().1);
    assert_eq!(
        lobe.to_bits(),
        ((0.4 * actual[0] + actual[1]) * 2.0).to_bits()
    );
    let texels = bytes
        .chunks_exact(256)
        .flat_map(|row| &row[..usize::try_from(maps.lut.width()).unwrap() * 8])
        .copied()
        .collect::<Vec<_>>();
    println!(
        "BASE authored={authored} lut={texels:?} lobe={:08x}",
        lobe.to_bits()
    );
}

#[test]
fn existing_preparation_paths_match_base_half_float_arithmetic() {
    let context = pollster::block_on(crate::device::GpuContext::request()).unwrap();
    let source = source_cube(&context);
    let config = EnvironmentPreparation {
        cube_size: 4,
        mip_count: 3,
        lut_size: 1,
        samples: 1,
        specular_intensity_micro: INTENSITY,
        ..settings()
    };
    let cube = worldrunner_render_plan::SceneSpecularCube::new(4, vec![0; 288].into()).unwrap();
    for path in 0..4 {
        let mut encoder = context.submission().begin(context.device()).unwrap();
        let budget = AllocationBudget(1_000_000);
        let maps = match path {
            0 => encode(context.device(), &mut encoder, &source, config, budget),
            1 => authored::encode(context.device(), &mut encoder, &cube, config, budget),
            2 => encode_with_authored_brdf(
                context.device(),
                &mut encoder,
                &source,
                config,
                authored_table_format(&context, wgpu::TextureFormat::Rg16Float),
                budget,
            ),
            _ => authored::encode_with_authored_brdf(
                context.device(),
                &mut encoder,
                &cube,
                config,
                authored_table_format(&context, wgpu::TextureFormat::Rg16Float),
                budget,
            ),
        }
        .unwrap();
        encoder.finish().submit().unwrap();
        assert_base_output(&context, &maps);
    }
}
