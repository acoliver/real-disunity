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
    Contract(#[from] ContractError),
}

pub(crate) fn supported(limits: &wgpu::Limits) -> bool {
    limits.max_storage_buffers_per_shader_stage >= 5
        && limits.max_storage_buffer_binding_size >= 128
        && limits.max_buffer_size >= 128
        && limits.max_bindings_per_bind_group >= 11
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
            ("texels", &self.texels),
            ("images", &self.images),
        ] {
            buffer_size(&limits, name, bytes.len() as u64)?;
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
        buffer_size(&limits, "positions", count * 8)?;
        buffer_size(&limits, "products", count * 16)?;
        Ok(())
    }
}
