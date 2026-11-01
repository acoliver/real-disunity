//! Reading and decoding individual objects of a built player, without loading whole files.

use std::{
    collections::BTreeMap,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

use worldrunner_unity_serialized::{
    ClassId, ObjectInfo, SerializedError, SerializedFile,
    animator::{AnimationClip, Animator, AnimatorOverrideController},
    animator_controller::AnimatorController,
    camera::Camera,
    classes::{GameObject, MonoBehaviour, MonoScript, RectTransform, Transform},
    font::Font,
    graphics::{
        Material, MeshFilter, MeshRenderer, SkinnedMeshRenderer, Texture2D, Texture2DArray,
    },
    graphics_settings::GraphicsSettings,
    light::Light,
    managers::{
        BuildSettings, InputManager, PreloadData, QualitySettings, ResourceManager, TagManager,
    },
    mesh::Mesh,
    object::{Layout, decode},
    render_settings::RenderSettings,
    render_texture::RenderTexture,
    shader::Shader,
    shader_program::PlayerSubProgram,
    shader_records::{
        PlatformProgram, SPIRV_PROGRAM, platform_program_types, platform_programs, vulkan_stages,
    },
    smolv,
    sorting_group::SortingGroup,
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

    pub(crate) fn error(&self, source: SerializedError) -> PlayerError {
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
    let (_, kind, chunks) = load_shader(open, path_id, platform)?;
    Ok((kind, chunks))
}

/// Decodes a shader, returning it with its object bytes.
fn decode_shader(open: &mut OpenFile, path_id: i64) -> Result<(Shader, Vec<u8>), PlayerError> {
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
    Ok((shader, bytes))
}

/// A shader's platforms, as Unity's `ShaderCompilerPlatform` values in platform order.
///
/// # Errors
/// Fails if the object is missing or is not a shader that decodes.
pub fn shader_platforms(open: &mut OpenFile, path_id: i64) -> Result<Vec<u32>, PlayerError> {
    decode_shader(open, path_id).map(|(shader, _)| shader.compiled.platforms)
}

/// Decodes a shader and decompresses one platform's chunks, with the platform's
/// `ShaderCompilerPlatform`.
fn load_shader(
    open: &mut OpenFile,
    path_id: i64,
    platform: usize,
) -> Result<(Shader, u32, Vec<Vec<u8>>), PlayerError> {
    let (shader, bytes) = decode_shader(open, path_id)?;
    let chunks = shader
        .compiled
        .platform_chunks(&bytes, platform)
        .map_err(|source| open.error(source))?;
    let kind = shader
        .compiled
        .platforms
        .get(platform)
        .copied()
        .unwrap_or_default();
    Ok((shader, kind, chunks))
}

/// A variant's blob index, sub-shader and pass index, and stage name.
type VariantLabel = (u32, (usize, usize), &'static str);

/// Every variant of a shader, labelled with its blob index, pass and stage.
fn variants(shader: &Shader) -> Vec<(VariantLabel, &PlayerSubProgram)> {
    let mut found = Vec::new();
    for (sub_index, sub_shader) in shader.parsed.sub_shaders.iter().enumerate() {
        for (pass_index, pass) in sub_shader.passes.iter().enumerate() {
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
                found.extend(all.map(|v| ((v.blob_index, (sub_index, pass_index), stage), v)));
            }
        }
    }
    found
}

/// Describes one program: its chunk, type, keywords, code and stage modules.
fn describe_program(
    program: &PlatformProgram<VariantLabel>,
    chunks: &[Vec<u8>],
) -> Result<String, SerializedError> {
    let (blob_index, (sub_shader, pass), stage) = program.label;
    let record = &program.record;
    let code = program
        .code(chunks)
        .ok_or(SerializedError::InvalidShaderProgram {
            offset: 0,
            reason: "the program's code lies outside the chunks",
        })?;
    let stages = if i64::from(record.program_type) == i64::from(SPIRV_PROGRAM) {
        vulkan_stages(code)?
            .iter()
            .map(|stage| format!("{}:{:?}:{}", stage.slot, stage.encoding, stage.range.len()))
            .collect::<Vec<_>>()
            .join(",")
    } else {
        "-".to_owned()
    };
    Ok(format!(
        "{blob_index} pass={sub_shader}.{pass} stage={stage} chunk={} type={} keywords={:?} \
         code={} stages={stages} rest={}",
        program.entry.segment,
        record.program_type,
        record.keywords,
        code.len(),
        record.rest.len()
    ))
}

/// Lists the programs of one shader platform, one line per variant compiled for it.
///
/// # Errors
/// Fails if the shader's chunks do not decompress, the platform's program types are not
/// known, or [`platform_programs`] or a Vulkan program's stage table rejects the data.
pub fn describe_shader_programs(
    open: &mut OpenFile,
    path_id: i64,
    platform: usize,
) -> Result<String, PlayerError> {
    let (shader, kind, chunks) = load_shader(open, path_id, platform)?;
    let types = platform_program_types(kind).ok_or_else(|| {
        open.error(SerializedError::InvalidShaderProgram {
            offset: 0,
            reason: "the program types of this shader platform are not known",
        })
    })?;
    let lines = platform_programs(variants(&shader), &types, &chunks)
        .and_then(|programs| {
            programs
                .iter()
                .map(|program| describe_program(program, &chunks))
                .collect::<Result<Vec<_>, _>>()
        })
        .map_err(|source| open.error(source))?;
    Ok(lines.join("\n"))
}

/// One decoded stage module of a shader's Vulkan program.
#[derive(Debug)]
pub struct ShaderModule {
    /// The program's blob entry.
    pub blob_index: u32,
    /// The stage slot in the program's wrapper.
    pub slot: usize,
    /// The SPIR-V module, decoded from SMOL-V where it was stored that way.
    pub words: Vec<u32>,
    /// The execution model of each entry point, such as 0 for vertex or 4 for fragment.
    pub entry_points: Vec<u32>,
}

/// Decodes every stage module of a shader's Vulkan programs on one platform. A program
/// several variants share is decoded once.
///
/// # Errors
/// Fails if the platform is not Vulkan, the shader's programs cannot be read, or a module is
/// not valid SMOL-V or SPIR-V.
pub fn shader_modules(
    open: &mut OpenFile,
    path_id: i64,
    platform: usize,
) -> Result<Vec<ShaderModule>, PlayerError> {
    let (shader, kind, chunks) = load_shader(open, path_id, platform)?;
    let types = platform_program_types(kind)
        .filter(|types| types.contains(&SPIRV_PROGRAM))
        .ok_or_else(|| {
            open.error(SerializedError::InvalidShaderProgram {
                offset: 0,
                reason: "the shader platform is not Vulkan",
            })
        })?;
    let mut programs = platform_programs(variants(&shader), &types, &chunks)
        .map_err(|source| open.error(source))?;
    programs.sort_by_key(|program| program.label.0);
    programs.dedup_by_key(|program| program.label.0);
    let mut modules = Vec::new();
    for program in &programs {
        decode_modules(program, &chunks, &mut modules).map_err(|source| open.error(source))?;
    }
    Ok(modules)
}

fn decode_modules(
    program: &PlatformProgram<VariantLabel>,
    chunks: &[Vec<u8>],
    modules: &mut Vec<ShaderModule>,
) -> Result<(), SerializedError> {
    let code = program
        .code(chunks)
        .ok_or(SerializedError::InvalidShaderProgram {
            offset: 0,
            reason: "the program's code lies outside the chunks",
        })?;
    for stage in vulkan_stages(code)? {
        let words = smolv::module_words(stage.encoding, &code[stage.range.clone()])?;
        modules.push(ShaderModule {
            blob_index: program.label.0,
            slot: stage.slot,
            entry_points: smolv::entry_points(&words)?,
            words,
        });
    }
    Ok(())
}

/// A layout's class with its check. The class comes from the layout, so the two cannot drift.
const fn layout<T: Layout + std::fmt::Debug>() -> (ClassId, Check) {
    (T::CLASS, check::<T>)
}

/// The classes with layouts, each with the check [`decode_census`] runs on its objects.
const LAYOUTS: [(ClassId, Check); 36] = [
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
    layout::<Texture2DArray>(),
    layout::<RenderTexture>(),
    layout::<Mesh>(),
    layout::<Camera>(),
    layout::<Light>(),
    layout::<Shader>(),
    layout::<CanvasRenderer>(),
    layout::<Canvas>(),
    layout::<CanvasGroup>(),
    layout::<SortingGroup>(),
    layout::<TextAsset>(),
    layout::<Sprite>(),
    layout::<SpriteAtlas>(),
    layout::<Font>(),
    layout::<BuildSettings>(),
    layout::<TagManager>(),
    layout::<ResourceManager>(),
    layout::<PreloadData>(),
    layout::<RenderSettings>(),
    layout::<GraphicsSettings>(),
    layout::<QualitySettings>(),
    layout::<InputManager>(),
    layout::<Animator>(),
    layout::<AnimatorController>(),
    layout::<AnimatorOverrideController>(),
    layout::<AnimationClip>(),
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
