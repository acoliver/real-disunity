//! Overlay-only draw style uniform: the distance field of a text draw, and whether the texture
//! holds colour premultiplied by its alpha. Ordinary groups bind an explicit zero record.
use worldrunner_world_simulation::ui::presentation::UiDistanceStyle;

pub(crate) fn layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("UI distance style"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: wgpu::BufferSize::new(32),
            },
            count: None,
        }],
    })
}

pub(super) fn pack(style: Option<UiDistanceStyle>, premultiplied: bool) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(32);
    let (spread, radius, mode, color) = style.map_or((0., 0., 0, [0.; 4]), |s| {
        (s.spread_px, s.radius_px, s.mode as u32, s.effect_color)
    });
    bytes.extend(spread.to_le_bytes());
    bytes.extend(radius.to_le_bytes());
    bytes.extend(mode.to_le_bytes());
    bytes.extend(u32::from(premultiplied).to_le_bytes());
    bytes.extend(color.into_iter().flat_map(f32::to_le_bytes));
    bytes
}

pub(super) fn upload(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    style: Option<UiDistanceStyle>,
    premultiplied: bool,
) -> wgpu::BindGroup {
    let uniform = worldrunner_render_gpu::resource::upload_buffer(
        device,
        queue,
        "UI distance style",
        wgpu::BufferUsages::UNIFORM,
        &pack(style, premultiplied),
    );
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("UI distance style"),
        layout: &layout(device),
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform.as_entire_binding(),
        }],
    })
}

pub(super) fn validate(quad: &super::DynamicQuad, index: usize) -> Result<(), super::DynamicError> {
    let Some(style) = quad.distance_field else {
        return Ok(());
    };
    let invalid = || super::DynamicError::InvalidQuad {
        index,
        field: "distance_field",
    };
    style.validate().map_err(|_| invalid())?;
    if !quad.overlay || quad.texture.is_none() || quad.triangle_uv.is_some() {
        return Err(invalid());
    }
    let corners = quad.corners.ok_or_else(invalid)?;
    worldrunner_world_simulation::ui::presentation::validate_distance_mapping(corners, quad.uv_rect)
        .map_err(|_| invalid())
}

#[cfg(test)]
mod tests {
    use super::*;
    use worldrunner_world_simulation::ui::presentation::UiDistanceMode;
    #[test]
    fn uniform_layout_and_ordinary_reset() {
        assert_eq!(pack(None, false), vec![0; 32]);
        assert_eq!(&pack(None, true)[12..16], &1_u32.to_le_bytes());
        let bytes = pack(
            Some(UiDistanceStyle {
                spread_px: 8.,
                radius_px: 2.,
                mode: UiDistanceMode::Glow,
                effect_color: [0.25, 0.5, 0.75, 1.],
            }),
            false,
        );
        assert_eq!(bytes.len(), 32);
        assert_eq!(&bytes[0..4], &8_f32.to_le_bytes());
        assert_eq!(&bytes[4..8], &2_f32.to_le_bytes());
        assert_eq!(&bytes[8..12], &3_u32.to_le_bytes());
        assert_eq!(&bytes[12..16], &[0; 4]);
        assert_eq!(&bytes[16..20], &0.25_f32.to_le_bytes());
        assert_eq!(&bytes[28..32], &1_f32.to_le_bytes());
    }
}
