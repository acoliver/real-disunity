//! The runtime records into frames the submission authority owns and never submits itself.

use worldrunner_render_gpu::submission::TransactionError;
use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::module::StageKind;
use worldrunner_render_program::runtime::{
    BoundResource, ComputeProgram, DispatchRequest, ProgramRuntime,
};
use worldrunner_unity_serialized::shader_program::{ProgramParameters, UavParam};

use crate::common::{Gpu, same, stage, words_to_bytes};

const FILL: &str = "
@group(0) @binding(0) var<storage, read_write> result: array<u32>;
@compute @workgroup_size(1) fn main() { result[0] = 42u; result[1] = 43u; }";

fn fill_program(runtime: &ProgramRuntime) -> ComputeProgram {
    let parameters = ProgramParameters {
        uavs: vec![UavParam {
            name_index: 0,
            index: 0,
            original_index: 0,
        }],
        ..ProgramParameters::default()
    };
    let kernel = stage(
        FILL,
        ("main", StageKind::Compute),
        vec![same(0, 0)],
        parameters,
    );
    runtime.compute_program(&kernel).unwrap()
}

fn words(bytes: &[u8]) -> Vec<u32> {
    bytes
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

#[test]
fn two_runtimes_recorded_into_one_frame_are_submitted_once() {
    let gpu = Gpu::new();
    let (mut compute, mut drawing) = (gpu.runtime(), gpu.runtime());
    let program = fill_program(&compute);
    let output = compute
        .create_buffer("result", &words_to_bytes(&[0; 2]))
        .unwrap();
    let target = drawing
        .create_render_target("clear", [2, 2], wgpu::TextureFormat::Rgba8Unorm, None)
        .unwrap();
    let before = gpu.authority.submitted_serial();

    let mut frame = gpu.authority.begin(&gpu.device).unwrap();
    compute
        .dispatch(
            &mut frame,
            &DispatchRequest {
                program: &program,
                bindings: &[BoundResource::buffer(0, 0, output)],
                groups: [1, 1, 1],
            },
        )
        .unwrap();
    let words_read = compute.read_buffer(&mut frame, output).unwrap();
    drawing
        .clear_target(&mut frame, target, [1.0, 0.0, 0.0, 1.0], 1.0)
        .unwrap();
    let image_read = drawing.read_target(&mut frame, target).unwrap();
    assert_eq!(
        gpu.authority.submitted_serial(),
        before,
        "recording must not submit"
    );

    let submission = frame.finish().submit().unwrap();
    assert_eq!(gpu.authority.submitted_serial(), before + 1);
    assert_eq!(
        words(&words_read.resolve(submission.clone()).unwrap()),
        [42, 43]
    );
    let image = image_read.resolve(submission).unwrap();
    assert_eq!(image.pixel(1, 1), [255, 0, 0, 255]);
    assert_eq!(
        gpu.authority.submitted_serial(),
        before + 1,
        "reading back must not submit either"
    );
}

#[test]
fn an_encoder_the_authority_does_not_own_is_refused() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let target = runtime
        .create_render_target("t", [2, 2], wgpu::TextureFormat::Rgba8Unorm, None)
        .unwrap();
    let mut foreign = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    assert_eq!(
        runtime
            .clear_target(&mut foreign, target, [0.0; 4], 1.0)
            .unwrap_err(),
        ProgramError::Transaction(TransactionError::Missing)
    );
}

#[test]
fn device_loss_is_a_named_error_before_any_recording() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let target = runtime
        .create_render_target("t", [2, 2], wgpu::TextureFormat::Rgba8Unorm, None)
        .unwrap();
    let mut frame = gpu.authority.begin(&gpu.device).unwrap();
    gpu.authority
        .report_device_loss("adapter removed".to_owned());
    let lost = ProgramError::DeviceLost("adapter removed".to_owned());
    assert_eq!(
        runtime
            .clear_target(&mut frame, target, [0.0; 4], 1.0)
            .unwrap_err(),
        lost
    );
    assert_eq!(runtime.create_buffer("late", &[0; 4]).unwrap_err(), lost);
}

#[test]
fn a_recorded_write_lands_at_its_place_in_the_frames_order() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let buffer = runtime
        .create_buffer("data", &words_to_bytes(&[1, 2, 3, 4]))
        .unwrap();
    let mut frame = gpu.authority.begin(&gpu.device).unwrap();
    let before = runtime.read_buffer(&mut frame, buffer).unwrap();
    runtime
        .write_buffer(&mut frame, buffer, 4, &words_to_bytes(&[20, 30]))
        .unwrap();
    let after = runtime.read_buffer(&mut frame, buffer).unwrap();
    let submission = frame.finish().submit().unwrap();
    assert_eq!(
        words(&before.resolve(submission.clone()).unwrap()),
        [1, 2, 3, 4]
    );
    assert_eq!(words(&after.resolve(submission).unwrap()), [1, 20, 30, 4]);
}

#[test]
fn a_write_outside_the_buffer_or_off_the_copy_alignment_is_a_named_error() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let buffer = runtime.create_buffer("data", &[0; 8]).unwrap();
    let mut frame = gpu.authority.begin(&gpu.device).unwrap();
    assert_eq!(
        runtime
            .write_buffer(&mut frame, buffer, 4, &[0; 8])
            .unwrap_err(),
        ProgramError::WriteRange {
            offset: 4,
            len: 8,
            size: 8
        }
    );
    assert_eq!(
        runtime
            .write_buffer(&mut frame, buffer, 2, &[0; 4])
            .unwrap_err(),
        ProgramError::WriteAlignment { offset: 2, len: 4 }
    );
}
