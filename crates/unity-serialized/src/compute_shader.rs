//! The layout of `ComputeShader` objects: the kernels a compute shader declares, in the order a
//! script addresses them by index, and for each platform the compiled variants of every kernel
//! with the resources they bind.
//!
//! The layout was derived from the player checked (Unity 6000.3.15f1, its 16 compute shaders)
//! and every object of it is consumed to its last byte:
//!
//! - The object is its name and a list of platforms.
//! - A platform is a renderer id and a level, its kernels, the layouts of its constant buffers
//!   and a flag word. Fifteen shaders have two platforms: renderer 17, whose code is GLSL text,
//!   and renderer [`SPIRV_RENDERER`], whose code is Vulkan SPIR-V. One shader has only the SPIR-V
//!   platform. The kernels are the same in each, in the same order.
//! - A kernel is its name, its variant count, a list of header words, its variants, a table
//!   from the keywords of a variant to its index, the kernel's keyword names and two words.
//! - A variant is five binding lists (constant buffers, textures, built-in samplers, buffers and
//!   writable buffers or images), its code, its thread group size, a requirements word and a
//!   keyword list. A list of names follows every variant but a kernel's last.
//! - A binding is its name, an alias name, a slot word and two more words. Its slot word holds
//!   the binding in the low 16 bits, the descriptor set in the next seven and flags above them
//!   for a texture, a writable resource and a storage image. In all 76 SPIR-V variants of the
//!   player, the sets and bindings the slots give are exactly those the module decorates.
//!
//! A kernel that does not match is an error, not a guess, so a kernel index is never wrong.

use std::ops::Deref;

use crate::{
    ClassId, SerializedError,
    object::{Layout, ObjectReader},
    smolv,
};

/// The renderer id of the platform that holds Vulkan SPIR-V, as measured: every variant of every
/// platform with this id in the player checked is a SPIR-V module. It is not the same numbering as
/// the platform ids of graphics shaders.
pub const SPIRV_RENDERER: u32 = 21;

/// Fewest bytes of a string: its length word.
const STRING_LEN: usize = 4;
/// Fewest bytes of a binding: two empty names and four words.
const BINDING_LEN: usize = 2 * STRING_LEN + 16;
/// Bytes of a built-in sampler: two words.
const SAMPLER_LEN: usize = 8;
/// Fewest bytes of a variant: five empty lists, empty code, the group size's count and three
/// dimensions, the requirements word and an empty keyword list.
const VARIANT_LEN: usize = 5 * 4 + 4 + 4 + 3 * 4 + 4 + 4;
/// Fewest bytes of a kernel: its name, its variant count, an empty header list, and the table,
/// keyword names and two words that follow its variants.
const KERNEL_LEN: usize = STRING_LEN + 4 + 4 + 4 + 4 + 8;
/// Fewest bytes of a platform: renderer, level, kernel count, buffer count and the flag word.
const PLATFORM_LEN: usize = 5 * 4;
/// Fewest bytes of a constant buffer layout: its name, size and an empty member list.
const BUFFER_LEN: usize = STRING_LEN + 4 + 4;
/// Fewest bytes of a buffer member: its name and five words.
const MEMBER_LEN: usize = STRING_LEN + 5 * 4;
/// Thread group sizes have three dimensions.
const GROUP_DIMENSIONS: usize = 3;

/// The word a binding states when it has no sampler or dimension.
const NONE: u32 = u32::MAX;
const SET_SHIFT: u32 = 16;
const SET_MASK: u32 = 0x7f;
const TEXTURE_FLAG: u32 = 1 << 23;
const WRITABLE_FLAG: u32 = 1 << 25;
const IMAGE_FLAG: u32 = 1 << 31;

/// SPIR-V's magic number, and SMOL-V's, as the first four bytes of a module read little-endian.
const SPIRV_MAGIC: u32 = 0x0723_0203;
const SMOLV_MAGIC: u32 = 0x534d_4f4c;

/// Compiled code. Its debug form gives only its length.
#[derive(Clone, PartialEq, Eq, Default)]
pub struct Code(pub Vec<u8>);

