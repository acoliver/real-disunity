use crate::PostError;

/// Scene attachment contract. Exposure and display transforms are deliberately not implicit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameConfig {
    pub size: [u32; 2],
    /// Presentation extent; `None` uses the scene extent.
    pub output_size: Option<[u32; 2]>,
    pub format: wgpu::TextureFormat,
    pub samples: u32,
}

impl FrameConfig {
    pub const fn new(size: [u32; 2], format: wgpu::TextureFormat, samples: u32) -> Self {
        Self {
            size,
            output_size: None,
            format,
            samples,
        }
    }

    /// Set an explicit internal extent while retaining the original output extent.
    #[must_use]
    pub fn with_render_extent(mut self, size: [u32; 2]) -> Self {
        self.output_size = Some(self.output_extent());
        self.size = size;
        self
    }

    pub const fn output_extent(self) -> [u32; 2] {
        match self.output_size {
            Some(size) => size,
            None => self.size,
        }
    }

    /// Adapter capabilities are checked before any engine profile allocation.
    pub fn validate_adapter(self, adapter: &wgpu::Adapter) -> Result<(), PostError> {
        self.validate_extended(&adapter.limits())?;
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

    /// Structural checks plus the sample counts every device guarantees (1 and 4).
    pub fn validate(self, limits: &wgpu::Limits) -> Result<(), PostError> {
        self.validate_with(limits, false)
    }

    /// As [`Self::validate`], admitting 2 and 8 samples only when the device enabled
    /// `TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES`. Whether the adapter supports the
    /// count for this format is answered by [`Self::validate_adapter`].
    pub fn validate_device(self, device: &wgpu::Device) -> Result<(), PostError> {
        let extended = device
            .features()
            .contains(wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES);
        self.validate_with(&device.limits(), extended)
    }

    /// Checks that defer sample-count capability to adapter queries.
    pub(crate) fn validate_extended(self, limits: &wgpu::Limits) -> Result<(), PostError> {
        self.validate_with(limits, true)
    }

    fn validate_with(self, limits: &wgpu::Limits, extended_samples: bool) -> Result<(), PostError> {
        let output = self.output_extent();
        if self.size.contains(&0) || output.contains(&0) {
            return Err(PostError::EmptyExtent);
        }
        if output
            .iter()
            .zip(self.size)
            .any(|(&out, render)| out < render)
        {
            return Err(PostError::RenderExtent);
        }
        if output
            .iter()
            .any(|&side| side > limits.max_texture_dimension_2d)
        {
            return Err(PostError::TextureLimit {
                size: output,
                limit: limits.max_texture_dimension_2d,
            });
        }
        if !matches!(self.samples, 1 | 2 | 4 | 8) {
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
        ) || !(extended_samples || features.flags.sample_count_supported(self.samples))
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
        if !extended_samples
            && !wgpu::TextureFormat::Depth32Float
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
    velocity: Option<(wgpu::Texture, wgpu::TextureView)>,
    temporal_mask: Option<(wgpu::Texture, wgpu::TextureView)>,
}

/// Optional attachments beside the resolved colour and depth.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TargetOptions {
    /// The object-motion residual attachment for a TAA resolve.
    pub velocity: bool,
    /// The reactive and composition mask of temporal reconstruction.
    pub temporal_mask: bool,
}

/// Format of the temporal mask: reactive in the red channel, composition in the green channel,
/// both in `[0, 1]`.
pub const TEMPORAL_MASK_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rg8Unorm;

/// Format of the object-motion residual attachment, in uv units of the previous frame.
pub const VELOCITY_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rg16Float;

impl FrameTargets {
    pub fn new(device: &wgpu::Device, config: FrameConfig) -> Result<Self, PostError> {
        Self::with_velocity(device, config, false)
    }

    /// As [`Self::new`], also allocating the velocity attachment when `velocity` is true.
    ///
    /// The attachment sits beside the depth: same extent and sample count, never resolved
    /// (averaging the residuals of two objects has no meaning), and the TAA resolve reads it
    /// per sample. A host clears it to zero, which means "camera reprojection only".
    pub fn with_velocity(
        device: &wgpu::Device,
        config: FrameConfig,
        velocity: bool,
    ) -> Result<Self, PostError> {
        Self::with_options(
            device,
            config,
            TargetOptions {
                velocity,
                temporal_mask: false,
            },
        )
    }

    /// As [`Self::new`], with the optional attachments `options` selects.
    pub fn with_options(
        device: &wgpu::Device,
        config: FrameConfig,
        options: TargetOptions,
    ) -> Result<Self, PostError> {
        config.validate_device(device)?;
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
        let velocity = options.velocity.then(|| {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("frame velocity"),
                size: extent(config.size),
                mip_level_count: 1,
                sample_count: config.samples,
                dimension: wgpu::TextureDimension::D2,
                format: VELOCITY_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            (texture, view)
        });
        let temporal_mask = options
            .temporal_mask
            .then(|| mask_texture(device, config.size));
        Ok(Self {
            config,
            texture,
            resolved,
            multisampled,
            depth,
            velocity,
            temporal_mask,
        })
    }

    /// The temporal mask, present only for targets built with `TargetOptions::temporal_mask`.
    pub fn temporal_mask(&self) -> Option<&wgpu::TextureView> {
        self.temporal_mask.as_ref().map(|(_, view)| view)
    }

    /// The texture behind [`Self::temporal_mask`].
    pub fn temporal_mask_texture(&self) -> Option<&wgpu::Texture> {
        self.temporal_mask.as_ref().map(|(texture, _)| texture)
    }

    /// Check a host-supplied mask texture against the internal extent of `config`: one sample,
    /// [`TEMPORAL_MASK_FORMAT`] and exactly the internal extent. Nothing is encoded.
    pub fn validate_temporal_mask(
        config: FrameConfig,
        texture: &wgpu::Texture,
    ) -> Result<(), PostError> {
        if texture.sample_count() != 1 {
            return Err(PostError::SampleCount(texture.sample_count()));
        }
        if texture.format() != TEMPORAL_MASK_FORMAT {
            return Err(PostError::UnsupportedFormat(texture.format()));
        }
        let actual = [texture.width(), texture.height()];
        if actual != config.size {
            return Err(PostError::MaskExtent {
                expected: config.size,
                actual,
            });
        }
        Ok(())
    }

    /// The velocity attachment, present only for targets built for a TAA resolve.
    pub fn velocity(&self) -> Option<&wgpu::TextureView> {
        self.velocity.as_ref().map(|(_, view)| view)
    }

    /// The texture behind [`Self::velocity`].
    pub fn velocity_texture(&self) -> Option<&wgpu::Texture> {
        self.velocity.as_ref().map(|(texture, _)| texture)
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

fn mask_texture(device: &wgpu::Device, size: [u32; 2]) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = color_texture(device, "frame temporal mask", size, TEMPORAL_MASK_FORMAT);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
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
