//! The offscreen work a flush executes: camera renders, blits, texture copies and compute
//! dispatches, in the order scripts recorded them.
use crate::{GpuFlush, HostError, native};

/// A texture a piece of work or a binding names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GpuTexture {
    None,
    /// A player texture by the identity the host knows it by (see [`crate::PlayerTexture`]).
    Asset(u64),
    /// A render texture of the world, by id.
    Render(u64),
    /// The texture the global shader state holds under this property name when the dispatch
    /// runs (see [`GpuFlush::property`] with [`crate::GpuScope::Global`]).
    Global(String),
}

/// One value a dispatch ran with.
#[derive(Clone, Debug, PartialEq)]
pub enum GpuValue {
    Float(f32),
    Int(i32),
    Vector([f32; 4]),
    /// Sixteen floats in column-major order.
    Matrix([f32; 16]),
    Texture {
        texture: GpuTexture,
        scale: [f32; 2],
        offset: [f32; 2],
    },
    /// A compute buffer, by id.
    Buffer(u64),
}

/// A named value of a dispatch.
#[derive(Clone, Debug, PartialEq)]
pub struct GpuBinding {
    pub name: String,
    pub value: GpuValue,
}

/// A dispatch of one kernel of a compute shader. `bindings` is the snapshot of the values the
/// kernel had when the script called `Dispatch`: floats, integers, vectors, matrices, textures
/// and buffers, each list in name order.
#[derive(Clone, Debug, PartialEq)]
pub struct GpuDispatch {
    /// The compute shader's id (see [`GpuFlush::compute_shader`]).
    pub shader: u64,
    pub kernel: u32,
    pub groups: [u32; 3],
    pub bindings: Vec<GpuBinding>,
}

/// One piece of offscreen work. Ids are the ABI ids of the world; a resource destroyed after the
/// work was recorded no longer resolves and the executor skips the work.
#[derive(Clone, Debug, PartialEq)]
pub enum GpuWork {
    /// A camera component (by managed handle) draws into a render texture.
    CameraRender {
        camera: u64,
        target: u64,
    },
    /// A full-surface draw of `source` into `target` with a material pass (0 for no material,
    /// which copies; a pass of -1 draws every pass).
    Blit {
        source: GpuTexture,
        target: u64,
        material: u64,
        pass: i32,
    },
    /// A copy of one element and mip level of a texture into one of a render texture.
    CopyTexture {
        source: GpuTexture,
        source_element: u32,
        source_mip: u32,
        target: u64,
        target_element: u32,
        target_mip: u32,
    },
    Dispatch(GpuDispatch),
}

fn texture(kind: u32, resource: u64, global: String) -> Result<GpuTexture, HostError> {
    match kind {
        0 => Ok(GpuTexture::None),
        1 => Ok(GpuTexture::Asset(resource)),
        2 => Ok(GpuTexture::Render(resource)),
        3 => Ok(GpuTexture::Global(global)),
        other => Err(HostError::Managed(format!("unknown texture kind {other}"))),
    }
}

impl GpuFlush<'_> {
    /// Takes all the offscreen work recorded since the last take, in call order, dispatches
    /// included. Work nobody takes is dropped when the next frame starts.
    pub fn take_offscreen(&mut self) -> Result<Vec<GpuWork>, HostError> {
        let mut count = 0_u32;
        let probe = unsafe { (self.calls.take)(self.world, std::ptr::null_mut(), 0, &mut count) };
        self.check(probe)?;
        if count == 0 {
            return Ok(Vec::new());
        }
        let mut records = vec![native::DisunityOffscreenWork::default(); count as usize];
        let taken =
            unsafe { (self.calls.take)(self.world, records.as_mut_ptr(), count, &mut count) };
        self.check(taken)?;
        records
            .iter()
            .map(|record| self.work(record))
            .collect::<Result<_, _>>()
    }

    fn work(&self, record: &native::DisunityOffscreenWork) -> Result<GpuWork, HostError> {
        let source = || texture(record.source_kind, record.source, String::new());
        Ok(match record.kind {
            0 => GpuWork::CameraRender {
                camera: record.camera,
                target: record.target,
            },
            1 => GpuWork::Blit {
                source: source()?,
                target: record.target,
                material: record.material,
                pass: record.pass,
            },
            2 => GpuWork::CopyTexture {
                source: source()?,
                source_element: record.source_element,
                source_mip: record.source_mip,
                target: record.target,
                target_element: record.target_element,
                target_mip: record.target_mip,
            },
            3 => GpuWork::Dispatch(GpuDispatch {
                shader: record.shader,
                kernel: record.kernel,
                groups: [record.groups_x, record.groups_y, record.groups_z],
                bindings: self.bindings(record.bindings)?,
            }),
            other => return Err(HostError::Managed(format!("unknown work kind {other}"))),
        })
    }

    /// The values a dispatch record ran with, by its token.
    fn bindings(&self, token: u32) -> Result<Vec<GpuBinding>, HostError> {
        let mut info = native::DisunityComputeBindingsInfo::default();
        let none = (
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );
        let probe =
            unsafe { (self.calls.bindings)(self.world, token, &mut info, none.0, none.1, none.2) };
        self.check(probe)?;
        let mut entries =
            vec![native::DisunityComputeBinding::default(); info.entry_count as usize];
        let mut floats = vec![0.0_f32; info.float_count as usize];
        let mut text = vec![0_u8; info.text_length as usize];
        let read = unsafe {
            (self.calls.bindings)(
                self.world,
                token,
                &mut info,
                entries.as_mut_ptr(),
                floats.as_mut_ptr(),
                text.as_mut_ptr(),
            )
        };
        self.check(read)?;
        entries
            .iter()
            .map(|entry| binding(entry, &floats, &text))
            .collect()
    }
}

fn text_at(text: &[u8], offset: u32, length: u32) -> Result<String, HostError> {
    let range = offset as usize..(offset as usize).saturating_add(length as usize);
    text.get(range)
        .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
        .ok_or_else(|| HostError::Managed("a binding names text outside the engine's text".into()))
}

fn floats_at<const N: usize>(floats: &[f32], offset: u32) -> Result<[f32; N], HostError> {
    let start = offset as usize;
    floats
        .get(start..start + N)
        .and_then(|slice| <[f32; N]>::try_from(slice).ok())
        .ok_or_else(|| {
            HostError::Managed("a binding names floats outside the engine's floats".into())
        })
}

fn binding(
    entry: &native::DisunityComputeBinding,
    floats: &[f32],
    text: &[u8],
) -> Result<GpuBinding, HostError> {
    let at = entry.float_offset;
    let value = match entry.kind {
        0 => GpuValue::Float(floats_at::<1>(floats, at)?[0]),
        1 => GpuValue::Int(entry.int_value),
        2 => GpuValue::Vector(floats_at(floats, at)?),
        3 => GpuValue::Matrix(floats_at(floats, at)?),
        4 => {
            let [scale_x, scale_y, offset_x, offset_y] = floats_at::<4>(floats, at)?;
            GpuValue::Texture {
                texture: texture(
                    entry.texture_kind,
                    entry.resource,
                    text_at(text, entry.global_offset, entry.global_length)?,
                )?,
                scale: [scale_x, scale_y],
                offset: [offset_x, offset_y],
            }
        }
        5 => GpuValue::Buffer(entry.resource),
        other => return Err(HostError::Managed(format!("unknown binding kind {other}"))),
    };
    Ok(GpuBinding {
        name: text_at(text, entry.name_offset, entry.name_length)?,
        value,
    })
}
