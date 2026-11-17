//! Per-frame CPU values copied into slot buffers through the recording frame's upload staging.
use crate::{FrameError, PostError};
use worldrunner_render_gpu::submission::Publications;

/// Stages `bytes` in the reusable upload staging of the transaction `encoder` records and copies
/// them into the head of `destination` at the current point of the encoder. The bytes are copied
/// at stage time, so the caller's values must be final. `bytes` must be a nonzero multiple of
/// four bytes long, which the fixed sizes of the post uploads are.
///
/// # Errors
///
/// [`PostError::Frame`] carrying `TransactionError::Missing` outside a prepared frame, or
/// `ForeignEncoder` when `encoder` is not the one that frame records. Nothing is staged then.
pub(crate) fn stage_copy(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    destination: &wgpu::Buffer,
    bytes: &[u8],
) -> Result<(), PostError> {
    let frame = Publications::for_encoder(encoder).map_err(FrameError::from)?;
    let (source, offset) = frame.stage_upload(device, bytes);
    let size = wgpu::BufferAddress::try_from(bytes.len()).expect("an upload fits a buffer address");
    encoder.copy_buffer_to_buffer(&source, offset, destination, 0, size);
    Ok(())
}
