//! The offscreen work a frame records: camera renders, blits and texture copies into render
//! textures. The world validates and orders the records; this file converts them.
use crate::{
    DisunityResult,
    callbacks::mutate,
    errors::AbiError,
    graphics_properties::{texture_parts, texture_ref},
    physics::{component, identity},
    store::Session,
};
use worldrunner_world_simulation::graphics::{MaterialId, OffscreenWork, RenderTextureId};

const WORK_CAMERA_RENDER: u32 = 0;
const WORK_BLIT: u32 = 1;
const WORK_COPY_TEXTURE: u32 = 2;

/// One piece of offscreen work. `kind` is 0 for a camera render (`camera` is the camera
/// component, `target` the render texture), 1 for a blit and 2 for a texture copy. A blit and a
/// copy read `source_kind` and `source` as a texture property does (0 none, 1 asset, 2 render
/// texture); a blit draws with `material` (0 for none) and `pass` (-1 for every pass); a copy
/// addresses `source_element`, `source_mip`, `target_element` and `target_mip`. Fields a kind
/// does not use are ignored.
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
            _ => Err("offscreen work is a camera render (0), a blit (1) or a copy (2)".into()),
        }
    }

    fn from_native(session: &Session, work: &OffscreenWork) -> Self {
        match *work {
            OffscreenWork::CameraRender { camera, target } => Self {
                kind: WORK_CAMERA_RENDER,
                camera: identity(session, camera).unwrap_or(0),
                target: target.to_bits(),
                ..Self::default()
            },
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
                    pass,
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
                    source_element,
                    source_mip,
                    target_element,
                    target_mip,
                    ..Self::default()
                }
            }
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
        for (index, work) in s.native.take_offscreen().iter().enumerate() {
            unsafe {
                records
                    .add(index)
                    .write(DisunityOffscreenWork::from_native(s, work));
            }
        }
        Ok(())
    })
}
