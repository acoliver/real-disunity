use crate::PostError;

/// Scene attachment contract. Exposure and display transforms are deliberately not implicit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameConfig {
    pub size: [u32; 2],
    pub format: wgpu::TextureFormat,
    pub samples: u32,
}

impl FrameConfig {
    pub const fn new(size: [u32; 2], format: wgpu::TextureFormat, samples: u32) -> Self {
        Self {
            size,
            format,
            samples,
        }
    }

    /// Adapter capabilities are checked before any engine profile allocation.
    pub fn validate_adapter(self, adapter: &wgpu::Adapter) -> Result<(), PostError> {
        self.validate(&adapter.limits())?;
        for format in [self.format, wgpu::TextureFormat::Depth32Float] {
            let features = adapter.get_texture_format_features(format);
            if !features.flags.sample_count_supported(self.samples) {
                return Err(PostError::SampleCount(self.samples));
            }
            if !features
                .allowed_usages
                .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
            {
                return Err(PostError::UnsupportedFormat(format));
            }
            if format == self.format
                && self.samples > 1
                && !features
                    .flags
                    .contains(wgpu::TextureFormatFeatureFlags::MULTISAMPLE_RESOLVE)
            {
                return Err(PostError::UnsupportedResolve(format));
            }
        }
        Ok(())
    }

    pub fn validate(self, limits: &wgpu::Limits) -> Result<(), PostError> {
        if self.size.contains(&0) {
            return Err(PostError::EmptyExtent);
        }
        if self
            .size
            .iter()
            .any(|&side| side > limits.max_texture_dimension_2d)
        {
            return Err(PostError::TextureLimit {
                size: self.size,
                limit: limits.max_texture_dimension_2d,
            });
        }
        if !matches!(self.samples, 1 | 2 | 4 | 8 | 16) {
            return Err(PostError::SampleCount(self.samples));
        }
        if self.format.is_depth_stencil_format() {
            return Err(PostError::NonColorFormat(self.format));
        }
        let features = self
            .format
            .guaranteed_format_features(wgpu::Features::empty());
        if !features.allowed_usages.contains(
            wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
        ) || !features.flags.sample_count_supported(self.samples)
        {
            return Err(PostError::UnsupportedFormat(self.format));
        }
        if self.samples > 1
            && !features
                .flags
                .contains(wgpu::TextureFormatFeatureFlags::MULTISAMPLE_RESOLVE)
        {
            return Err(PostError::UnsupportedResolve(self.format));
        }
        if !wgpu::TextureFormat::Depth32Float
            .guaranteed_format_features(wgpu::Features::empty())
            .flags
            .sample_count_supported(self.samples)
        {
            return Err(PostError::SampleCount(self.samples));
        }
        Ok(())
    }
}

/// Retains a sampleable resolved target and, optionally, its multisampled scene attachment.
#[derive(Debug)]
pub struct FrameTargets {
    config: FrameConfig,
    texture: wgpu::Texture,
    resolved: wgpu::TextureView,
    multisampled: Option<(wgpu::Texture, wgpu::TextureView)>,
    depth: wgpu::TextureView,
}

impl FrameTargets {
    pub fn new(device: &wgpu::Device, config: FrameConfig) -> Result<Self, PostError> {
        config.validate(&device.limits())?;
        let texture = color_texture(device, "post resolved scene", config.size, config.format);
        let resolved = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let multisampled = (config.samples > 1).then(|| {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("post multisampled scene"),
                size: extent(config.size),
                mip_level_count: 1,
                sample_count: config.samples,
                dimension: wgpu::TextureDimension::D2,
                format: config.format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            (texture, view)
        });
        let depth = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("frame depth"),
                size: extent(config.size),
                mip_level_count: 1,
                sample_count: config.samples,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Depth32Float,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor::default());
        Ok(Self {
            config,
            texture,
            resolved,
            multisampled,
            depth,
        })
    }

    pub const fn depth(&self) -> &wgpu::TextureView {
        &self.depth
    }

    /// Resolve retained scene samples without altering their values.
    pub fn encode_resolve(&self, encoder: &mut wgpu::CommandEncoder) {
        self.encode_resolve_timed(encoder, None);
    }

    pub(crate) fn encode_resolve_timed(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        stamp: Option<&crate::timing::Stamp>,
    ) {
        if let Some(resolve) = self.resolve() {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("frame MSAA resolve"),
                timestamp_writes: stamp.map(crate::timing::Stamp::render),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: self.scene(),
                    resolve_target: Some(resolve),
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
        }
    }

    pub const fn config(&self) -> FrameConfig {
        self.config
    }
    pub const fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }
    pub const fn resolved(&self) -> &wgpu::TextureView {
        &self.resolved
    }
    pub const fn scene(&self) -> &wgpu::TextureView {
        match &self.multisampled {
            Some((_, view)) => view,
            None => &self.resolved,
        }
    }
    pub const fn resolve(&self) -> Option<&wgpu::TextureView> {
        match self.multisampled {
            Some(_) => Some(&self.resolved),
            None => None,
        }
    }
}

/// Common single-sample color allocation for authored intermediate and output resources.
/// Callers validate their extents before allocation (frame chains do this on construction/resize).
pub fn color_texture(
    device: &wgpu::Device,
    label: &str,
    size: [u32; 2],
    format: wgpu::TextureFormat,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: extent(size),
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

pub(crate) const fn extent(size: [u32; 2]) -> wgpu::Extent3d {
    wgpu::Extent3d {
        width: size[0],
        height: size[1],
        depth_or_array_layers: 1,
    }
}
