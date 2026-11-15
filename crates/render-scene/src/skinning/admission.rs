//! Whether a device can hold the scene palette and read it from the vertex stage.
//!
//! Static scenes have no palette and are never refused here. A skinned scene is refused at
//! upload with the limit it needs, rather than at the first draw or by silently skinning on
//! the CPU.

use wgpu::Limits;

use super::{PALETTE_ENTRY_BYTES, ScenePalette};

/// Vertex buffers a skinned object reads: geometry, instances and the skin stream.
const SKINNED_VERTEX_BUFFERS: u32 = 3;

/// Storage buffers the vertex stage reads in a skinned program: the contact heights and the
/// palette.
const SKINNED_VERTEX_STORAGE: u32 = 2;

/// Vertex attributes of the widest skinned layout: seven geometry, five instance, one skin
/// descriptor.
const SKINNED_VERTEX_ATTRIBUTES: u32 = 13;

/// The device cannot draw the scene's skinned objects.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SkinAdmissionError {
    /// The palette is larger than one storage binding or one buffer.
    #[error("the skin palette needs {required} bytes, device limit is {limit}")]
    PaletteSize { required: u64, limit: u64 },
    /// The device offers fewer of a resource than skinned objects read.
    #[error("skinned objects need {required} {what}, the device offers {limit}")]
    Limit {
        what: &'static str,
        required: u32,
        limit: u32,
    },
}

/// Checks `palette` against `limits`. An empty palette needs nothing.
///
/// # Errors
///
/// Returns the first limit the skinned programs would exceed.
pub(crate) fn validate(palette: &ScenePalette, limits: &Limits) -> Result<(), SkinAdmissionError> {
    if palette.is_empty() {
        return Ok(());
    }
    let size = palette.bytes().len() as u64;
    debug_assert_eq!(size % PALETTE_ENTRY_BYTES, 0);
    let allowed = limits
        .max_storage_buffer_binding_size
        .min(limits.max_buffer_size);
    if size > allowed {
        return Err(SkinAdmissionError::PaletteSize {
            required: size,
            limit: allowed,
        });
    }
    for (what, required, limit) in [
        (
            "vertex buffers",
            SKINNED_VERTEX_BUFFERS,
            limits.max_vertex_buffers,
        ),
        (
            "vertex attributes",
            SKINNED_VERTEX_ATTRIBUTES,
            limits.max_vertex_attributes,
        ),
        (
            "storage buffers in the vertex stage",
            SKINNED_VERTEX_STORAGE,
            limits.max_storage_buffers_per_shader_stage,
        ),
    ] {
        if limit < required {
            return Err(SkinAdmissionError::Limit {
                what,
                required,
                limit,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use worldrunner_render_plan::SceneSkinPose;

    use super::*;

    fn palette(joints: usize) -> ScenePalette {
        let pose = SceneSkinPose::rest_pose(joints).unwrap();
        ScenePalette::new(&[pose]).unwrap()
    }

    #[test]
    fn a_scene_without_poses_needs_nothing() {
        let none = Limits {
            max_storage_buffer_binding_size: 0,
            max_storage_buffers_per_shader_stage: 0,
            ..Limits::default()
        };
        assert_eq!(validate(&ScenePalette::default(), &none), Ok(()));
    }

    #[test]
    fn a_palette_beyond_the_binding_size_is_refused_with_its_size() {
        let limits = Limits {
            max_storage_buffer_binding_size: 95,
            ..Limits::default()
        };
        assert_eq!(
            validate(&palette(1), &limits),
            Err(SkinAdmissionError::PaletteSize {
                required: 2 * PALETTE_ENTRY_BYTES,
                limit: 95
            })
        );
    }

    #[test]
    fn a_device_without_vertex_storage_refuses_the_palette() {
        let limits = Limits {
            max_storage_buffers_per_shader_stage: 1,
            ..Limits::default()
        };
        assert!(matches!(
            validate(&palette(2), &limits),
            Err(SkinAdmissionError::Limit {
                what: "storage buffers in the vertex stage",
                ..
            })
        ));
        assert_eq!(validate(&palette(2), &Limits::default()), Ok(()));
    }
}
