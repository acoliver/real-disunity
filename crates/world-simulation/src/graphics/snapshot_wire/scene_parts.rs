//! The encodings of the cameras, renderers, offscreen work and the snapshot itself.
use super::WireError;
use super::codec::{Reader, Wire, Writer, wire_struct};
use super::command_parts::{read_command, write_command};
use crate::ComponentHandle;
use crate::graphics::{CameraView, GraphicsSnapshot, OffscreenWork, RenderTextureId, RendererView};
use crate::handles::Identity;
use crate::presentation::{Camera, CameraClear, CameraFlags, Projection};

impl Wire for ComponentHandle {
    fn write(&self, out: &mut Writer) -> Result<(), WireError> {
        self.0.world.write(out)?;
        self.0.slot.write(out)?;
        self.0.generation.write(out)
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, WireError> {
        Ok(Self(Identity {
            world: u64::read(input)?,
            slot: u32::read(input)?,
            generation: u32::read(input)?,
        }))
    }
}

impl Wire for Camera {
    fn write(&self, out: &mut Writer) -> Result<(), WireError> {
        self.enabled.write(out)?;
        (self.projection as u32).write(out)?;
        self.half_height.write(out)?;
        self.field_of_view.write(out)?;
        self.near.write(out)?;
        self.far.write(out)?;
        (self.clear as u32).write(out)?;
        self.background.write(out)?;
        self.depth.write(out)?;
        self.culling_mask.write(out)?;
        self.viewport.write(out)?;
        self.depth_texture_mode.write(out)?;
        self.flags.bits().write(out)?;
        self.target_display.write(out)?;
        self.target_texture.write(out)?;
        self.projection_matrix.write(out)?;
        self.world_to_camera.write(out)?;
        self.camera_type.write(out)
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, WireError> {
        let enabled = bool::read(input)?;
        let projection = u32::read(input)?;
        let projection =
            Projection::from_value(projection).ok_or(WireError::UnknownTag(projection))?;
        let half_height = f32::read(input)?;
        let field_of_view = f32::read(input)?;
        let near = f32::read(input)?;
        let far = f32::read(input)?;
        let clear = u32::read(input)?;
        let clear = CameraClear::from_flags(clear).ok_or(WireError::UnknownTag(clear))?;
        let background = <[f64; 4]>::read(input)?;
        let depth = i32::read(input)?;
        let culling_mask = u32::read(input)?;
        let viewport = <[f32; 4]>::read(input)?;
        let depth_texture_mode = u32::read(input)?;
        let flags = u32::read(input)?;
        let flags = CameraFlags::from_bits(flags).ok_or(WireError::UnknownTag(flags))?;
        Ok(Self {
            enabled,
            projection,
            half_height,
            field_of_view,
            near,
            far,
            clear,
            background,
            depth,
            culling_mask,
            viewport,
            depth_texture_mode,
            flags,
            target_display: u32::read(input)?,
            target_texture: Option::<RenderTextureId>::read(input)?,
            projection_matrix: Wire::read(input)?,
            world_to_camera: Wire::read(input)?,
            camera_type: u32::read(input)?,
        })
    }
}

wire_struct!(CameraView {
    settings,
    world_matrix
});
wire_struct!(RendererView {
    local_to_world,
    mesh,
    layer,
    materials,
    render_queue,
    property_block
});

const WORK_CAMERA_RENDER: u8 = 0;
const WORK_BLIT: u8 = 1;
const WORK_COPY_TEXTURE: u8 = 2;
const WORK_DISPATCH: u8 = 3;
pub(super) const WORK_SET_RENDER_TARGET: u8 = 4;
pub(super) const WORK_CLEAR_RENDER_TARGET: u8 = 5;
pub(super) const WORK_SET_GLOBAL: u8 = 6;
pub(super) const WORK_SET_GLOBAL_KEYWORD: u8 = 7;
pub(super) const WORK_ALLOCATE_TEMPORARY: u8 = 8;
pub(super) const WORK_RELEASE_TEMPORARY: u8 = 9;

impl Wire for OffscreenWork {
    fn write(&self, out: &mut Writer) -> Result<(), WireError> {
        match self {
            Self::CameraRender { camera, target } => {
                WORK_CAMERA_RENDER.write(out)?;
                camera.write(out)?;
                target.write(out)
            }
            Self::Blit {
                source,
                target,
                material,
                pass,
            } => {
                WORK_BLIT.write(out)?;
                source.write(out)?;
                target.write(out)?;
                material.write(out)?;
                pass.write(out)
            }
            Self::CopyTexture {
                source,
                source_element,
                source_mip,
                target,
                target_element,
                target_mip,
            } => {
                WORK_COPY_TEXTURE.write(out)?;
                source.write(out)?;
                [*source_element, *source_mip].write(out)?;
                target.write(out)?;
                [*target_element, *target_mip].write(out)
            }
            Self::Dispatch {
                shader,
                kernel,
                groups,
                bindings,
            } => {
                WORK_DISPATCH.write(out)?;
                shader.write(out)?;
                kernel.write(out)?;
                groups.write(out)?;
                bindings.write(out)
            }
            command => write_command(command, out),
        }
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, WireError> {
        match u8::read(input)? {
            WORK_CAMERA_RENDER => Ok(Self::CameraRender {
                camera: Wire::read(input)?,
                target: Wire::read(input)?,
            }),
            WORK_BLIT => Ok(Self::Blit {
                source: Wire::read(input)?,
                target: Wire::read(input)?,
                material: Wire::read(input)?,
                pass: Wire::read(input)?,
            }),
            WORK_COPY_TEXTURE => read_copy(input),
            WORK_DISPATCH => Ok(Self::Dispatch {
                shader: Wire::read(input)?,
                kernel: Wire::read(input)?,
                groups: Wire::read(input)?,
                bindings: Wire::read(input)?,
            }),
            other => read_command(other, input),
        }
    }
}

fn read_copy(input: &mut Reader<'_>) -> Result<OffscreenWork, WireError> {
    let source = Wire::read(input)?;
    let [source_element, source_mip] = <[u32; 2]>::read(input)?;
    let target = Wire::read(input)?;
    let [target_element, target_mip] = <[u32; 2]>::read(input)?;
    Ok(OffscreenWork::CopyTexture {
        source,
        source_element,
        source_mip,
        target,
        target_element,
        target_mip,
    })
}

wire_struct!(GraphicsSnapshot {
    shaders,
    materials,
    render_textures,
    global,
    work,
    cameras,
    lights,
    renderers,
    compute_shaders,
    compute_buffers
});
