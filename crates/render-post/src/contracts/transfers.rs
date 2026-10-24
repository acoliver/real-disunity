use super::{AccessMode, BufferView, CheckedView, ContractError, ResourceUse, UseKind};

#[derive(Clone, Debug)]
pub struct TextureCopy {
    pub view: CheckedView,
    pub origin: [u32; 2],
    pub extent: [u32; 2],
    /// Partial destination updates must consume initialized prior contents.
    pub before: Option<u32>,
}
impl TextureCopy {
    pub fn whole(view: CheckedView) -> Self {
        let mip = view.range.mips.start;
        let extent = physical_extent(view.view.texture(), mip);
        Self {
            view,
            origin: [0, 0],
            extent,
            before: None,
        }
    }
    pub(super) fn info(&self) -> wgpu::TexelCopyTextureInfo<'_> {
        wgpu::TexelCopyTextureInfo {
            texture: self.view.view.texture(),
            mip_level: self.view.range.mips.start,
            origin: wgpu::Origin3d {
                x: self.origin[0],
                y: self.origin[1],
                z: self.view.range.layers.start,
            },
            aspect: self.view.range.aspect,
        }
    }
    pub(super) fn size(&self) -> wgpu::Extent3d {
        wgpu::Extent3d {
            width: self.extent[0],
            height: self.extent[1],
            depth_or_array_layers: self.view.range.layers.end - self.view.range.layers.start,
        }
    }
    pub(super) fn usage(&self, read: bool) -> Result<ResourceUse, ContractError> {
        self.validate()?;
        if read {
            return Ok(ResourceUse::read(self.view.clone(), UseKind::Copy));
        }
        let full = Self::whole(self.view.clone());
        if self.origin != [0, 0] || self.extent != full.extent {
            let before = self
                .before
                .ok_or(ContractError::Producer(self.view.allocation))?;
            Ok(ResourceUse::read_write(
                self.view.clone(),
                before,
                UseKind::Copy,
            ))
        } else {
            Ok(ResourceUse::write(self.view.clone(), UseKind::Copy))
        }
    }
    pub(super) fn validate(&self) -> Result<(), ContractError> {
        let texture = self.view.view.texture();
        let mip = self.view.range.mips.start;
        let bounds = physical_extent(texture, mip);
        if self.view.range.mips.len() != 1
            || texture.sample_count() != 1
            || self.extent.contains(&0)
            || (texture.format().is_depth_stencil_format()
                && (self.origin != [0, 0] || self.extent != bounds))
            || self
                .origin
                .iter()
                .zip(self.extent)
                .zip(bounds)
                .any(|((&origin, size), bound)| {
                    origin.checked_add(size).is_none_or(|end| end > bound)
                })
        {
            return Err(ContractError::CopyExtent(self.view.allocation));
        }
        let (bw, bh) = texture.format().block_dimensions();
        if !self.origin[0].is_multiple_of(bw)
            || !self.origin[1].is_multiple_of(bh)
            || !self.extent[0].is_multiple_of(bw)
            || !self.extent[1].is_multiple_of(bh)
        {
            return Err(ContractError::CopyAlignment(self.view.allocation));
        }
        Ok(())
    }
}
pub(super) fn buffer_copy(
    source: &BufferView,
    destination: &BufferView,
) -> Result<(), ContractError> {
    if source.range.end - source.range.start != destination.range.end - destination.range.start {
        return Err(ContractError::CopyExtent(destination.allocation));
    }
    for view in [source, destination] {
        if !view.range.start.is_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT)
            || !(view.range.end - view.range.start).is_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT)
        {
            return Err(ContractError::CopyAlignment(view.allocation));
        }
    }
    // WebGPU disallows buffer-to-buffer copies within one backing buffer.
    if source.allocation == destination.allocation {
        return Err(ContractError::Hazard(source.allocation));
    }
    Ok(())
}
pub(super) fn texture_copy(
    source: &TextureCopy,
    destination: &TextureCopy,
) -> Result<(), ContractError> {
    source.validate()?;
    destination.validate()?;
    let sf = source.view.view.texture().format();
    let df = destination.view.view.texture().format();
    if source.size() != destination.size()
        || sf.remove_srgb_suffix() != df.remove_srgb_suffix()
        || !all_aspects(sf, source.view.range.aspect)
        || !all_aspects(df, destination.view.range.aspect)
    {
        return Err(ContractError::CopyExtent(destination.view.allocation));
    }
    Ok(())
}
pub(super) fn texture_buffer(
    copy: &TextureCopy,
    buffer: &BufferView,
    pitch: u32,
    rows: u32,
    read_texture: bool,
) -> Result<Vec<BufferView>, ContractError> {
    copy.validate()?;
    validate_direction(copy, read_texture)?;
    let format = copy.view.view.texture().format();
    let block = format
        .block_copy_size(Some(copy.view.range.aspect))
        .ok_or(ContractError::CopyExtent(copy.view.allocation))?;
    let (bw, bh) = format.block_dimensions();
    let width = copy.extent[0].div_ceil(bw);
    let height = copy.extent[1].div_ceil(bh);
    let alignment = if format.is_depth_stencil_format() {
        4
    } else {
        u64::from(block)
    };
    if !pitch.is_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
        || !buffer.range.start.is_multiple_of(alignment)
    {
        return Err(ContractError::CopyAlignment(buffer.allocation));
    }
    let row_bytes = u64::from(width) * u64::from(block);
    let length = u64::from(pitch)
        .checked_mul(u64::from(rows))
        .and_then(|v| v.checked_mul(u64::from(copy.size().depth_or_array_layers - 1)))
        .and_then(|v| v.checked_add(u64::from(pitch) * u64::from(height - 1)))
        .and_then(|v| v.checked_add(row_bytes))
        .ok_or(ContractError::CopyExtent(buffer.allocation))?;
    if u64::from(pitch) < row_bytes
        || rows < height
        || length > buffer.range.end - buffer.range.start
    {
        return Err(ContractError::CopyExtent(buffer.allocation));
    }
    let mut spans = Vec::new();
    for layer in 0..copy.size().depth_or_array_layers {
        for row in 0..height {
            let start = buffer.range.start
                + (u64::from(layer) * u64::from(rows) + u64::from(row)) * u64::from(pitch);
            let mut span = buffer.clone();
            span.range = start..start + row_bytes;
            spans.push(span);
        }
    }
    Ok(spans)
}
fn physical_extent(texture: &wgpu::Texture, mip: u32) -> [u32; 2] {
    let (bw, bh) = texture.format().block_dimensions();
    [
        (texture.width() >> mip).max(1).div_ceil(bw) * bw,
        (texture.height() >> mip).max(1).div_ceil(bh) * bh,
    ]
}
fn all_aspects(format: wgpu::TextureFormat, aspect: wgpu::TextureAspect) -> bool {
    match aspect {
        wgpu::TextureAspect::All => true,
        wgpu::TextureAspect::DepthOnly => format.has_depth_aspect() && !format.has_stencil_aspect(),
        wgpu::TextureAspect::StencilOnly => {
            format.has_stencil_aspect() && !format.has_depth_aspect()
        }
        _ => false,
    }
}
fn validate_direction(copy: &TextureCopy, read: bool) -> Result<(), ContractError> {
    use wgpu::{TextureAspect as A, TextureFormat as F};
    let format = copy.view.view.texture().format();
    let aspect = copy.view.range.aspect;
    let forbidden = if read {
        matches!(
            (format, aspect),
            (F::Depth24Plus, _) | (F::Depth24PlusStencil8, A::DepthOnly)
        )
    } else {
        matches!(
            (format, aspect),
            (F::Depth24Plus | F::Depth32Float, _)
                | (
                    F::Depth24PlusStencil8 | F::Depth32FloatStencil8,
                    A::DepthOnly
                )
        )
    };
    if forbidden {
        return Err(ContractError::CopyDirection {
            allocation: copy.view.allocation,
            read,
        });
    }
    Ok(())
}
pub(super) fn buffer_info(
    view: &BufferView,
    pitch: u32,
    rows: u32,
) -> wgpu::TexelCopyBufferInfo<'_> {
    wgpu::TexelCopyBufferInfo {
        buffer: &view.buffer,
        layout: wgpu::TexelCopyBufferLayout {
            offset: view.range.start,
            bytes_per_row: Some(pitch),
            rows_per_image: Some(rows),
        },
    }
}
pub(super) fn is_write(access: AccessMode) -> bool {
    access != AccessMode::Read
}
