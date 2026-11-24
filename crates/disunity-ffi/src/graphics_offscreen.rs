//! The offscreen work a frame records: camera renders, blits and texture copies into render
//! textures. The world validates and orders the records; this file converts them.
use crate::{
    DisunityResult, callbacks::mutate, errors::AbiError, graphics_properties::texture_ref,
    physics::component, store::Session,
};
use worldrunner_world_simulation::graphics::{
    MaterialId, OffscreenWork, RenderTextureId, TargetRef,
};

const WORK_CAMERA_RENDER: u32 = 0;
const WORK_BLIT: u32 = 1;
const WORK_COPY_TEXTURE: u32 = 2;

const TARGET_RENDER: u32 = 0;
const TARGET_TEMPORARY: u32 = 1;

/// One piece of offscreen work a script records with [`disunity_offscreen_record`]. `kind` is 0
/// for a camera render (`camera` is the camera component, `target` the render texture), 1 for a
/// blit and 2 for a texture copy. A blit and a copy read `source_kind` and `source` as a texture
/// property does (0 none, 1 asset, 2 render texture, 4 temporary render texture), and write the
/// target that `target_kind` names: 0 for the render texture `target`, 1 for the temporary render
/// texture whose property name id is the low 32 bits of `target`. A camera render ignores
/// `target_kind`. A blit draws with `material` (0 for none) and `pass` (-1 for every pass); a copy
/// addresses `source_element`, `source_mip`, `target_element` and `target_mip`. A dispatch and the
/// commands of `graphics_commands` are recorded by their own calls. Fields a kind does not use are
/// ignored. A host reads the frame's work, every kind of it, in the graphics snapshot.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisunityOffscreenWork {
    pub kind: u32,
    pub camera: u64,
    pub target: u64,
    pub target_kind: u32,
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
                target: self.work_target()?,
                material: (self.material != 0).then(|| MaterialId::from_bits(self.material)),
                pass: self.pass,
            }),
            WORK_COPY_TEXTURE => Ok(OffscreenWork::CopyTexture {
                source: texture_ref(self.source_kind, self.source)?,
                source_element: self.source_element,
                source_mip: self.source_mip,
                target: self.work_target()?,
                target_element: self.target_element,
                target_mip: self.target_mip,
            }),
            _ => Err("offscreen work is a camera render (0), a blit (1) or a copy (2); a dispatch and the commands have their own calls".into()),
        }
    }

    /// The target of a blit or a copy.
    fn work_target(self) -> Result<TargetRef, AbiError> {
        match self.target_kind {
            TARGET_RENDER => Ok(TargetRef::Render(RenderTextureId::from_bits(self.target))),
            TARGET_TEMPORARY => u32::try_from(self.target)
                .map(|bits| TargetRef::Temporary(bits.cast_signed()))
                .map_err(|_| "a temporary render texture's name id is a 32-bit int".into()),
            _ => Err(
                "a blit or copy target is a render texture (0) or a temporary render texture (1)"
                    .into(),
            ),
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
