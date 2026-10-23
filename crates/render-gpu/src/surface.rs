//! Surface policy without a dependency on a particular window system.
use thiserror::Error;

mod presenter;
mod transition;
pub use presenter::Presenter;
pub use transition::{SurfaceAction, SurfaceTransition};

#[derive(Debug, Error)]
pub enum SurfaceError {
    #[error("surface configuration failed: {0}")]
    Configuration(String),
    #[error("surface is not configured after a failed configuration")]
    Unconfigured,
    #[error("surface validation failed")]
    Validation,
    #[error("surface has no supported color format")]
    Format,
    #[error("surface extent {size:?} exceeds device limit {limit}")]
    Extent { size: [u32; 2], limit: u32 },
    #[error("surface does not support configured format {0:?}")]
    UnsupportedFormat(wgpu::TextureFormat),
    #[error("surface does not support configured usages {0:?}")]
    UnsupportedUsage(wgpu::TextureUsages),
    #[error("surface does not support configured present mode {0:?}")]
    UnsupportedPresentMode(wgpu::PresentMode),
    #[error("surface does not support configured alpha mode {0:?}")]
    UnsupportedAlphaMode(wgpu::CompositeAlphaMode),
}

#[derive(Debug)]
pub enum AcquiredFrame {
    Frame(wgpu::SurfaceTexture),
    Skip,
    Reconfigure,
    Recreate,
}

/// Suboptimal frames remain usable; dropping them can starve presentation indefinitely.
pub fn classify(acquired: wgpu::CurrentSurfaceTexture) -> Result<AcquiredFrame, SurfaceError> {
    Ok(match acquired {
        wgpu::CurrentSurfaceTexture::Success(frame)
        | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => AcquiredFrame::Frame(frame),
        wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
            AcquiredFrame::Skip
        }
        wgpu::CurrentSurfaceTexture::Outdated => AcquiredFrame::Reconfigure,
        wgpu::CurrentSurfaceTexture::Lost => AcquiredFrame::Recreate,
        wgpu::CurrentSurfaceTexture::Validation => return Err(SurfaceError::Validation),
    })
}

pub fn validate_extent(size: [u32; 2], limit: u32) -> Result<bool, SurfaceError> {
    if size.contains(&0) {
        return Ok(false);
    }
    if size.iter().any(|&side| side > limit) {
        return Err(SurfaceError::Extent { size, limit });
    }
    Ok(true)
}

/// A zero-area surface is suspended, never silently configured to a different size.
/// Returns `false` when the caller should keep the surface suspended.
pub fn configure(
    surface: &wgpu::Surface<'_>,
    device: &wgpu::Device,
    config: &wgpu::SurfaceConfiguration,
) -> Result<bool, SurfaceError> {
    if !validate_extent(
        [config.width, config.height],
        device.limits().max_texture_dimension_2d,
    )? {
        return Ok(false);
    }
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    surface.configure(device, config);
    if let Some(error) = pollster::block_on(scope.pop()) {
        return Err(SurfaceError::Configuration(error.to_string()));
    }
    Ok(true)
}

/// Validates an explicit presentation policy against the actual surface capabilities and builds
/// its configuration. Callers own policy (format preference, latency, and present mode); the
/// shared service owns capability checking and the concrete wgpu configuration.
#[derive(Clone, Copy, Debug)]
pub struct SurfacePolicy {
    pub format: wgpu::TextureFormat,
    pub usage: wgpu::TextureUsages,
    pub size: [u32; 2],
    pub present_mode: wgpu::PresentMode,
    pub desired_maximum_frame_latency: u32,
    pub alpha_mode: wgpu::CompositeAlphaMode,
}