impl std::fmt::Debug for Code {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Code({} bytes)", self.0.len())
    }
}

impl Deref for Code {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.0
    }
}

/// Where a binding is bound: the word the object stores.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ComputeSlot(pub u32);

impl ComputeSlot {
    /// The binding index, the low 16 bits.
    #[must_use]
    pub const fn binding(self) -> u16 {
        (self.0 & 0xffff) as u16
    }

    /// The descriptor set, the seven bits above the binding. Only sets 0 and 1 have been seen.
    #[must_use]
    pub const fn set(self) -> u16 {
        ((self.0 >> SET_SHIFT) & SET_MASK) as u16
    }

    /// Whether the slot is a texture's, including a storage image's.
    #[must_use]
    pub const fn is_texture(self) -> bool {
        self.0 & TEXTURE_FLAG != 0
    }

    /// Whether the slot is a writable resource's: a writable buffer or a storage image.
    #[must_use]
    pub const fn is_writable(self) -> bool {
        self.0 & WRITABLE_FLAG != 0
    }

    /// Whether the slot is a storage image's.
    #[must_use]
    pub const fn is_image(self) -> bool {
        self.0 & IMAGE_FLAG != 0
    }
}

/// A constant buffer, texture, buffer or writable resource a variant binds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComputeBinding {
    pub name: String,
    /// A second name, empty in every binding of the player checked.
    pub alias: String,
    pub slot: ComputeSlot,
    /// For a texture, the binding of the separate sampler it is read with, if it has one.
    pub sampler: Option<u32>,
    /// For a texture or storage image, its dimension as a number in which 2, 3 and 5 are the 2D,
    /// 3D and 2D array textures the modules of the player declare there.
    pub dimension: Option<i32>,
    /// The last word, zero in every binding of the player checked.
    pub reserved: u32,
}

impl ComputeBinding {
    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        let option = |word: u32| (word != NONE).then_some(word);
        Ok(Self {
            name: r.string()?,
            alias: r.string()?,
            slot: ComputeSlot(r.u32()?),
            sampler: option(r.u32()?),
            dimension: option(r.u32()?).map(u32::cast_signed),
            reserved: r.u32()?,
        })
    }

    fn list(r: &mut ObjectReader<'_>, what: &'static str) -> Result<Vec<Self>, SerializedError> {
        r.array(what, BINDING_LEN, Self::read)
    }
}

/// A sampler the platform declares by its state: a packed sampler state and the binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ComputeSampler {
    pub state: u32,
    pub bind_point: u32,
}

/// One compiled variant of a kernel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComputeVariant {
    pub constant_buffers: Vec<ComputeBinding>,
    pub textures: Vec<ComputeBinding>,
    pub samplers: Vec<ComputeSampler>,
    /// Buffers the kernel reads.
    pub buffers: Vec<ComputeBinding>,
    /// Buffers and images the kernel writes.
    pub writable: Vec<ComputeBinding>,
    /// The compiled code: SPIR-V on a SPIR-V platform, GLSL text on a GLSL one.
    pub code: Code,
    pub thread_group_size: [u32; 3],
    /// A word that is 50241 in every variant of the player checked.
    pub requirements: u32,
    /// The variant's own keyword list, empty in the player checked.
    pub keywords: Vec<String>,
    /// The list that follows every variant but a kernel's last: empty or one empty name in the
    /// player checked. A kernel's last variant has none.
    pub following: Vec<String>,
}

impl ComputeVariant {
    /// The variant's SPIR-V module as words, from raw SPIR-V or SMOL-V code.
    ///
    /// # Errors
    /// Fails when the code is neither, which a GLSL platform's is.
    pub fn module_words(&self) -> Result<Vec<u32>, SerializedError> {
        let invalid = |reason| SerializedError::InvalidComputeShader { offset: 0, reason };
        let magic = self
            .code
            .first_chunk::<4>()
            .map(|bytes| u32::from_le_bytes(*bytes));
        match magic {
            Some(SMOLV_MAGIC) => smolv::decode(&self.code),
            Some(SPIRV_MAGIC) if self.code.len().is_multiple_of(4) => Ok(self
                .code
                .chunks_exact(4)
                .map(|word| u32::from_le_bytes([word[0], word[1], word[2], word[3]]))
                .collect()),
            Some(SPIRV_MAGIC) => Err(invalid("the SPIR-V module is not a whole number of words")),
            _ => Err(invalid("the code is neither SPIR-V nor SMOL-V")),
        }
    }

