//! Compare producer and local-consumer bits with runtime storage operands.
//!
//! WGSL does not guarantee signed-zero preservation. Each backend must agree across
//! the zero-bias paths and return P numerically, without requiring a zero's sign.
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
    let expression = expression.replace("shadow.bias.y", &bias);
    let vertical = if derivative {
        expression.replace("dpdx", "dpdy")
    } else {
        expression.clone()
    };
    format!(
        "var<private> receiver_0_a: vec4<f32>; var<private> receiver_0_b: vec2<f32>;
fn scene_capture_receiver(position: vec3<f32>, normal: vec3<f32>) {{
    let selected = {expression};
    let vertical = {vertical};
    receiver_0_a = vec4<f32>(selected, vertical.x); receiver_0_b = vertical.yz;
}}
{}",
        include_str!("receiver_zero_bias.wgsl")
    )
}

fn assert_position_values(result: &[Vec<u8>], derivative: bool) {
    for index in (0..16_usize).step_by(2) {
        let x = f32::from(u16::try_from(index % 4).unwrap()) + 0.5;
        let y = f32::from(u16::try_from(index / 4).unwrap()) + 0.5;
        let expected = if derivative {
            [vec![1.0, 0.0, 0.0, 0.0], vec![1.0, 0.0]]
        } else {
            [vec![x, y, 0.0, x], vec![y, 0.0]]
        };
        for (attachment, expected) in expected.iter().enumerate() {
            let stride = expected.len() * 4;
            let actual: Vec<_> = result[attachment][index * stride..(index + 1) * stride]
                .chunks_exact(4)
                .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
                .collect();
            assert_eq!(&actual, expected, "attachment={attachment} pixel={index}");
        }
    }
}

/// Compare actual dP across paired producers and local derivative selects, without
/// replacing derivatives by raw storage operands: Metal can normalize zero signs
/// during arithmetic. Displacement paths must agree bitwise; B must equal P
/// numerically, regardless of whether the position select normalizes a zero.
#[test]
fn gpu_runtime_zero_bias_paths_agree_on_position_bits() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    for negative in [false, true] {
        let bias = if negative { -0.0 } else { 0.0 };
        for normal in [
            1.0_f32.to_bits(),
            f32::INFINITY.to_bits(),
            f32::NAN.to_bits(),
            0x7fc0_1234,
        ] {
            let producers = [ReceiverCapture::Position, ReceiverCapture::Normal]
                .map(|capture| operands(&context, &operand_source(bias, capture), normal));
            assert_eq!(
                producers[0], producers[1],
                "normal={normal:08x} bias={bias}"
            );
            assert_position_values(&producers[0], true);
            let displacement = operands(&context, &selection_source(negative, true), normal);
            assert_eq!(
                displacement, producers[0],
                "normal={normal:08x} negative={negative}"
            );
            let position = operands(&context, &selection_source(negative, false), normal);
            assert_position_values(&position, false);
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
