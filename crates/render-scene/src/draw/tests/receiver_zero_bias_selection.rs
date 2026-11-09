//! Exercise the production selects with runtime storage operands and inspect signed-zero bits.
use super::*;

fn selection_source(negative: bool, derivative: bool) -> String {
    let source = include_str!("../../shader/shadows.wgsl");
    let expression = if derivative {
        source
            .split_once("let receiver_dx = ")
            .unwrap()
            .1
            .split_once(';')
            .unwrap()
            .0
    } else {
        source
            .split_once("vec4<f32>(select(position + ")
            .unwrap()
            .1
            .split_once(", 1.0)")
            .unwrap()
            .0
    };
    let expression = if derivative {
        expression.to_owned()
    } else {
        format!("select(position + {expression}")
    };
    let bias = format!(
        "bitcast<f32>(operands[0].z | {}u)",
        u32::from(negative) << 31
    );
    let expression = expression
        .replace("dpdx(position)", "position")
        .replace("dpdx(normal)", "normal")
        .replace("shadow.bias.y", &bias);
    format!(
        "var<private> receiver_0_a: vec4<f32>; var<private> receiver_0_b: vec2<f32>;
fn scene_capture_receiver(position: vec3<f32>, normal: vec3<f32>) {{
    let selected = {expression};
    receiver_0_a = vec4<f32>(selected, selected.x); receiver_0_b = selected.yz;
}}
{}",
        include_str!("receiver_zero_bias.wgsl")
    )
}

#[test]
fn gpu_runtime_zero_bias_selects_position_bits_including_negative_zero() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    for derivative in [false, true] {
        for negative in [false, true] {
            for normal in [
                1.0_f32.to_bits(),
                f32::INFINITY.to_bits(),
                f32::NAN.to_bits(),
                0x7fc0_1234,
            ] {
                let result = operands(&context, &selection_source(negative, derivative), normal);
                for index in (0..16_usize).step_by(2) {
                    let x = f32::from(u16::try_from(index % 4).unwrap()) + 0.5;
                    let y = f32::from(u16::try_from(index / 4).unwrap()) + 0.5;
                    let sign = u32::try_from((index / 4) % 2).unwrap() << 31;
                    let expected = [x.to_bits(), y.to_bits(), sign, x.to_bits()];
                    let actual: Vec<_> = result[0][index * 16..(index + 1) * 16]
                        .chunks_exact(4)
                        .map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()))
                        .collect();
                    assert_eq!(
                        actual, expected,
                        "normal={normal:08x} negative={negative} derivative={derivative} pixel={index}"
                    );
                    let actual: Vec<_> = result[1][index * 8..(index + 1) * 8]
                        .chunks_exact(4)
                        .map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()))
                        .collect();
                    assert_eq!(actual, [y.to_bits(), sign]);
                }
            }
        }
    }
}

#[test]
fn runtime_selection_operand_sources_validate() {
    for negative in [false, true] {
        for derivative in [false, true] {
            let source = selection_source(negative, derivative);
            let module = naga::front::wgsl::parse_str(&source).unwrap();
            naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::all(),
            )
            .validate(&module)
            .unwrap();
        }
    }
}