pub fn configuration(
    capabilities: &wgpu::SurfaceCapabilities,
    policy: SurfacePolicy,
) -> Result<wgpu::SurfaceConfiguration, SurfaceError> {
    let SurfacePolicy {
        format,
        usage,
        size,
        present_mode,
        desired_maximum_frame_latency,
        alpha_mode,
    } = policy;
    if !capabilities.formats.contains(&format) {
        return Err(SurfaceError::UnsupportedFormat(format));
    }
    if !capabilities.usages.contains(usage) {
        return Err(SurfaceError::UnsupportedUsage(
            usage.difference(capabilities.usages),
        ));
    }
    if !matches!(
        present_mode,
        wgpu::PresentMode::AutoVsync | wgpu::PresentMode::AutoNoVsync
    ) && !capabilities.present_modes.contains(&present_mode)
    {
        return Err(SurfaceError::UnsupportedPresentMode(present_mode));
    }
    let automatic_alpha = alpha_mode == wgpu::CompositeAlphaMode::Auto
        && capabilities.alpha_modes.iter().any(|mode| {
            matches!(
                mode,
                wgpu::CompositeAlphaMode::Opaque | wgpu::CompositeAlphaMode::Inherit
            )
        });
    if !automatic_alpha && !capabilities.alpha_modes.contains(&alpha_mode) {
        return Err(SurfaceError::UnsupportedAlphaMode(alpha_mode));
    }
    Ok(wgpu::SurfaceConfiguration {
        usage,
        format,
        width: size[0],
        height: size[1],
        present_mode,
        desired_maximum_frame_latency,
        alpha_mode,
        color_space: wgpu::SurfaceColorSpace::Auto,
        view_formats: vec![format.add_srgb_suffix()],
    })
}

#[derive(Clone, Copy, Debug)]
pub enum FormatPreference {
    Srgb,
    SurfaceOrder,
}

#[derive(Clone, Copy, Debug)]
pub struct SurfaceFormat {
    pub format: wgpu::TextureFormat,
    pub capture_supported: bool,
}

/// Preserve the caller's color-space policy while sharing format/capture capability checks.
pub fn select_format(
    capabilities: &wgpu::SurfaceCapabilities,
    preference: FormatPreference,
) -> Result<SurfaceFormat, SurfaceError> {
    use wgpu::TextureFormat::{Bgra8Unorm, Bgra8UnormSrgb, Rgba8Unorm, Rgba8UnormSrgb};
    let format = match preference {
        FormatPreference::Srgb => capabilities
            .formats
            .iter()
            .find(|f| matches!(f, Bgra8UnormSrgb | Rgba8UnormSrgb)),
        FormatPreference::SurfaceOrder => capabilities
            .formats
            .iter()
            .find(|f| matches!(f, Bgra8Unorm | Bgra8UnormSrgb | Rgba8Unorm | Rgba8UnormSrgb))
            .or_else(|| capabilities.formats.first()),
    }
    .copied()
    .ok_or(SurfaceError::Format)?;
    Ok(SurfaceFormat {
        format,
        capture_supported: capabilities.usages.contains(wgpu::TextureUsages::COPY_SRC)
            && matches!(
                format,
                Bgra8Unorm | Bgra8UnormSrgb | Rgba8Unorm | Rgba8UnormSrgb
            ),
    })
}

/// Converts padded RGBA/BGRA rows to RGBA without re-encoding their transfer function.
pub fn unpack_rgba8(
    format: wgpu::TextureFormat,
    size: [u32; 2],
    pitch: u32,
    bytes: &[u8],
) -> Option<Vec<u8>> {
    use wgpu::TextureFormat::{Bgra8Unorm, Bgra8UnormSrgb, Rgba8Unorm, Rgba8UnormSrgb};
    if !matches!(
        format,
        Bgra8Unorm | Bgra8UnormSrgb | Rgba8Unorm | Rgba8UnormSrgb
    ) || size.contains(&0)
    {
        return None;
    }
    let row_bytes = usize::try_from(size[0]).ok()?.checked_mul(4)?;
    let pitch = usize::try_from(pitch).ok()?;
    let height = usize::try_from(size[1]).ok()?;
    if pitch < row_bytes || bytes.len() < pitch.checked_mul(height)? {
        return None;
    }
    let mut result = Vec::new();
    result
        .try_reserve_exact(row_bytes.checked_mul(height)?)
        .ok()?;
    for row in bytes.chunks_exact(pitch).take(height) {
        result.extend_from_slice(&row[..row_bytes]);
    }
    if matches!(format, Bgra8Unorm | Bgra8UnormSrgb) {
        for pixel in result.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
    }
    Some(result)
}