    fn read(r: &mut ObjectReader<'_>, last: bool) -> Result<Self, SerializedError> {
        let constant_buffers = ComputeBinding::list(r, "constant buffer")?;
        let textures = ComputeBinding::list(r, "texture binding")?;
        let samplers = r.array("built-in sampler", SAMPLER_LEN, |r| {
            Ok(ComputeSampler {
                state: r.u32()?,
                bind_point: r.u32()?,
            })
        })?;
        let buffers = ComputeBinding::list(r, "buffer binding")?;
        let writable = ComputeBinding::list(r, "writable binding")?;
        let code = Code(r.array("compiled code", 1, ObjectReader::u8)?);
        let thread_group_size = read_group_size(r)?;
        let requirements = r.u32()?;
        let keywords = names(r, "variant keyword")?;
        let following = if last {
            Vec::new()
        } else {
            names(r, "variant keyword")?
        };
        Ok(Self {
            constant_buffers,
            textures,
            samplers,
            buffers,
            writable,
            code,
            thread_group_size,
            requirements,
            keywords,
            following,
        })
    }
}

fn invalid(r: &ObjectReader<'_>, reason: &'static str) -> SerializedError {
    SerializedError::InvalidComputeShader {
        offset: r.position(),
        reason,
    }
}

fn read_group_size(r: &mut ObjectReader<'_>) -> Result<[u32; 3], SerializedError> {
    let size = r.array("thread group size", 4, ObjectReader::u32)?;
    <[u32; GROUP_DIMENSIONS]>::try_from(size)
        .map_err(|_| invalid(r, "a thread group size states other than three dimensions"))
}

fn names(r: &mut ObjectReader<'_>, what: &'static str) -> Result<Vec<String>, SerializedError> {
    r.array(what, STRING_LEN, ObjectReader::string)
}

/// One kernel: its name and its compiled variants.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComputeKernel {
    pub name: String,
    /// A list of words after the variant count: `[0]` in 94 of the 104 kernels of the player
    /// checked, empty in 8 and `[1]` in 2.
    pub header: Vec<u32>,
    pub variants: Vec<ComputeVariant>,
    /// The keywords of each variant, in the order the keyword names list them and joined by
    /// spaces, with the index of the variant. A kernel without keywords has the one entry
    /// `("", 0)`.
    pub keyword_variants: Vec<(String, u32)>,
    pub keyword_names: Vec<String>,
    /// Two words that are zero in every kernel of the player checked.
    pub trailing: [u32; 2],
}

impl ComputeKernel {
    /// The thread group size the kernel's first variant states, or `None` when it states none
    /// (all three dimensions zero).
    #[must_use]
    pub fn thread_group_size(&self) -> Option<[u32; 3]> {
        self.variants
            .first()
            .map(|variant| variant.thread_group_size)
            .filter(|size| size.iter().any(|&dimension| dimension != 0))
    }

