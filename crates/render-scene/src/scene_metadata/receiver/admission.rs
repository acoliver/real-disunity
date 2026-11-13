use super::ReceiverLayout;
use crate::renderer::depth::DepthMode;
use crate::terrain::program::{self, Screen, Surface};
use worldrunner_render_post::FrameError;

pub(super) fn validate(
    limits: &wgpu::Limits,
    layout: ReceiverLayout,
    metadata_colors: u32,
    terrain: Option<Surface>,
) -> Result<(), FrameError> {
    layout
        .validate(limits, metadata_colors)
        .map_err(FrameError::UnsupportedEffect)?;
    let required = program::forward_sampled_textures(terrain, screen(metadata_colors), true);
    if required > limits.max_sampled_textures_per_shader_stage {
        return Err(FrameError::UnsupportedEffect(
            "receiver forward sampled texture count",
        ));
    }
    Ok(())
}

/// Whether object relief cards may sample the Opaque depth they are tested against. The
/// receiver stays admitted when this is false; the cards then keep the plain consumer.
pub(super) fn card_reject(
    limits: &wgpu::Limits,
    backend: wgpu::Backend,
    metadata_colors: u32,
) -> bool {
    // GL cannot sample a depth texture that is also the current read-only attachment.
    backend != wgpu::Backend::Gl
        && program::forward_sampled_textures(None, screen(metadata_colors), true)
            < limits.max_sampled_textures_per_shader_stage
}

/// Whether a frame's colour pass draws with the card reject: the device admitted it and the
/// pass reuses the prepass depth, the only mode that attaches it read-only. Shared,
/// independent and multisampled depth never qualify.
pub(super) fn selects_card_reject(admitted: bool, depth: DepthMode) -> bool {
    admitted && depth == DepthMode::Reuse
}

/// Metadata color targets the receiver consumer reads: merged output adds the reflection target.
pub(super) fn metadata_colors(output: super::super::MetadataOutput) -> u32 {
    if output == super::super::MetadataOutput::Merged {
        2
    } else {
        1
    }
}

fn screen(metadata_colors: u32) -> Screen {
    if metadata_colors == 2 {
        Screen::Reflection
    } else {
        Screen::Ambient
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paired_layered_reflections_admit_the_complete_fragment_texture_context() {
        let mut limits = wgpu::Limits {
            max_sampled_textures_per_shader_stage: 16,
            max_color_attachments: 8,
            max_color_attachment_bytes_per_sample: 64,
            ..Default::default()
        };
        assert_eq!(
            program::forward_sampled_textures(Some(Surface::Layered), Screen::Reflection, true),
            17
        );
        assert!(matches!(
            validate(&limits, ReceiverLayout::Opposing, 2, Some(Surface::Layered)),
            Err(FrameError::UnsupportedEffect(
                "receiver forward sampled texture count"
            ))
        ));
        limits.max_sampled_textures_per_shader_stage = 17;
        assert!(validate(&limits, ReceiverLayout::Opposing, 2, Some(Surface::Layered)).is_ok());
        limits.max_sampled_textures_per_shader_stage = 16;
        for surface in [
            None,
            Some(Surface::Plain),
            Some(Surface::Pages),
            Some(Surface::LayeredPages),
        ] {
            assert!(validate(&limits, ReceiverLayout::Opposing, 2, surface).is_ok());
        }
        assert!(validate(&limits, ReceiverLayout::Opposing, 1, Some(Surface::Layered)).is_ok());
        assert_eq!(
            program::forward_sampled_textures(Some(Surface::Layered), Screen::Reflection, false),
            15
        );
        assert_eq!(
            program::forward_sampled_textures(None, Screen::Reflection, true),
            15
        );
    }

    fn card_limits(textures: u32) -> wgpu::Limits {
        wgpu::Limits {
            max_sampled_textures_per_shader_stage: textures,
            max_color_attachments: 8,
            max_color_attachment_bytes_per_sample: 64,
            ..Default::default()
        }
    }

    #[test]
    fn the_card_reject_needs_a_sampling_backend_and_one_more_texture() {
        assert!(card_reject(&card_limits(16), wgpu::Backend::Vulkan, 2));
        assert!(card_reject(&card_limits(16), wgpu::Backend::Metal, 2));
        assert!(!card_reject(&card_limits(16), wgpu::Backend::Gl, 2));
        assert!(!card_reject(&card_limits(15), wgpu::Backend::Vulkan, 2));
    }

    #[test]
    fn only_reused_depth_selects_the_card_reject() {
        assert!(selects_card_reject(true, DepthMode::Reuse));
        for depth in [
            DepthMode::Independent,
            DepthMode::Shared,
            DepthMode::Prepass,
        ] {
            assert!(!selects_card_reject(true, depth));
        }
        assert!(!selects_card_reject(false, DepthMode::Reuse));
    }

    #[test]
    fn refusing_the_card_reject_leaves_receiver_admission_unchanged() {
        let limits = card_limits(15);
        assert!(!card_reject(&limits, wgpu::Backend::Vulkan, 2));
        assert!(validate(&limits, ReceiverLayout::Opposing, 2, None).is_ok());
    }
}
