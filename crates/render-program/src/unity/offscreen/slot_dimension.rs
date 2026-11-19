//! Whether the texture a draw binds to a slot has the view the slot's shader declares, and the
//! refusal that names both when it has not.

use wgpu::TextureViewDimension as View;

use crate::ids::TextureId;

use super::error::OffscreenError;
use super::frame::RenderDimension;

/// Whether a texture stored with `stored` has a view of the `declared` dimension. The rule is the
/// one the resource registry applies to a view: a volume has only a 3D view, which is whole
/// whatever its depth, and a plain, cube or array texture has a view by its layers (one for 2D,
/// six for a cube, a multiple of six for a cube array, any for a 2D array).
fn gives_view(stored: RenderDimension, declared: View) -> bool {
    if let RenderDimension::Unknown(_) = stored {
        // The state refuses a dimension it could not map before a draw reads it.
        return true;
    }
    let volume = stored.is_volume();
    if volume != (declared == View::D3) {
        return false;
    }
    let layers = if volume { 1 } else { stored.layers() };
    match declared {
        View::D1 => false,
        View::D2 => layers == 1,
        View::D2Array | View::D3 => true,
        View::Cube => layers == 6,
        View::CubeArray => layers.is_multiple_of(6),
    }
}

/// Refuses a slot whose declared view dimension `texture`, stored as `stored`, does not have. A
/// slot the shader declares as a plain 2D texture over an array of several layers is
/// `FlatSlotOverArray`, since no 2D view can show them (a one-layer array has a 2D view). Any
/// other pair the texture has no view for, such as a cube slot over a 2D texture or a 2D slot over
/// a volume, is `SlotDimensionMismatch`.
///
/// # Errors
/// Fails with [`OffscreenError::FlatSlotOverArray`] or [`OffscreenError::SlotDimensionMismatch`],
/// which name the property `name`, and the layers or both dimensions.
pub(super) fn refuse(
    name: &str,
    texture: TextureId,
    stored: RenderDimension,
    declared: View,
) -> Result<(), OffscreenError> {
    if declared == View::D2
        && let RenderDimension::D2Array(layers @ 2..) = stored
    {
        return Err(OffscreenError::FlatSlotOverArray {
            name: name.to_owned(),
            texture,
            layers,
        });
    }
    if gives_view(stored, declared) {
        return Ok(());
    }
    Err(OffscreenError::SlotDimensionMismatch {
        name: name.to_owned(),
        texture,
        declared,
        stored,
    })
}

/// The declared view dimension as a shader author names it.
pub(super) const fn view_name(view: View) -> &'static str {
    match view {
        View::D1 => "a 1D texture",
        View::D2 => "a 2D texture",
        View::D2Array => "a 2D array",
        View::Cube => "a cube",
        View::CubeArray => "a cube array",
        View::D3 => "a 3D texture",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_VIEWS: [View; 6] = [
        View::D1,
        View::D2,
        View::D2Array,
        View::Cube,
        View::CubeArray,
        View::D3,
    ];

    /// The views a texture stored as `stored` gives.
    fn views_of(stored: RenderDimension) -> Vec<View> {
        ALL_VIEWS
            .into_iter()
            .filter(|&view| refuse("_Tex", TextureId(1), stored, view).is_ok())
            .collect()
    }

    #[test]
    fn a_plain_texture_gives_a_2d_view_and_an_array_view_of_one_layer() {
        assert_eq!(views_of(RenderDimension::D2), [View::D2, View::D2Array]);
    }

    #[test]
    fn a_cube_gives_a_cube_view_and_the_array_views_of_its_faces() {
        assert_eq!(
            views_of(RenderDimension::Cube),
            [View::D2Array, View::Cube, View::CubeArray]
        );
    }

    #[test]
    fn a_volume_of_any_depth_gives_only_a_3d_view() {
        assert_eq!(views_of(RenderDimension::D3(1)), [View::D3]);
        assert_eq!(views_of(RenderDimension::D3(64)), [View::D3]);
    }

    #[test]
    fn an_array_gives_a_cube_view_only_with_six_layers() {
        assert!(views_of(RenderDimension::D2Array(6)).contains(&View::Cube));
        assert!(!views_of(RenderDimension::D2Array(3)).contains(&View::Cube));
        assert!(views_of(RenderDimension::D2Array(12)).contains(&View::CubeArray));
    }

    #[test]
    fn a_2d_slot_over_an_array_of_several_layers_keeps_its_own_refusal() {
        let error =
            refuse("_Tex", TextureId(3), RenderDimension::D2Array(2), View::D2).unwrap_err();
        assert!(
            matches!(error, OffscreenError::FlatSlotOverArray { layers: 2, .. }),
            "{error}"
        );
        assert!(refuse("_Tex", TextureId(3), RenderDimension::D2Array(1), View::D2).is_ok());
    }

    #[test]
    fn the_refusal_names_the_property_and_both_dimensions() {
        let error = refuse("_Sky", TextureId(7), RenderDimension::D2, View::Cube).unwrap_err();
        assert!(matches!(
            error,
            OffscreenError::SlotDimensionMismatch {
                declared: View::Cube,
                stored: RenderDimension::D2,
                ..
            }
        ));
        assert_eq!(
            error.to_string(),
            "the shader reads _Sky as a cube and texture TextureId(7) is a 2D texture"
        );
        let volume = refuse("_Noise", TextureId(2), RenderDimension::Cube, View::D3).unwrap_err();
        assert!(
            volume
                .to_string()
                .starts_with("the shader reads _Noise as a 3D texture")
                && volume.to_string().ends_with("is a cube"),
            "{volume}"
        );
    }
}
