//! The offscreen work a frame records: camera renders, blits and texture copies into render
//! textures. The world validates and orders the records; this file converts them.
use crate::{
    DisunityResult,
    callbacks::mutate,
    errors::AbiError,
    graphics_commands::{command_kind, detail_of},
    graphics_properties::{texture_parts, texture_ref},
    physics::{component, identity},
    store::Session,
};
use worldrunner_world_simulation::graphics::{MaterialId, OffscreenWork, RenderTextureId};

const WORK_CAMERA_RENDER: u32 = 0;
const WORK_BLIT: u32 = 1;
const WORK_COPY_TEXTURE: u32 = 2;
const WORK_DISPATCH: u32 = 3;

/// One piece of offscreen work. `kind` is 0 for a camera render (`camera` is the camera
/// component, `target` the render texture), 1 for a blit, 2 for a texture copy and 3 for a
/// compute dispatch. A blit and a copy read `source_kind` and `source` as a texture property
/// does (0 none, 1 asset, 2 render texture); a blit draws with `material` (0 for none) and
/// `pass` (-1 for every pass); a copy addresses `source_element`, `source_mip`,
/// `target_element` and `target_mip`. A dispatch names its `shader` and `kernel` and the thread
/// groups it launches; `bindings` is the token that [`crate::compute_bindings`] reads the values
/// it ran with by, valid until the next take. A dispatch is recorded by
/// `disunity_compute_dispatch`, never by [`disunity_offscreen_record`], and only taking returns
/// one. The commands of `graphics_commands` take kinds 4 to 9 and a `bindings` token for
/// `disunity_offscreen_detail`, which reads the record's encoded bytes; they are recorded by
/// their own calls. Fields a kind does not use are ignored.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisunityOffscreenWork {
    pub kind: u32,
    pub camera: u64,
    pub target: u64,
    pub source_kind: u32,
    pub source: u64,
    pub material: u64,
    pub pass: i32,
    pub source_element: u32,
    pub source_mip: u32,
    pub target_element: u32,
    pub target_mip: u32,
    pub shader: u64,
    pub kernel: u32,
    pub groups_x: u32,
    pub groups_y: u32,
    pub groups_z: u32,
    pub bindings: u32,
}

impl DisunityOffscreenWork {
    fn to_native(self, session: &Session) -> Result<OffscreenWork, AbiError> {
        let target = RenderTextureId::from_bits(self.target);
        match self.kind {
            WORK_CAMERA_RENDER => Ok(OffscreenWork::CameraRender {
                camera: component(session, self.camera)?,
                target,
            }),
            WORK_BLIT => Ok(OffscreenWork::Blit {
                source: texture_ref(self.source_kind, self.source)?,
                target,
                material: (self.material != 0).then(|| MaterialId::from_bits(self.material)),
                pass: self.pass,
            }),
            WORK_COPY_TEXTURE => Ok(OffscreenWork::CopyTexture {
                source: texture_ref(self.source_kind, self.source)?,
                source_element: self.source_element,
                source_mip: self.source_mip,
                target,
                target_element: self.target_element,
                target_mip: self.target_mip,
            }),
            WORK_DISPATCH => {
                Err("a compute dispatch is recorded by disunity_compute_dispatch".into())
            }
            _ => Err("offscreen work is a camera render (0), a blit (1) or a copy (2); commands have their own calls".into()),
        }
    }

    fn from_native(session: &Session, work: &OffscreenWork, token: u32) -> Self {
        match work {
            OffscreenWork::CameraRender { camera, target } => Self {
                kind: WORK_CAMERA_RENDER,
                camera: identity(session, *camera).unwrap_or(0),
                target: target.to_bits(),
                ..Self::default()
            },
            OffscreenWork::Blit { .. } | OffscreenWork::CopyTexture { .. } => {
                Self::draw_record(work)
            }
            OffscreenWork::Dispatch {
                shader,
                kernel,
                groups,
                ..
            } => Self {
                kind: WORK_DISPATCH,
                shader: shader.to_bits(),
                kernel: *kernel,
                groups_x: groups[0],
                groups_y: groups[1],
                groups_z: groups[2],
                bindings: token,
                ..Self::default()
            },
            command => Self {
                kind: command_kind(command).unwrap_or_default(),
                bindings: token,
                ..Self::default()
            },
        }
    }

    /// The record of a blit or a texture copy.
    fn draw_record(work: &OffscreenWork) -> Self {
        match work {
            OffscreenWork::Blit {
                source,
                target,
                material,
                pass,
            } => {
                let (source_kind, source) = texture_parts(source);
                Self {
                    kind: WORK_BLIT,
                    target: target.to_bits(),
                    source_kind,
                    source,
                    material: material.map_or(0, MaterialId::to_bits),
                    pass: *pass,
                    ..Self::default()
                }
            }
            OffscreenWork::CopyTexture {
                source,
                source_element,
                source_mip,
                target,
                target_element,
                target_mip,
            } => {
                let (source_kind, source) = texture_parts(source);
                Self {
                    kind: WORK_COPY_TEXTURE,
                    target: target.to_bits(),
                    source_kind,
                    source,
                    source_element: *source_element,
                    source_mip: *source_mip,
                    target_element: *target_element,
                    target_mip: *target_mip,
                    ..Self::default()
                }
            }
            _ => Self::default(),
        }
    }
}

/// Appends a piece of offscreen work to the frame's list. Everything it names must be alive.
///
/// # Safety
/// `work` is readable for one record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_offscreen_record(
    world: u64,
    work: *const DisunityOffscreenWork,
) -> DisunityResult {
    if work.is_null() {
        return DisunityResult::invalid_argument();
    }
    let work = unsafe { *work };
    mutate(world, |s| {
        let native = work.to_native(s)?;
        s.native.record_offscreen(native)?;
        Ok(())
    })
}

/// Drains the frame's offscreen work into `records` (up to `capacity` of them, in call order)
/// and writes how many there were to `count`. A zero `capacity` drains nothing and only reports
/// the count; a capacity below the count is rejected and drains nothing.
///
/// # Safety
/// `records` is writable for `capacity` records, or null when `capacity` is zero; `count` is
/// writable for one value.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_offscreen_take(
    world: u64,
    records: *mut DisunityOffscreenWork,
    capacity: u32,
    count: *mut u32,
) -> DisunityResult {
    if count.is_null() || (capacity > 0 && records.is_null()) {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let pending =
            u32::try_from(s.native.offscreen_work().len()).map_err(|_| "work count overflow")?;
        unsafe { *count = pending };
        if capacity == 0 {
            return Ok(());
        }
        if capacity < pending {
            return Err("offscreen work capacity is too small".into());
        }
        let drained = s.native.take_offscreen();
        s.dispatch_bindings.clear();
        s.work_details.clear();
        for (index, work) in drained.iter().enumerate() {
            let token = match work {
                OffscreenWork::Dispatch { bindings, .. } => {
                    s.dispatch_bindings.push(bindings.clone());
                    u32::try_from(s.dispatch_bindings.len()).map_err(|_| "work count overflow")?
                }
                other if command_kind(other).is_some() => {
                    s.work_details.push(detail_of(other)?);
                    u32::try_from(s.work_details.len()).map_err(|_| "work count overflow")?
                }
                _ => 0,
            };
            unsafe {
                records
                    .add(index)
                    .write(DisunityOffscreenWork::from_native(s, work, token));
            }
        }
        Ok(())
    })
}
