//! The transfer's staging buffer: made once, reused, counted in the budget.

use wgpu::TextureFormat;
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_program::error::ProgramError;

use crate::common::{Gpu, submit, texture, transfer};

#[test]
fn repeated_transfers_make_no_buffer_after_the_first() {
    let Some(gpu) = Gpu::new() else { return };
    let mut runtime = gpu.runtime();
    let source = texture(&mut runtime, TextureFormat::Rg32Uint, [2, 2], 1, 1);
    let destination = texture(&mut runtime, TextureFormat::Bc1RgbaUnormSrgb, [8, 8], 4, 4);
    let before = runtime.live_bytes();
    let whole = transfer((source, 0, 0, [0, 0], [2, 2]), (destination, 0, 2, [8, 8]));
    submit(&mut runtime, |runtime, encoder| {
        runtime.transfer_block_image(encoder, &whole)
    })
    .unwrap();
    let first = runtime.stats();
    let held = runtime.live_bytes();
    assert_eq!(first.transfer_buffers_built, 1);
    assert!(held > before, "the staging buffer is in the budget");
    let holdings = runtime.holdings();
    let staging: Vec<_> = holdings
        .iter()
        .filter(|holding| holding.label == "block transfer staging")
        .collect();
    assert_eq!(staging.len(), 1);
    assert_eq!(staging[0].count, 1);
    // Several frames, a frame with two transfers, and a smaller transfer into a short mip.
    let small = transfer((source, 0, 0, [1, 1], [1, 1]), (destination, 2, 3, [2, 2]));
    for _ in 0..3 {
        submit(&mut runtime, |runtime, encoder| {
            runtime.transfer_block_image(encoder, &whole)?;
            runtime.transfer_block_image(encoder, &small)
        })
        .unwrap();
    }
    assert_eq!(runtime.stats(), first);
    assert_eq!(runtime.live_bytes(), held);
}

#[test]
fn a_larger_transfer_grows_the_staging_buffer_once_and_a_smaller_one_reuses_it() {
    let Some(gpu) = Gpu::new() else { return };
    let mut runtime = gpu.runtime();
    let source = texture(&mut runtime, TextureFormat::Rg32Uint, [4, 4], 1, 1);
    let destination = texture(
        &mut runtime,
        TextureFormat::Bc1RgbaUnormSrgb,
        [16, 16],
        1,
        1,
    );
    let one = transfer((source, 0, 0, [0, 0], [1, 1]), (destination, 0, 0, [4, 4]));
    let all = transfer(
        (source, 0, 0, [0, 0], [4, 4]),
        (destination, 0, 0, [16, 16]),
    );
    for (request, built) in [(&one, 1), (&all, 2), (&one, 2), (&all, 2)] {
        submit(&mut runtime, |runtime, encoder| {
            runtime.transfer_block_image(encoder, request)
        })
        .unwrap();
        assert_eq!(runtime.stats().transfer_buffers_built, built);
    }
}

#[test]
fn a_transfer_over_the_budget_is_refused_and_makes_no_buffer() {
    let Some(gpu) = Gpu::new() else { return };
    let mut probe = gpu.runtime();
    texture(&mut probe, TextureFormat::Rg32Uint, [2, 2], 1, 1);
    texture(&mut probe, TextureFormat::Bc1RgbaUnormSrgb, [8, 8], 1, 1);
    let textures = probe.live_bytes();
    let mut runtime = gpu.runtime_with(AllocationBudget(textures + 16));
    let source = texture(&mut runtime, TextureFormat::Rg32Uint, [2, 2], 1, 1);
    let destination = texture(&mut runtime, TextureFormat::Bc1RgbaUnormSrgb, [8, 8], 1, 1);
    let request = transfer((source, 0, 0, [0, 0], [2, 2]), (destination, 0, 0, [8, 8]));
    let error = submit(&mut runtime, |runtime, encoder| {
        runtime.transfer_block_image(encoder, &request)
    })
    .unwrap_err();
    assert!(matches!(error, ProgramError::OverBudget { .. }), "{error}");
    assert_eq!(runtime.stats().transfer_buffers_built, 0);
    assert_eq!(runtime.live_bytes(), textures);
}
