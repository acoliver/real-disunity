use crate::{DEPTH_FORMAT, ReceiverDimension, begin_depth_pass};

/// Caller-owned resource ceilings, independent of scene or quality policy.
#[derive(Clone, Copy, Debug)]
pub struct DepthMapBudget {
    pub max_resolution: u32,
    pub max_layers: u32,
    pub max_texture_bytes: u64,
    /// Ceiling for one padded layer copy, not an unbounded queue of copies.
    pub max_readback_bytes: u64,
}

/// A square depth map. Receiver dimension is explicit even for a one-layer array.
#[derive(Clone, Copy, Debug)]
pub struct DepthMapDescriptor<'a> {
    pub label: Option<&'a str>,
    pub resolution: u32,
    pub layers: u32,
    pub receiver_dimension: ReceiverDimension,
    /// Enables copy usage; copy size and budget are checked only when requested.
    pub readback: bool,
    pub budget: DepthMapBudget,
}

/// Invalid requests are rejected before creating GPU resources or encoding copies.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DepthMapError {
    #[error("depth resolution {requested} is outside 1..={limit}")]
    Resolution { requested: u32, limit: u32 },
    #[error("depth layers {requested} is outside 1..={limit}")]
    Layers { requested: u32, limit: u32 },
    #[error("a D2 receiver requires exactly one layer")]
    ReceiverLayers,
    #[error("depth texture requires {required} bytes, budget is {budget}")]
    TextureBudget { required: u64, budget: u64 },
    #[error("depth readback requires {required} bytes, limit is {limit}")]
    ReadbackBudget { required: u64, limit: u64 },
    #[error("depth resource size arithmetic overflow")]
    SizeOverflow,
    #[error("depth layer {requested} is outside 0..{layers}")]
    LayerIndex { requested: u32, layers: u32 },
    #[error("this depth map was created without readback usage")]
    ReadbackDisabled,
}

impl DepthMapDescriptor<'_> {
    /// Checks texture dimensions and nominal texture bytes, not optional copy limits.
    /// A valid texture may be too large to read back on the device.
    pub fn validate(&self, limits: &wgpu::Limits) -> Result<(), DepthMapError> {
        let limit = self
            .budget
            .max_resolution
            .min(limits.max_texture_dimension_2d);
        if self.resolution == 0 || self.resolution > limit {
            return Err(DepthMapError::Resolution {
                requested: self.resolution,
                limit,
            });
        }
        let limit = self.budget.max_layers.min(limits.max_texture_array_layers);
        if self.layers == 0 || self.layers > limit {
            return Err(DepthMapError::Layers {
                requested: self.layers,
                limit,
            });
        }
        if self.receiver_dimension == ReceiverDimension::D2 && self.layers != 1 {
            return Err(DepthMapError::ReceiverLayers);
        }
        let required = u64::from(self.resolution)
            .checked_mul(u64::from(self.resolution))
            .and_then(|size| size.checked_mul(u64::from(self.layers)))
            .and_then(|size| size.checked_mul(4))
            .ok_or(DepthMapError::SizeOverflow)?;
        if required > self.budget.max_texture_bytes {
            return Err(DepthMapError::TextureBudget {
                required,
                budget: self.budget.max_texture_bytes,
            });
        }
        Ok(())
    }

    /// Optional CPU preflight for one padded layer copy, including texture validation.
    /// Allocation does not require this check; each actual copy enforces it independently.
    pub fn validate_readback(&self, limits: &wgpu::Limits) -> Result<(), DepthMapError> {
        self.validate(limits)?;
        if !self.readback {
            return Err(DepthMapError::ReadbackDisabled);
        }
        bounded_readback_layout(
            self.resolution,
            self.budget.max_readback_bytes.min(limits.max_buffer_size),
        )?;
        Ok(())
    }
}

fn bounded_readback_layout(resolution: u32, limit: u64) -> Result<(u32, u64), DepthMapError> {
    let (bytes_per_row, required) = readback_layout(resolution)?;
    if required > limit {
        return Err(DepthMapError::ReadbackBudget { required, limit });
    }
    Ok((bytes_per_row, required))
}

