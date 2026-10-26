use super::{DynamicError, DynamicQuad};
use glam::{Mat4, Quat, Vec3};

pub(super) fn validate_texture(
    device: &wgpu::Device,
    texture: &crate::texture::PreparedTexture<'_>,
) -> Result<(), DynamicError> {
    if texture.width().max(texture.height()) > device.limits().max_texture_dimension_2d {
        return Err(DynamicError::DeviceLimit("texture dimension"));
    }
    if !device
        .features()
        .contains(texture.format().required_features())
    {
        return Err(DynamicError::DeviceLimit("texture format features"));
    }
    let features = texture
        .format()
        .guaranteed_format_features(device.features());
    if !features
        .flags
        .contains(wgpu::TextureFormatFeatureFlags::FILTERABLE)
        || !features
            .allowed_usages
            .contains(wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST)
    {
        return Err(DynamicError::DeviceLimit("filterable sampled texture"));
    }
    Ok(())
}

pub(super) fn validate_quad(
    quad: &DynamicQuad,
    index: usize,
    texture_count: usize,
) -> Result<(), DynamicError> {
    let invalid = |field| DynamicError::InvalidQuad { index, field };
    if !quad.position.into_iter().all(f32::is_finite) {
        return Err(invalid("position"));
    }
    if !quad.scale.into_iter().all(f32::is_finite) {
        return Err(invalid("scale"));
    }
    if quad
        .corners
        .is_some_and(|corners| corners.into_iter().flatten().any(|v| !v.is_finite()))
    {
        return Err(invalid("corners"));
    }
    let rotation = Quat::from_array(quad.rotation);
    if !rotation.is_finite() || (rotation.length_squared() - 1.0).abs() > 0.001 {
        return Err(invalid("rotation"));
    }
    if !quad.rect.into_iter().all(f32::is_finite)
        || quad.rect[0] >= quad.rect[2]
        || quad.rect[1] >= quad.rect[3]
    {
        return Err(invalid("rect"));
    }
    if !quad.uv_rect.into_iter().all(f32::is_finite) {
        return Err(invalid("uv_rect"));
    }
    if !quad
        .tint
        .into_iter()
        .all(|value| value.is_finite() && value >= 0.)
        || quad.tint[3] > 1.
    {
        return Err(invalid("tint"));
    }
    if quad.texture.is_some_and(|texture| texture >= texture_count) {
        return Err(invalid("texture"));
    }
    let model = Mat4::from_scale_rotation_translation(
        Vec3::from_array(quad.scale),
        rotation.normalize(),
        Vec3::from_array(quad.position),
    );
    let corners = quad.corners.map_or(
        [
            Vec3::new(quad.rect[0], 0.0, quad.rect[1]),
            Vec3::new(quad.rect[2], 0.0, quad.rect[1]),
            Vec3::new(quad.rect[2], 0.0, quad.rect[3]),
            Vec3::new(quad.rect[0], 0.0, quad.rect[3]),
        ],
        |points| points.map(Vec3::from_array),
    );
    for position in corners {
        if !model.transform_point3(position).is_finite() {
            return Err(invalid("transformed rect"));
        }
    }
    Ok(())
}
