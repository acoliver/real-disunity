use super::ReceiverLayout;
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
}
