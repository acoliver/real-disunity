//! The `Canvas`, `CanvasRenderer` and `CanvasGroup` layouts, which place and draw UI.

use crate::{
    ClassId, SerializedError,
    object::{Layout, ObjectReader, PPtr},
};

/// Draws one UI element's mesh for its canvas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CanvasRenderer {
    pub game_object: PPtr,
    /// Whether a fully transparent element is skipped.
    pub cull_transparent_mesh: bool,
}

impl Layout for CanvasRenderer {
    const CLASS: ClassId = ClassId::CANVAS_RENDERER;

    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        Ok(Self {
            game_object: r.pptr()?,
            cull_transparent_mesh: r.bool()?,
        })
    }
}

/// Whether a canvas snaps its elements to pixels, and whether it overrides its parent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PixelPerfect {
    pub enabled: bool,
    pub override_parent: bool,
}

/// Where a canvas sorts among other canvases and renderers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CanvasSorting {
    /// Whether a nested canvas sorts on its own instead of with its parent.
    pub override_parent: bool,
    pub bucket_normalized_size: f32,
    pub layer_id: i32,
    pub order: i16,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Canvas {
    pub game_object: PPtr,
    pub enabled: bool,
    /// Unity's `RenderMode`: 0 screen space overlay, 1 screen space camera, 2 world space.
    pub render_mode: i32,
    pub camera: PPtr,
    /// Distance from the camera in screen space camera mode.
    pub plane_distance: f32,
    pub pixel_perfect: PixelPerfect,
    pub receives_events: bool,
    pub sorting: CanvasSorting,
    pub vertex_color_always_gamma_space: bool,
    /// Unity's `AdditionalCanvasShaderChannels` flags.
    pub additional_shader_channels: i32,
    pub update_rect_transform_for_standalone: i32,
    pub target_display: i8,
}

impl Layout for Canvas {
    const CLASS: ClassId = ClassId::CANVAS;

    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        let game_object = r.pptr()?;
        let enabled = r.bool()?;
        r.align()?;
        let render_mode = r.i32()?;
        let camera = r.pptr()?;
        let plane_distance = r.f32()?;
        let [pixel, receives_events, override_sorting, override_pixel] =
            [r.bool()?, r.bool()?, r.bool()?, r.bool()?];
        let bucket_normalized_size = r.f32()?;
        let vertex_color_always_gamma_space = r.bool()?;
        r.align()?;
        let additional_shader_channels = r.i32()?;
        let update_rect_transform_for_standalone = r.i32()?;
        let layer_id = r.i32()?;
        Ok(Self {
            game_object,
            enabled,
            render_mode,
            camera,
            plane_distance,
            pixel_perfect: PixelPerfect {
                enabled: pixel,
                override_parent: override_pixel,
            },
            receives_events,
            sorting: CanvasSorting {
                override_parent: override_sorting,
                bucket_normalized_size,
                layer_id,
                order: r.i16()?,
            },
            vertex_color_always_gamma_space,
            additional_shader_channels,
            update_rect_transform_for_standalone,
            target_display: r.i8()?,
        })
    }
}

/// How a canvas group affects input to the elements under it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GroupInput {
    pub interactable: bool,
    pub blocks_raycasts: bool,
    /// Whether the group ignores the settings of groups above it.
    pub ignore_parent_groups: bool,
}

/// Fades and gates input for the UI elements under one object.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CanvasGroup {
    pub game_object: PPtr,
    pub enabled: bool,
    pub alpha: f32,
    pub input: GroupInput,
}

impl Layout for CanvasGroup {
    const CLASS: ClassId = ClassId::CANVAS_GROUP;

    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        let game_object = r.pptr()?;
        let enabled = r.bool()?;
        r.align()?;
        let alpha = r.f32()?;
        let input = GroupInput {
            interactable: r.bool()?,
            blocks_raycasts: r.bool()?,
            ignore_parent_groups: r.bool()?,
        };
        r.align()?;
        Ok(Self {
            game_object,
            enabled,
            alpha,
            input,
        })
    }
}
