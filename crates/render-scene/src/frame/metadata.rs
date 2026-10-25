//! Numerical contracts over existing executor resources. No resources are allocated here.
use wgpu::TextureFormat;
use worldrunner_render_post::{AttachmentView, FrameError, ResourceRef};

/// The meaning of stored values, independently of their texture representation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MetadataMeaning {
    /// Forward device depth in [0,1], cleared to one.
    DeviceDepth,
    /// View-space unit normal in RGB; alpha zero denotes uncovered pixels.
    ViewNormalCoverage,
    /// Linear F0 RGB and perceptual roughness alpha.
    MaterialF0Roughness,
    /// Replaceable environment-specular contribution in linear HDR radiance.
    EnvironmentSpecularLinear,
    /// Nearest positive view depth in metres, for conservative traversal.
    ConservativeDepthMeters,
    /// Averaged depth and normal, for occlusion filtering, never conservative traversal.
    AverageDepthNormalMeters,
}

/// Pixel-centre prepasses deliberately differ from forward MSAA edge coverage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MetadataSampling {
    PixelCenter,
    PerSample(u32),
    /// Minimum forward depth across the given source sample count, stored once per pixel.
    ConservativeResolve(u32),
}
impl MetadataSampling {
    pub const fn samples(self) -> u32 {
        match self {
            Self::PixelCenter | Self::ConservativeResolve(_) => 1,
            Self::PerSample(samples) => samples,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("unsupported frame metadata format, sample count or reduction")]
pub struct MetadataError;

/// A typed product retains the executor's resource identity, not a second handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameProduct {
    resource: ResourceRef,
    meaning: MetadataMeaning,
    format: TextureFormat,
    sampling: MetadataSampling,
}
impl FrameProduct {
    pub fn new(
        resource: ResourceRef,
        meaning: MetadataMeaning,
        format: TextureFormat,
        sampling: MetadataSampling,
    ) -> Result<Self, MetadataError> {
        validate_representation(meaning, format, sampling)?;
        Ok(Self {
            resource,
            meaning,
            format,
            sampling,
        })
    }
    pub const fn resource(self) -> ResourceRef {
        self.resource
    }
    pub const fn meaning(self) -> MetadataMeaning {
        self.meaning
    }
    pub const fn format(self) -> TextureFormat {
        self.format
    }
    pub const fn sampling(self) -> MetadataSampling {
        self.sampling
    }
    /// Checks the actual backing texture before admitting it to the frame graph.
    pub fn attachment(self, texture: &wgpu::Texture) -> Result<AttachmentView, FrameError> {
        if texture.format() != self.format || texture.sample_count() != self.sampling.samples() {
            return Err(FrameError::UnsupportedEffect("metadata texture contract"));
        }
        AttachmentView::new(self.resource, texture)
    }
    fn stock(name: &'static str, meaning: MetadataMeaning, format: TextureFormat) -> Self {
        Self::new(
            ResourceRef::new(name),
            meaning,
            format,
            MetadataSampling::PixelCenter,
        )
        .expect("stock metadata contract")
    }
    pub fn prepass_depth() -> Self {
        Self::stock(
            "prepass depth",
            MetadataMeaning::DeviceDepth,
            TextureFormat::Depth32Float,
        )
    }
    pub fn normals() -> Self {
        Self::stock(
            "prepass normals",
            MetadataMeaning::ViewNormalCoverage,
            TextureFormat::Rgba16Float,
        )
    }
    pub fn material() -> Self {
        Self::stock(
            "reflection material",
            MetadataMeaning::MaterialF0Roughness,
            TextureFormat::Rgba16Float,
        )
    }
    pub fn specular() -> Self {
        Self::stock(
            "environment specular",
            MetadataMeaning::EnvironmentSpecularLinear,
            TextureFormat::Rgba16Float,
        )
    }
    pub fn linear_depth(level: u32) -> Self {
        let mut product = Self::stock(
            "linear camera depth",
            MetadataMeaning::ConservativeDepthMeters,
            TextureFormat::R32Float,
        );
        product.resource.version = level;
        product
    }
    pub fn occlusion_metadata(level: u32) -> Self {
        let mut product = Self::stock(
            "averaged occlusion depth normal",
            MetadataMeaning::AverageDepthNormalMeters,
            TextureFormat::Rgba16Float,
        );
        product.resource.version = level;
        product
    }
    pub fn opaque_depth(source_samples: u32) -> Self {
        Self::resolved_depth("opaque snapshot depth", source_samples)
    }
    pub fn final_depth(source_samples: u32) -> Self {
        Self::resolved_depth("post-water depth", source_samples)
    }
    fn resolved_depth(name: &'static str, source_samples: u32) -> Self {
        Self::new(
            ResourceRef::new(name),
            MetadataMeaning::ConservativeDepthMeters,
            TextureFormat::R32Float,
            MetadataSampling::ConservativeResolve(source_samples),
        )
        .expect("stock forward depth sample count")
    }
}

/// Admits source metadata only when all products share one extent and sampling
/// domain. Resolved traversal depth is a different product and cannot be passed
/// as device depth. Producers must use the same deformation and alpha predicate.
pub fn validate_inputs(
    depth: &wgpu::Texture,
    normal: &wgpu::Texture,
    material: Option<&wgpu::Texture>,
    specular: Option<&wgpu::Texture>,
    sampling: MetadataSampling,
) -> Result<(), MetadataError> {
    let products = [
        (Some(depth), MetadataMeaning::DeviceDepth),
        (Some(normal), MetadataMeaning::ViewNormalCoverage),
        (material, MetadataMeaning::MaterialF0Roughness),
        (specular, MetadataMeaning::EnvironmentSpecularLinear),
    ];
    for (texture, meaning) in products {
        if let Some(texture) = texture {
            validate_representation(meaning, texture.format(), sampling)?;
            if texture.size() != depth.size()
                || texture.sample_count() != sampling.samples()
                || texture.dimension() != wgpu::TextureDimension::D2
                || texture.depth_or_array_layers() != 1
                || !texture
                    .usage()
                    .contains(wgpu::TextureUsages::TEXTURE_BINDING)
            {
                return Err(MetadataError);
            }
        }
    }
    Ok(())
}

fn validate_representation(
    meaning: MetadataMeaning,
    format: TextureFormat,
    sampling: MetadataSampling,
) -> Result<(), MetadataError> {
    let expected = match meaning {
        MetadataMeaning::DeviceDepth => TextureFormat::Depth32Float,
        MetadataMeaning::ConservativeDepthMeters => TextureFormat::R32Float,
        MetadataMeaning::ViewNormalCoverage
        | MetadataMeaning::MaterialF0Roughness
        | MetadataMeaning::EnvironmentSpecularLinear
        | MetadataMeaning::AverageDepthNormalMeters => TextureFormat::Rgba16Float,
    };
    if !matches!(sampling.samples(), 1 | 4) || format != expected {
        return Err(MetadataError);
    }
    if let MetadataSampling::ConservativeResolve(source_samples) = sampling
        && (!matches!(source_samples, 1 | 4) || meaning != MetadataMeaning::ConservativeDepthMeters)
    {
        return Err(MetadataError);
    }
    if matches!(
        meaning,
        MetadataMeaning::ConservativeDepthMeters | MetadataMeaning::AverageDepthNormalMeters
    ) && sampling.samples() != 1
    {
        return Err(MetadataError);
    }
    Ok(())
}