fn readback_layout(resolution: u32) -> Result<(u32, u64), DepthMapError> {
    let alignment = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let bytes = resolution
        .checked_mul(4)
        .ok_or(DepthMapError::SizeOverflow)?;
    let padded = bytes
        .checked_add(alignment - 1)
        .ok_or(DepthMapError::SizeOverflow)?
        / alignment
        * alignment;
    let size = u64::from(padded)
        .checked_mul(u64::from(resolution))
        .ok_or(DepthMapError::SizeOverflow)?;
    Ok((padded, size))
}

/// Retained texture, explicit receiver view and independent per-layer attachments.
#[derive(Debug)]
pub struct DepthMap {
    texture: wgpu::Texture,
    receiver_view: wgpu::TextureView,
    layer_views: Vec<wgpu::TextureView>,
    resolution: u32,
    layers: u32,
    readback: bool,
    max_readback_bytes: u64,
}

impl DepthMap {
    pub fn new(
        device: &wgpu::Device,
        descriptor: DepthMapDescriptor<'_>,
    ) -> Result<Self, DepthMapError> {
        descriptor.validate(&device.limits())?;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: descriptor.label,
            size: wgpu::Extent3d {
                width: descriptor.resolution,
                height: descriptor.resolution,
                depth_or_array_layers: descriptor.layers,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | if descriptor.readback {
                    wgpu::TextureUsages::COPY_SRC
                } else {
                    wgpu::TextureUsages::empty()
                },
            view_formats: &[],
        });
        let receiver_view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: descriptor.label,
            dimension: Some(descriptor.receiver_dimension.into()),
            array_layer_count: Some(descriptor.layers),
            ..Default::default()
        });
        let layer_views = (0..descriptor.layers)
            .map(|layer| {
                texture.create_view(&wgpu::TextureViewDescriptor {
                    label: descriptor.label,
                    dimension: Some(wgpu::TextureViewDimension::D2),
                    base_array_layer: layer,
                    array_layer_count: Some(1),
                    ..Default::default()
                })
            })
            .collect();
        Ok(Self {
            texture,
            receiver_view,
            layer_views,
            resolution: descriptor.resolution,
            layers: descriptor.layers,
            readback: descriptor.readback,
            max_readback_bytes: descriptor.budget.max_readback_bytes,
        })
    }

    pub const fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }
    pub const fn receiver_view(&self) -> &wgpu::TextureView {
        &self.receiver_view
    }
    pub const fn resolution(&self) -> u32 {
        self.resolution
    }
    pub const fn layers(&self) -> u32 {
        self.layers
    }

    pub fn layer_view(&self, layer: u32) -> Result<&wgpu::TextureView, DepthMapError> {
        self.layer_views
            .get(layer as usize)
            .ok_or(DepthMapError::LayerIndex {
                requested: layer,
                layers: self.layers,
            })
    }

    pub fn begin_layer_pass<'a>(
        &self,
        encoder: &'a mut wgpu::CommandEncoder,
        layer: u32,
        label: Option<&str>,
    ) -> Result<wgpu::RenderPass<'a>, DepthMapError> {
        Ok(begin_depth_pass(encoder, self.layer_view(layer)?, label))
    }

    /// Encodes one bounded layer copy. Submit the encoder BEFORE mapping the buffer.
    /// The caller owns mapping, polling, unmapping and the number of outstanding copies.
    /// Checks padded size against the retained budget and device limit before allocating.
    pub fn copy_layer_to_readback(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        layer: u32,
    ) -> Result<DepthReadback, DepthMapError> {
        self.layer_view(layer)?;
        if !self.readback {
            return Err(DepthMapError::ReadbackDisabled);
        }
        let (bytes_per_row, size) = bounded_readback_layout(
            self.resolution,
            self.max_readback_bytes.min(device.limits().max_buffer_size),
        )?;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("depth layer readback"),
            size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: 0,
                    y: 0,
                    z: layer,
                },
                aspect: wgpu::TextureAspect::DepthOnly,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row),
                    rows_per_image: None,
                },
            },
            wgpu::Extent3d {
                width: self.resolution,
                height: self.resolution,
                depth_or_array_layers: 1,
            },
        );
        Ok(DepthReadback {
            buffer,
            bytes_per_row,
            resolution: self.resolution,
        })
    }
}

/// One layer's asynchronous readback storage and padded-row interpretation.
#[derive(Debug)]
pub struct DepthReadback {
    pub buffer: wgpu::Buffer,
    pub bytes_per_row: u32,
    pub resolution: u32,
}