    /// The variant compiled for exactly the `enabled` keywords, which are written in the order
    /// the kernel names them. `None` when a keyword is not one the kernel names or no variant has
    /// that combination.
    #[must_use]
    pub fn variant_for(&self, enabled: &[&str]) -> Option<&ComputeVariant> {
        if !enabled
            .iter()
            .all(|keyword| self.keyword_names.iter().any(|name| name == keyword))
        {
            return None;
        }
        let combination: Vec<&str> = self
            .keyword_names
            .iter()
            .map(String::as_str)
            .filter(|name| enabled.contains(name))
            .collect();
        let key = combination.join(" ");
        let (_, index) = self
            .keyword_variants
            .iter()
            .find(|(keywords, _)| *keywords == key)?;
        self.variants.get(usize::try_from(*index).ok()?)
    }

    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        let name = r.string()?;
        let variant_count = r.count("kernel variant", VARIANT_LEN)?;
        if variant_count == 0 {
            return Err(invalid(r, "a kernel has no variants"));
        }
        let header = r.array("kernel header word", 4, ObjectReader::u32)?;
        let variants = (0..variant_count)
            .map(|index| ComputeVariant::read(r, index + 1 == variant_count))
            .collect::<Result<_, _>>()?;
        let keyword_variants = r.array("keyword combination", STRING_LEN + 4, |r| {
            Ok((r.string()?, r.u32()?))
        })?;
        let keyword_names = names(r, "kernel keyword")?;
        Ok(Self {
            name,
            header,
            variants,
            keyword_variants,
            keyword_names,
            trailing: [r.u32()?, r.u32()?],
        })
    }
}

/// A member of a constant buffer: its name, scalar kind (0 float, 1 int), byte offset, array size
/// (0 when it is not an array), rows and columns. A vector has one row and a matrix as many rows
/// as columns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComputeMember {
    pub name: String,
    pub kind: i32,
    pub offset: i32,
    pub array_size: i32,
    pub rows: i32,
    pub columns: i32,
}

impl ComputeMember {
    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        Ok(Self {
            name: r.string()?,
            kind: r.i32()?,
            offset: r.i32()?,
            array_size: r.i32()?,
            rows: r.i32()?,
            columns: r.i32()?,
        })
    }
}

/// The layout of a constant buffer a platform's kernels bind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComputeBuffer {
    pub name: String,
    pub size: u32,
    pub members: Vec<ComputeMember>,
}

impl ComputeBuffer {
    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        Ok(Self {
            name: r.string()?,
            size: r.u32()?,
            members: r.array("buffer member", MEMBER_LEN, ComputeMember::read)?,
        })
    }
}

/// The kernels of a shader compiled for one renderer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComputePlatform {
    pub renderer: u32,
    pub level: u32,
    pub kernels: Vec<ComputeKernel>,
    pub constant_buffers: Vec<ComputeBuffer>,
    /// The flag word that ends a platform: 1 in every platform of the player checked.
    pub resolved: bool,
}

impl ComputePlatform {
    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        let renderer = r.u32()?;
        let level = r.u32()?;
        let kernel_count = r.count("kernel", KERNEL_LEN)?;
        let kernels = (0..kernel_count)
            .map(|_| ComputeKernel::read(r))
            .collect::<Result<_, _>>()?;
        let constant_buffers =
            r.array("constant buffer layout", BUFFER_LEN, ComputeBuffer::read)?;
        Ok(Self {
            renderer,
            level,
            kernels,
            constant_buffers,
            resolved: r.u32()? != 0,
        })
    }
}

/// A compute shader: its name and its platforms.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComputeShader {
    pub name: String,
    pub platforms: Vec<ComputePlatform>,
}

impl ComputeShader {
    /// The kernels in the order a script addresses them by index: those of the first platform.
    /// Every platform of the player checked lists the same kernels in the same order.
    #[must_use]
    pub fn kernels(&self) -> &[ComputeKernel] {
        self.platforms
            .first()
            .map_or(&[], |platform| platform.kernels.as_slice())
    }

    /// The platform compiled for `renderer`, such as [`SPIRV_RENDERER`].
    #[must_use]
    pub fn platform(&self, renderer: u32) -> Option<&ComputePlatform> {
        self.platforms
            .iter()
            .find(|platform| platform.renderer == renderer)
    }
}

impl Layout for ComputeShader {
    const CLASS: ClassId = ClassId::COMPUTE_SHADER;

    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        let name = r.string()?;
        let count = r.count("compute shader platform", PLATFORM_LEN)?;
        let platforms = (0..count)
            .map(|_| ComputePlatform::read(r))
            .collect::<Result<_, _>>()?;
        Ok(Self { name, platforms })
    }
}
