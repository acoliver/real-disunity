//! The layout of `ComputeShader` objects: the kernels a compute shader declares, in the order a
//! script addresses them by index, and the thread group size each one states.
//!
//! The object holds one list of kernels per target platform. The first list is read here; every
//! kernel's compiled variants are walked to find the next kernel, and the thread group size of the
//! first variant is the kernel's. The compiled code is not decoded. Everything after the first
//! list (the constant buffers and the other platforms) is left unread.
//!
//! Only the layouts of the player checked (Unity 6000.3.15f1, 15 of its 16 compute shaders) are
//! understood. A kernel that does not match is an error, not a guess, so a kernel index is never
//! wrong.

use crate::{
    ClassId, SerializedError,
    object::{Layout, ObjectReader},
};

/// Fewest bytes of a string.
const STRING_LEN: usize = 4;
/// Fewest bytes of a resource binding: its name and five more 32-bit fields.
const RESOURCE_LEN: usize = STRING_LEN + 5 * 4;
/// Bytes of a built-in sampler binding: two 32-bit fields.
const SAMPLER_LEN: usize = 8;
/// Fewest bytes of a kernel: its name, the variant count, the keyword list and four trailing
/// fields.
const KERNEL_LEN: usize = STRING_LEN + 4 + 4 + 16;
/// Fewest bytes of a compiled variant: five list counts, the code length, the thread group size
/// list, the marker and the two lists after it.
const VARIANT_LEN: usize = 5 * 4 + 4 + 4 + 3 * 4 + 4 + 2 * 4;
/// Thread group sizes have three dimensions.
const GROUP_DIMENSIONS: usize = 3;

/// One kernel: its name and the thread group size its compiled code states, or `None` when the
/// object states none (all three dimensions zero).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComputeKernel {
    pub name: String,
    pub thread_group_size: Option<[u32; 3]>,
}

/// A compute shader's name and kernels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComputeShader {
    pub name: String,
    pub kernels: Vec<ComputeKernel>,
}

fn invalid(r: &ObjectReader<'_>, reason: &'static str) -> SerializedError {
    SerializedError::InvalidComputeShader {
        offset: r.position(),
        reason,
    }
}

/// A list of 32-bit values, none of which matters here.
fn skip_words(r: &mut ObjectReader<'_>, what: &'static str) -> Result<(), SerializedError> {
    r.array(what, 4, ObjectReader::u32).map(drop)
}

/// A list of names, none of which matters here.
fn skip_names(r: &mut ObjectReader<'_>, what: &'static str) -> Result<(), SerializedError> {
    r.array(what, STRING_LEN, ObjectReader::string).map(drop)
}

/// A list of resource bindings; their names are not used.
fn skip_resources(r: &mut ObjectReader<'_>, what: &'static str) -> Result<(), SerializedError> {
    r.array(what, RESOURCE_LEN, |r| {
        r.string()?;
        r.bytes::<20>().map(drop)
    })
    .map(drop)
}

/// The five binding lists, the code and the thread group size of one compiled variant of a kernel,
/// and the bookkeeping after it. Returns the size the variant states.
fn skip_variant(r: &mut ObjectReader<'_>) -> Result<[u32; 3], SerializedError> {
    skip_resources(r, "constant buffer")?;
    skip_resources(r, "texture binding")?;
    r.array("built-in sampler", SAMPLER_LEN, |r| {
        r.bytes::<8>().map(drop)
    })?;
    skip_resources(r, "buffer binding")?;
    skip_resources(r, "writable buffer binding")?;
    r.byte_range("compiled code")?;
    let size = r.array("thread group size", 4, ObjectReader::u32)?;
    let size = <[u32; GROUP_DIMENSIONS]>::try_from(size)
        .map_err(|_| invalid(r, "a thread group size states other than three dimensions"))?;
    r.u32()?;
    skip_names(r, "variant keyword")?;
    skip_names(r, "variant keyword")?;
    Ok(size)
}

fn read_kernel(r: &mut ObjectReader<'_>) -> Result<ComputeKernel, SerializedError> {
    let name = r.string()?;
    let variants = r.count("kernel variant", VARIANT_LEN)?;
    skip_words(r, "kernel keyword")?;
    let mut size = None;
    for _ in 0..variants {
        let stated = skip_variant(r)?;
        size.get_or_insert(stated);
    }
    r.bytes::<16>()?;
    Ok(ComputeKernel {
        name,
        thread_group_size: size.filter(|size| size.iter().any(|&dimension| dimension != 0)),
    })
}

impl Layout for ComputeShader {
    const CLASS: ClassId = ClassId::COMPUTE_SHADER;

    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        let name = r.string()?;
        let platforms = r.u32()?;
        if platforms == 0 {
            r.rest()?;
            return Ok(Self {
                name,
                kernels: Vec::new(),
            });
        }
        // The first platform: its target renderer and level, then its kernels.
        r.bytes::<8>()?;
        let kernel_count = r.count("kernel", KERNEL_LEN)?;
        let kernels = (0..kernel_count)
            .map(|_| read_kernel(r))
            .collect::<Result<_, _>>()?;
        r.rest()?;
        Ok(Self { name, kernels })
    }
}
