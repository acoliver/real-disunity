//! Serialized compute shader objects for tests, written the way a Unity 6 player stores them.
//!
//! The writer takes its own plain specs, not the reader's types, so a test that decodes what it
//! wrote checks the reader against this file's statement of the layout. The layout is the one
//! measured over the 16 compute shaders of the Old World player: every object is consumed to its
//! last byte by it.
//!
//! An object is its name and a list of platforms. A platform is a renderer id and level, its
//! kernels, its constant buffer layouts and one flag word. A kernel is its name, its variant
//! count, a list of header words, its variants, then a table from keyword combination to variant
//! index, the keyword names and two zero words. A variant is five binding lists, its code, its
//! thread group size, a requirements word and keyword lists.

#![allow(dead_code)]

use worldrunner_unity_serialized::write::ObjectWriter;

/// The renderer id of a platform that holds Vulkan SPIR-V, and of one that holds GLSL text.
pub const SPIRV: u32 = 21;
pub const GLSL: u32 = 17;

/// Flags in a binding's slot word, above the 16 bits of the binding and the seven of the set.
const TEXTURE_FLAG: u32 = 1 << 23;
const WRITABLE_FLAG: u32 = 1 << 25;
const IMAGE_FLAG: u32 = 1 << 31;
const NONE: u32 = u32::MAX;

/// A constant buffer, texture, buffer or writable buffer binding.
#[derive(Clone, Debug)]
pub struct Binding {
    pub name: String,
    pub slot: u32,
    pub sampler: Option<u32>,
    pub dimension: Option<i32>,
}

impl Binding {
    pub fn constant_buffer(name: &str, set: u32, binding: u32) -> Self {
        Self::plain(name, (set << 16) | binding)
    }

    pub fn texture(name: &str, binding: u32, sampler: Option<u32>, dimension: i32) -> Self {
        Self {
            name: name.to_owned(),
            slot: TEXTURE_FLAG | binding,
            sampler,
            dimension: Some(dimension),
        }
    }

    pub fn buffer(name: &str, binding: u32) -> Self {
        Self::plain(name, binding)
    }

    pub fn writable(name: &str, binding: u32) -> Self {
        Self::plain(name, WRITABLE_FLAG | binding)
    }

    pub fn image(name: &str, binding: u32, dimension: i32) -> Self {
        Self {
            name: name.to_owned(),
            slot: IMAGE_FLAG | WRITABLE_FLAG | TEXTURE_FLAG | binding,
            sampler: None,
            dimension: Some(dimension),
        }
    }

    fn plain(name: &str, slot: u32) -> Self {
        Self {
            name: name.to_owned(),
            slot,
            sampler: None,
            dimension: None,
        }
    }
}

/// One compiled variant of a kernel.
#[derive(Clone, Debug)]
pub struct Variant {
    pub constant_buffers: Vec<Binding>,
    pub textures: Vec<Binding>,
    /// Packed sampler state and bind point of each built-in sampler.
    pub samplers: Vec<(u32, u32)>,
    pub buffers: Vec<Binding>,
    pub writable: Vec<Binding>,
    pub code: Vec<u8>,
    pub size: [u32; 3],
    /// The list that follows every variant of a kernel but its last.
    pub following: Vec<String>,
}

