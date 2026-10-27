use super::LayeredGpuSource;
use worldrunner_render_post::contracts::ContractError;

#[derive(Debug, thiserror::Error)]
pub enum LayeredGpuError {
    #[error("layered terrain is unsupported by the device limits")]
    Unsupported,
    #[error("layered terrain {buffer} needs {required} bytes, device limit is {limit}")]
    BufferLimit {
        buffer: &'static str,
        required: u64,
        limit: u64,
    },
    #[error(transparent)]
    Upload(#[from] worldrunner_render_gpu::texture::UploadError),
    #[error(transparent)]
    Contract(#[from] ContractError),
}

pub(crate) fn supported(limits: &wgpu::Limits) -> bool {
    limits.max_storage_buffers_per_shader_stage >= 5
        && limits.max_storage_buffer_binding_size >= 128
        && limits.max_buffer_size >= 128
        && limits.max_bindings_per_bind_group >= 23
        && limits.max_sampled_textures_per_shader_stage >= 16
        && limits.max_samplers_per_shader_stage >= 4
        && limits.max_compute_workgroup_size_x >= 64
        && limits.max_compute_workgroup_size_y >= 8
        && limits.max_compute_invocations_per_workgroup >= 64
        && limits.max_storage_textures_per_shader_stage >= 1
}

pub(super) fn buffer_size(
    limits: &wgpu::Limits,
    buffer: &'static str,
    required: u64,
) -> Result<(), LayeredGpuError> {
    let limit = limits
        .max_storage_buffer_binding_size
        .min(limits.max_buffer_size);
    if required > limit {
        return Err(LayeredGpuError::BufferLimit {
            buffer,
            required,
            limit,
        });
    }
    Ok(())
}

impl LayeredGpuSource {
    pub fn validate_device(&self, device: &wgpu::Device) -> Result<(), LayeredGpuError> {
        let limits = device.limits();
        if !supported(&limits) {
            return Err(LayeredGpuError::Unsupported);
        }
        for (name, bytes) in [
            ("parameters", &self.parameters),
            ("grid", &self.grid),
            ("images", &self.images),
        ] {
            buffer_size(&limits, name, bytes.len() as u64)?;
        }
        for images in &self.arrays {
            let first = images[0].as_prepared();
            if images.len() > limits.max_texture_array_layers as usize
                || first.width().max(first.height()) > limits.max_texture_dimension_2d
                || !device
                    .features()
                    .contains(first.format().required_features())
            {
                return Err(LayeredGpuError::Unsupported);
            }
        }
        Ok(())
    }

    pub(super) fn validate_positions(
        &self,
        device: &wgpu::Device,
        count: u64,
    ) -> Result<(), LayeredGpuError> {
        self.validate_device(device)?;
        let limits = device.limits();
        buffer_size(&limits, "positions", count * 8 + 8)?;
        buffer_size(&limits, "products", count * 16)?;
        Ok(())
    }
}
