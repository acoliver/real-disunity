//! A draw is prepared against the signature of the pass it will join, then recorded into a pass
//! the caller opened.

use worldrunner_render_program::error::{ProgramError, UnsupportedTarget};
use worldrunner_render_program::runtime::{DrawPacket, Geometry, TargetSignature};

use crate::common::Gpu;
use crate::patch::{patch, rectangle, state};
use crate::surface::{Surface, pixel, read};

const RGBA8: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

#[test]
fn a_prepared_draw_records_into_a_pass_the_caller_opened_without_submitting() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let patch = patch(&runtime);
    let green = patch.color(&mut runtime, [0.0, 1.0, 0.0, 1.0]);
    let input = rectangle(&mut runtime, [-0.5, -0.5, 0.5, 0.5], 0.5);
    let state = state();
    let signature = TargetSignature::new(&[RGBA8], Some(DEPTH), 1).unwrap();
    let prepared = runtime
        .prepare_draw(
            &DrawPacket {
                program: &patch.program,
                state: &state,
                front_face: wgpu::FrontFace::Cw,
                vertex_inputs: &[input],
                bindings: &green,
                geometry: Geometry::Vertices { first: 0, count: 6 },
            },
            signature,
        )
        .unwrap();
    assert_eq!(prepared.signature(), signature);

    let surface = Surface::new(&gpu.device, RGBA8);
    let before = gpu.authority.submitted_serial();
    let mut frame = gpu.authority.begin(&gpu.device).unwrap();
    let mut pass = surface.open(&mut frame);
    prepared.record(&mut pass);
    drop(pass);
    let readback = surface.copy_color(&gpu.device, &mut frame);
    assert_eq!(
        gpu.authority.submitted_serial(),
        before,
        "recording must not submit"
    );
    frame.finish().submit().unwrap();
    assert_eq!(gpu.authority.submitted_serial(), before + 1);

    let bytes = read(&gpu.device, &readback);
    // The patch covers clip -0.5..0.5, which is columns and rows 16 up to 48.
    assert_eq!(pixel(&bytes, 32, 32), [0, 255, 0, 255]);
    assert_eq!(pixel(&bytes, 17, 46), [0, 255, 0, 255]);
    assert_eq!(pixel(&bytes, 8, 32), [0, 0, 0, 255]);
    assert_eq!(pixel(&bytes, 32, 56), [0, 0, 0, 255]);
}

#[test]
fn the_pipeline_follows_the_signature_it_was_prepared_for() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let patch = patch(&runtime);
    let green = patch.color(&mut runtime, [0.0, 1.0, 0.0, 1.0]);
    let input = rectangle(&mut runtime, [-0.5, -0.5, 0.5, 0.5], 0.5);
    let state = state();
    let packet = DrawPacket {
        program: &patch.program,
        state: &state,
        front_face: wgpu::FrontFace::Cw,
        vertex_inputs: &[input],
        bindings: &green,
        geometry: Geometry::Vertices { first: 0, count: 6 },
    };
    for format in [RGBA8, wgpu::TextureFormat::Rgba16Float, RGBA8] {
        let signature = TargetSignature::new(&[format], Some(DEPTH), 1).unwrap();
        runtime.prepare_draw(&packet, signature).unwrap();
    }
    assert_eq!(
        runtime.cached_pipelines(),
        2,
        "one pipeline for each distinct signature"
    );
}

#[test]
fn signatures_the_runtime_does_not_draw_into_are_named() {
    assert_eq!(
        TargetSignature::new(&[RGBA8], Some(DEPTH), 4).unwrap_err(),
        ProgramError::UnsupportedTarget(UnsupportedTarget::Multisampled { samples: 4 })
    );
    assert_eq!(
        TargetSignature::new(&[RGBA8, RGBA8], Some(DEPTH), 1).unwrap_err(),
        ProgramError::UnsupportedTarget(UnsupportedTarget::ColorAttachments { count: 2 })
    );
    assert_eq!(
        TargetSignature::new(&[], Some(DEPTH), 1).unwrap_err(),
        ProgramError::UnsupportedTarget(UnsupportedTarget::ColorAttachments { count: 0 })
    );
    let stencil = wgpu::TextureFormat::Depth24PlusStencil8;
    assert_eq!(
        TargetSignature::new(&[RGBA8], Some(stencil), 1).unwrap_err(),
        ProgramError::UnsupportedTarget(UnsupportedTarget::Stencil { format: stencil })
    );
    assert_eq!(
        TargetSignature::new(&[RGBA8], Some(RGBA8), 1).unwrap_err(),
        ProgramError::UnsupportedTarget(UnsupportedTarget::NotDepth { format: RGBA8 })
    );
    assert!(TargetSignature::new(&[RGBA8], None, 1).is_ok());
}
