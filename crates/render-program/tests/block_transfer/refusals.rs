//! Transfers refused by name before anything is recorded.

use wgpu::TextureFormat;
use worldrunner_render_program::error::{BlockTransferError, ProgramError, TransferSide};
use worldrunner_render_program::runtime::{BlockImageTransfer, ProgramRuntime, TextureHandle};

use crate::common::{Gpu, refusal, submit, texture, transfer};

/// A change to a valid request and the refusal it must get.
type Case = (fn(&mut BlockImageTransfer), BlockTransferError);

struct Setup {
    runtime: ProgramRuntime,
    source: TextureHandle,
    destination: TextureHandle,
}

impl Setup {
    /// A 4 by 4 `Rg32Uint` source of one layer and mip, and an 8 by 8 BC1 destination of four
    /// layers and four mips.
    fn new(gpu: &Gpu) -> Self {
        let mut runtime = gpu.runtime();
        let source = texture(&mut runtime, TextureFormat::Rg32Uint, [4, 4], 1, 1);
        let destination = texture(&mut runtime, TextureFormat::Bc1RgbaUnormSrgb, [8, 8], 4, 4);
        Self {
            runtime,
            source,
            destination,
        }
    }

    /// The transfer of two by two blocks into layer 0 mip 0.
    fn valid(&self) -> BlockImageTransfer {
        transfer(
            (self.source, 0, 0, [0, 0], [2, 2]),
            (self.destination, 0, 0, [8, 8]),
        )
    }

    /// Refuses `request` and checks nothing was staged or counted.
    fn refused(&mut self, request: &BlockImageTransfer) -> BlockTransferError {
        let before = self.runtime.live_bytes();
        let error = refusal(&mut self.runtime, request);
        assert_eq!(self.runtime.stats().transfer_buffers_built, 0);
        assert_eq!(self.runtime.live_bytes(), before);
        error
    }
}

#[test]
fn a_valid_request_is_accepted() {
    let Some(gpu) = Gpu::new() else { return };
    let mut setup = Setup::new(&gpu);
    let request = setup.valid();
    submit(&mut setup.runtime, |runtime, encoder| {
        runtime.transfer_block_image(encoder, &request)
    })
    .unwrap();
}

#[test]
fn a_texel_size_that_is_not_the_block_size_is_refused() {
    let Some(gpu) = Gpu::new() else { return };
    let mut setup = Setup::new(&gpu);
    let bc5 = texture(&mut setup.runtime, TextureFormat::Bc5RgUnorm, [8, 8], 1, 1);
    let request = transfer((setup.source, 0, 0, [0, 0], [2, 2]), (bc5, 0, 0, [8, 8]));
    assert_eq!(
        setup.refused(&request),
        BlockTransferError::BlockSizeMismatch {
            texel_bytes: 8,
            block_bytes: 16
        }
    );
}

#[test]
fn a_source_that_is_not_rg32uint_or_rgba32uint_is_refused() {
    let Some(gpu) = Gpu::new() else { return };
    let mut setup = Setup::new(&gpu);
    let plain = texture(&mut setup.runtime, TextureFormat::Rgba16Uint, [4, 4], 1, 1);
    let request = transfer(
        (plain, 0, 0, [0, 0], [2, 2]),
        (setup.destination, 0, 0, [8, 8]),
    );
    assert_eq!(
        setup.refused(&request),
        BlockTransferError::SourceFormat {
            format: TextureFormat::Rgba16Uint
        }
    );
}

#[test]
fn a_destination_that_is_not_block_compressed_is_refused() {
    let Some(gpu) = Gpu::new() else { return };
    let mut setup = Setup::new(&gpu);
    let plain = texture(&mut setup.runtime, TextureFormat::Rgba16Uint, [8, 8], 1, 1);
    let request = transfer((setup.source, 0, 0, [0, 0], [2, 2]), (plain, 0, 0, [8, 8]));
    assert_eq!(
        setup.refused(&request),
        BlockTransferError::DestinationNotCompressed {
            format: TextureFormat::Rgba16Uint
        }
    );
}

#[test]
fn an_extent_past_the_destination_mip_is_refused() {
    let Some(gpu) = Gpu::new() else { return };
    let mut setup = Setup::new(&gpu);
    let mut request = setup.valid();
    request.destination.extent = [9, 8];
    request.source.extent = [3, 2];
    assert_eq!(
        setup.refused(&request),
        BlockTransferError::ExtentPastMip {
            extent: [9, 8],
            mip: [8, 8]
        }
    );
    // Mip 2 is two texels across: the block that covers it does not make a bigger extent fit.
    let mut request = setup.valid();
    request.destination.mip = 2;
    request.destination.extent = [4, 4];
    request.source.extent = [1, 1];
    assert_eq!(
        setup.refused(&request),
        BlockTransferError::ExtentPastMip {
            extent: [4, 4],
            mip: [2, 2]
        }
    );
}

#[test]
fn a_block_count_that_is_not_the_source_region_is_refused() {
    let Some(gpu) = Gpu::new() else { return };
    let mut setup = Setup::new(&gpu);
    let mut request = setup.valid();
    request.source.extent = [3, 2];
    assert_eq!(
        setup.refused(&request),
        BlockTransferError::BlockCountMismatch {
            blocks: [2, 2],
            region: [3, 2]
        }
    );
}

#[test]
fn a_source_region_past_the_source_mip_is_refused() {
    let Some(gpu) = Gpu::new() else { return };
    let mut setup = Setup::new(&gpu);
    let mut request = setup.valid();
    request.source.origin = [3, 0];
    assert_eq!(
        setup.refused(&request),
        BlockTransferError::SourceRegionPastMip {
            origin: [3, 0],
            extent: [2, 2],
            mip: [4, 4]
        }
    );
}

#[test]
fn a_layer_or_mip_out_of_range_is_refused_on_each_side() {
    let Some(gpu) = Gpu::new() else { return };
    let mut setup = Setup::new(&gpu);
    let cases: [Case; 4] = [
        (
            |request| request.destination.layer = 4,
            BlockTransferError::LayerOutOfRange {
                side: TransferSide::Destination,
                layer: 4,
                layers: 4,
            },
        ),
        (
            |request| request.source.layer = 1,
            BlockTransferError::LayerOutOfRange {
                side: TransferSide::Source,
                layer: 1,
                layers: 1,
            },
        ),
        (
            |request| request.destination.mip = 4,
            BlockTransferError::MipOutOfRange {
                side: TransferSide::Destination,
                mip: 4,
                mips: 4,
            },
        ),
        (
            |request| request.source.mip = 1,
            BlockTransferError::MipOutOfRange {
                side: TransferSide::Source,
                mip: 1,
                mips: 1,
            },
        ),
    ];
    for (change, expected) in cases {
        let mut request = setup.valid();
        change(&mut request);
        assert_eq!(setup.refused(&request), expected);
    }
}

#[test]
fn a_handle_the_runtime_does_not_hold_is_a_named_error() {
    let Some(gpu) = Gpu::new() else { return };
    let mut setup = Setup::new(&gpu);
    let request = setup.valid();
    setup.runtime.release_texture(setup.source).unwrap();
    let error = submit(&mut setup.runtime, |runtime, encoder| {
        runtime.transfer_block_image(encoder, &request)
    })
    .unwrap_err();
    assert_eq!(error, ProgramError::UnknownHandle { kind: "texture" });
}