impl Variant {
    pub fn new(code: &[u8], size: [u32; 3]) -> Self {
        Self {
            constant_buffers: Vec::new(),
            textures: Vec::new(),
            samplers: Vec::new(),
            buffers: Vec::new(),
            writable: Vec::new(),
            code: code.to_vec(),
            size,
            following: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Kernel {
    pub name: String,
    pub header: Vec<u32>,
    pub variants: Vec<Variant>,
    pub keyword_variants: Vec<(String, u32)>,
    pub keyword_names: Vec<String>,
}

impl Kernel {
    /// A kernel with one variant and no keywords.
    pub fn single(name: &str, variant: Variant) -> Self {
        Self {
            name: name.to_owned(),
            header: vec![0],
            variants: vec![variant],
            keyword_variants: vec![(String::new(), 0)],
            keyword_names: Vec::new(),
        }
    }
}

/// A member of a constant buffer: kind, offset, array size, rows and columns.
#[derive(Clone, Debug)]
pub struct Member {
    pub name: String,
    pub words: [i32; 5],
}

#[derive(Clone, Debug)]
pub struct BufferLayout {
    pub name: String,
    pub size: u32,
    pub members: Vec<Member>,
}

#[derive(Clone, Debug)]
pub struct Platform {
    pub renderer: u32,
    pub level: u32,
    pub kernels: Vec<Kernel>,
    pub buffers: Vec<BufferLayout>,
}

impl Platform {
    pub fn new(renderer: u32, level: u32, kernels: Vec<Kernel>) -> Self {
        Self {
            renderer,
            level,
            kernels,
            buffers: Vec::new(),
        }
    }
}

fn count(w: &mut ObjectWriter, len: usize) {
    w.u32(u32::try_from(len).unwrap());
}

fn names(w: &mut ObjectWriter, names: &[String]) {
    count(w, names.len());
    for name in names {
        w.string(name).unwrap();
    }
}

fn binding(w: &mut ObjectWriter, binding: &Binding) {
    w.string(&binding.name).unwrap();
    // An alias that every binding of the player checked leaves empty.
    w.string("").unwrap();
    w.u32(binding.slot);
    w.u32(binding.sampler.unwrap_or(NONE));
    w.u32(binding.dimension.map_or(NONE, i32::cast_unsigned));
    w.u32(0);
}

fn bindings(w: &mut ObjectWriter, list: &[Binding]) {
    count(w, list.len());
    for item in list {
        binding(w, item);
    }
}

fn variant(w: &mut ObjectWriter, variant: &Variant, last: bool) {
    bindings(w, &variant.constant_buffers);
    bindings(w, &variant.textures);
    count(w, variant.samplers.len());
    for &(state, bind_point) in &variant.samplers {
        w.u32(state).u32(bind_point);
    }
    bindings(w, &variant.buffers);
    bindings(w, &variant.writable);
    w.byte_array(&variant.code).unwrap();
    count(w, 3);
    for dimension in variant.size {
        w.u32(dimension);
    }
    w.u32(50241);
    // The keyword list of the variant itself, empty in the player checked.
    count(w, 0);
    if !last {
        names(w, &variant.following);
    }
}

fn kernel(w: &mut ObjectWriter, kernel: &Kernel) {
    w.string(&kernel.name).unwrap();
    count(w, kernel.variants.len());
    count(w, kernel.header.len());
    for &word in &kernel.header {
        w.u32(word);
    }
    for (index, item) in kernel.variants.iter().enumerate() {
        variant(w, item, index + 1 == kernel.variants.len());
    }
    count(w, kernel.keyword_variants.len());
    for (keywords, index) in &kernel.keyword_variants {
        w.string(keywords).unwrap();
        w.u32(*index);
    }
    names(w, &kernel.keyword_names);
    w.u32(0).u32(0);
}

fn buffer(w: &mut ObjectWriter, buffer: &BufferLayout) {
    w.string(&buffer.name).unwrap();
    w.u32(buffer.size);
    count(w, buffer.members.len());
    for member in &buffer.members {
        w.string(&member.name).unwrap();
        for word in member.words {
            w.i32(word);
        }
    }
}

fn platform(w: &mut ObjectWriter, platform: &Platform) {
    w.u32(platform.renderer).u32(platform.level);
    count(w, platform.kernels.len());
    for item in &platform.kernels {
        kernel(w, item);
    }
    count(w, platform.buffers.len());
    for item in &platform.buffers {
        buffer(w, item);
    }
    // The flag word every platform of the player checked ends with.
    w.u32(1);
}

/// A compute shader named `name` with `platforms`, in the byte order given.
pub fn compute_shader_bytes(name: &str, platforms: &[Platform], big_endian: bool) -> Vec<u8> {
    let mut w = ObjectWriter::new(big_endian);
    w.string(name).unwrap();
    count(&mut w, platforms.len());
    for item in platforms {
        platform(&mut w, item);
    }
    w.finish()
}
