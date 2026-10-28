//! Reading and decoding individual objects of a built player, without loading whole files.

use std::{
    collections::BTreeMap,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

use worldrunner_unity_serialized::{
    ClassId, ObjectInfo, SerializedError, SerializedFile,
    camera::Camera,
    classes::{GameObject, MonoBehaviour, MonoScript, RectTransform, Transform},
    font::Font,
    graphics::{Material, MeshFilter, MeshRenderer, SkinnedMeshRenderer, Texture2D},
    light::Light,
    managers::{BuildSettings, PreloadData, ResourceManager, TagManager},
    mesh::Mesh,
    object::{Layout, decode},
    render_settings::RenderSettings,
    shader::Shader,
    shader_program::PlayerSubProgram,
    shader_records::{
        ProgramEntry, platform_program_types, program_table, read_record, record_type,
        vulkan_stages,
    },
    sprite::{Sprite, SpriteAtlas},
    text_asset::TextAsset,
    ui::{Canvas, CanvasGroup, CanvasRenderer},
};

use crate::{PlayerError, read_metadata};

/// One serialized file opened for reading objects.
pub struct OpenFile {
    pub path: PathBuf,
    pub file: SerializedFile,
    handle: File,
}

impl OpenFile {
    /// Reads the file's metadata and keeps it open for object reads.
    ///
    /// # Errors
    /// Fails if the file cannot be read or is not a supported serialized file.
    pub fn open(path: &Path) -> Result<Self, PlayerError> {
        let file = read_metadata(path)?;
        let handle = File::open(path).map_err(|source| PlayerError::Io {
            path: path.to_owned(),
            source,
        })?;
        Ok(Self {
            path: path.to_owned(),
            file,
            handle,
        })
    }

    /// The bytes of one object.
    ///
    /// # Errors
    /// Fails if the object's range cannot be read.
    pub fn bytes(&mut self, object: &ObjectInfo) -> Result<Vec<u8>, PlayerError> {
        let io_error = |source| PlayerError::Io {
            path: self.path.clone(),
            source,
        };
        let len = self.handle.metadata().map_err(io_error)?.len();
        if object
            .offset
            .checked_add(u64::from(object.size))
            .is_none_or(|end| end > len)
        {
            return Err(self.error(SerializedError::ObjectOutOfBounds {
                path_id: object.path_id,
                start: object.offset,
                size: object.size,
                file_size: len,
            }));
        }
        let mut bytes = vec![0; object.size as usize];
        self.handle
            .seek(SeekFrom::Start(object.offset))
            .map_err(io_error)?;
        self.handle.read_exact(&mut bytes).map_err(io_error)?;
        Ok(bytes)
    }

    /// Decodes the object with this path ID using `T`'s layout.
    ///
    /// # Errors
    /// Fails if the object is missing, of another class, or does not decode.
    pub fn decode<T: Layout>(&mut self, path_id: i64) -> Result<T, PlayerError> {
        let object = *self
            .file
            .object(path_id)
            .ok_or_else(|| PlayerError::MissingObject {
                path: self.path.clone(),
                path_id,
            })?;
        let bytes = self.bytes(&object)?;
        decode(&bytes, &object, self.file.header.big_endian).map_err(|source| self.error(source))
    }

    fn error(&self, source: SerializedError) -> PlayerError {
        PlayerError::Serialized {
            path: self.path.clone(),
            source,
        }
    }
}

/// How many objects of one class decoded and failed, with the first failures.
#[derive(Debug, Default)]
pub struct ClassCensus {
    pub decoded: usize,
    pub failed: usize,
    pub examples: Vec<String>,
}

/// Decodes an object with one class's layout and describes the result.
type Check = fn(&[u8], &ObjectInfo, bool) -> Result<String, SerializedError>;

fn check<T: Layout + std::fmt::Debug>(
    bytes: &[u8],
    object: &ObjectInfo,
    big_endian: bool,
) -> Result<String, SerializedError> {
    decode::<T>(bytes, object, big_endian).map(|value| format!("{value:#?}"))
}

/// Decodes one object with its class's layout and describes its fields.
///
/// # Errors
/// Fails if the object is missing, cannot be read, has no layout, or does not decode.
pub fn describe_object(open: &mut OpenFile, path_id: i64) -> Result<String, PlayerError> {
    let object = *open
        .file
        .object(path_id)
        .ok_or_else(|| PlayerError::MissingObject {
            path: open.path.clone(),
            path_id,
        })?;
    let check = layout_check(object.class_id)
        .ok_or_else(|| open.error(SerializedError::UnsupportedClass(object.class_id)))?;
    let bytes = open.bytes(&object)?;
    check(&bytes, &object, open.file.header.big_endian).map_err(|source| open.error(source))
}

/// Decompresses the compiled program chunks of one platform of a shader, returning Unity's
/// `ShaderCompilerPlatform` of that platform with the chunks.
///
/// # Errors
/// Fails if the object is missing, is not a shader, does not decode, or its chunks for the
/// platform do not decompress.
pub fn shader_chunks(
    open: &mut OpenFile,
    path_id: i64,
    platform: usize,
) -> Result<(u32, Vec<Vec<u8>>), PlayerError> {
    let (shader, chunks) = load_shader(open, path_id, platform)?;
    let kind = shader
        .compiled
        .platforms
        .get(platform)
        .copied()
        .unwrap_or_default();
    Ok((kind, chunks))
}

/// Decodes a shader and decompresses one platform's chunks.
fn load_shader(
    open: &mut OpenFile,
    path_id: i64,
    platform: usize,
) -> Result<(Shader, Vec<Vec<u8>>), PlayerError> {
    let object = *open
        .file
        .object(path_id)
        .ok_or_else(|| PlayerError::MissingObject {
            path: open.path.clone(),
            path_id,
        })?;
    let bytes = open.bytes(&object)?;
    let shader = decode::<Shader>(&bytes, &object, open.file.header.big_endian)
        .map_err(|source| open.error(source))?;
    let chunks = shader
        .compiled
        .platform_chunks(&bytes, platform)
        .map_err(|source| open.error(source))?;
    Ok((shader, chunks))
}

/// Describes one program record: its chunk, type, keywords, code and stage modules.
fn describe_record(entry: &ProgramEntry, record: &[u8]) -> Result<String, SerializedError> {
    let program = read_record(record)?;
    let code = &record[program.code.clone()];
    let stages = if program.program_type == VULKAN_PROGRAM {
        vulkan_stages(code)?
            .iter()
            .map(|stage| format!("{}:{:?}:{}", stage.slot, stage.encoding, stage.range.len()))
            .collect::<Vec<_>>()
            .join(",")
    } else {
        "-".to_owned()
    };
    Ok(format!(
        "segment={} type={} keywords={:?} code={} stages={stages} rest={}",
        entry.segment,
        program.program_type,
        program.keywords,
        code.len(),
        program.rest.len()
    ))
}

/// Unity's `ShaderGpuProgramType` for Vulkan SPIR-V programs.
const VULKAN_PROGRAM: u32 = 25;

/// Every variant of a shader, with its pass index and stage name.
fn variants(shader: &Shader) -> Vec<(usize, &'static str, &PlayerSubProgram)> {
    let passes = shader.parsed.sub_shaders.iter().flat_map(|sub| &sub.passes);
    let mut found = Vec::new();
    for (index, pass) in passes.enumerate() {
        let p = &pass.programs;
        for (stage, program) in [
            ("vertex", &p.vertex),
            ("fragment", &p.fragment),
            ("geometry", &p.geometry),
            ("hull", &p.hull),
            ("domain", &p.domain),
            ("ray_tracing", &p.ray_tracing),
        ] {
            let all = program.player_sub_programs.iter().flatten();
            found.extend(all.map(|variant| (index, stage, variant)));
        }
    }
    found
}

/// Lists the program records of one shader platform, one line per variant compiled for it.
///
/// # Errors
/// Fails if the shader's chunks do not decompress, the platform's program types are not
/// known, a variant names a missing entry or a record of another type, or a record or Vulkan
/// program is malformed.
pub fn describe_shader_programs(
    open: &mut OpenFile,
    path_id: i64,
    platform: usize,
) -> Result<String, PlayerError> {
    let (shader, chunks) = load_shader(open, path_id, platform)?;
    let kind = shader
        .compiled
        .platforms
        .get(platform)
        .copied()
        .unwrap_or_default();
    let types = platform_program_types(kind).ok_or_else(|| {
        open.error(SerializedError::InvalidShaderProgram {
            offset: 0,
            reason: "the program types of this shader platform are not known",
        })
    })?;
    let lengths: Vec<usize> = chunks.iter().map(Vec::len).collect();
    let first = chunks.first().map_or(&[][..], Vec::as_slice);
    let entries = program_table(first, &lengths).map_err(|source| open.error(source))?;
    let mut lines = Vec::new();
    for (pass, stage, variant) in variants(&shader) {
        if !types.contains(&variant.gpu_program_type) {
            continue;
        }
        let line =
            describe_variant(&entries, &chunks, variant).map_err(|source| open.error(source))?;
        lines.push(format!(
            "{} pass={pass} stage={stage} {line}",
            variant.blob_index
        ));
    }
    Ok(lines.join("\n"))
}

/// Describes a variant's program record, which must have the variant's program type.
fn describe_variant(
    entries: &[ProgramEntry],
    chunks: &[Vec<u8>],
    variant: &PlayerSubProgram,
) -> Result<String, SerializedError> {
    let invalid = |reason| SerializedError::InvalidShaderProgram { offset: 0, reason };
    let entry = usize::try_from(variant.blob_index)
        .ok()
        .and_then(|index| entries.get(index))
        .ok_or_else(|| invalid("a variant names a blob entry that does not exist"))?;
    let record = &chunks[entry.segment][entry.range.clone()];
    if i64::from(record_type(record)?) != i64::from(variant.gpu_program_type) {
        return Err(invalid("a variant's record has a different program type"));
    }
    describe_record(entry, record)
}

/// A layout's class with its check. The class comes from the layout, so the two cannot drift.
const fn layout<T: Layout + std::fmt::Debug>() -> (ClassId, Check) {
    (T::CLASS, check::<T>)
}

/// The classes with layouts, each with the check [`decode_census`] runs on its objects.
const LAYOUTS: [(ClassId, Check); 26] = [
    layout::<GameObject>(),
    layout::<Transform>(),
    layout::<RectTransform>(),
    layout::<MonoScript>(),
    layout::<MonoBehaviour>(),
    layout::<MeshFilter>(),
    layout::<MeshRenderer>(),
    layout::<SkinnedMeshRenderer>(),
    layout::<Material>(),
    layout::<Texture2D>(),
    layout::<Mesh>(),
    layout::<Camera>(),
    layout::<Light>(),
    layout::<Shader>(),
    layout::<CanvasRenderer>(),
    layout::<Canvas>(),
    layout::<CanvasGroup>(),
    layout::<TextAsset>(),
    layout::<Sprite>(),
    layout::<SpriteAtlas>(),
    layout::<Font>(),
    layout::<BuildSettings>(),
    layout::<TagManager>(),
    layout::<ResourceManager>(),
    layout::<PreloadData>(),
    layout::<RenderSettings>(),
];
const FAILURES_KEPT: usize = 5;

fn layout_check(class: ClassId) -> Option<Check> {
    LAYOUTS
        .iter()
        .find(|&&(c, _)| c == class)
        .map(|&(_, check)| check)
}

/// Decodes every object whose class has a layout in the given files and counts the results.
///
/// # Errors
/// Fails if a file cannot be opened or an object cannot be read; decoding failures are
/// counted, not returned.
pub fn decode_census(paths: &[PathBuf]) -> Result<BTreeMap<ClassId, ClassCensus>, PlayerError> {
    let mut census = BTreeMap::<ClassId, ClassCensus>::new();
    for path in paths {
        let mut open = OpenFile::open(path)?;
        let objects: Vec<_> = open.file.objects().to_vec();
        for object in &objects {
            let Some(check) = layout_check(object.class_id) else {
                continue;
            };
            let bytes = open.bytes(object)?;
            let big_endian = open.file.header.big_endian;
            let entry = census.entry(object.class_id).or_default();
            match check(&bytes, object, big_endian) {
                Ok(_) => entry.decoded += 1,
                Err(error) => {
                    entry.failed += 1;
                    if entry.examples.len() < FAILURES_KEPT {
                        entry.examples.push(format!("{}: {error}", path.display()));
                    }
                }
            }
        }
    }
    Ok(census)
}
