//! Walks the Vulkan variants of a player's shaders: each variant's decoded parameters beside the
//! SPIR-V words of its stage modules. The reports that check or lay out a player's variants
//! share this walk and keep their own counts.

use std::{collections::BTreeSet, ops::RangeInclusive, path::PathBuf};

use worldrunner_unity_serialized::{
    ClassId, SerializedError,
    shader::Shader,
    shader_parameters::{VariantParameters, read_parameters},
    shader_program::Program,
    shader_records::{
        ProgramEntry, platform_program_types, program_table, read_record, vulkan_stages,
    },
    smolv,
};

use crate::{
    PlayerError,
    player_objects::{OpenFile, load_shader, shader_platforms},
    player_parameters::shader_programs,
    player_shaders::VulkanCensus,
};

/// A stage module of a variant, as SPIR-V words, with the slot of its stage in the program
/// record.
pub struct RawStage {
    pub slot: usize,
    pub words: Vec<u32>,
}

/// What the walk finds.
pub enum Event<'a> {
    /// A shader with a Vulkan platform, before its variants.
    Shader,
    /// A variant that failed before its parameters and modules could be read together, by cause.
    Unreadable(String),
    /// A variant whose record and parameters both decode, each pair once for its shader.
    Variant {
        parameters: &'a VariantParameters,
        stages: &'a [RawStage],
    },
}

const MISSING: SerializedError = SerializedError::InvalidShaderProgram {
    offset: 0,
    reason: "a variant names an entry that does not exist",
};

/// The stage modules of a program record.
fn read_stages(record: &[u8]) -> Result<Vec<RawStage>, SerializedError> {
    let read = read_record(record)?;
    let outside = || SerializedError::InvalidShaderProgram {
        offset: 0,
        reason: "the record's code lies outside it",
    };
    let code = record.get(read.code.clone()).ok_or_else(outside)?;
    let mut stages = Vec::new();
    for stage in vulkan_stages(code)? {
        let bytes = code.get(stage.range.clone()).ok_or_else(outside)?;
        stages.push(RawStage {
            slot: stage.slot,
            words: smolv::module_words(stage.encoding, bytes)?,
        });
    }
    Ok(stages)
}

fn cause(error: &SerializedError) -> String {
    match error {
        SerializedError::InvalidShaderProgram { reason, .. } => (*reason).to_owned(),
        SerializedError::InvalidCount { what, .. } => format!("a {what} count does not fit"),
        SerializedError::Truncated { .. } => "the data is truncated".to_owned(),
        SerializedError::InvalidString { .. } => "a name is not UTF-8".to_owned(),
        _ => "another error".to_owned(),
    }
}

/// A Vulkan platform's blob table with the chunks it indexes.
struct Platform<'a> {
    chunks: &'a [Vec<u8>],
    table: Vec<ProgramEntry>,
}

impl<'a> Platform<'a> {
    fn entry(&self, index: u32) -> Result<&'a [u8], SerializedError> {
        usize::try_from(index)
            .ok()
            .and_then(|index| self.table.get(index))
            .and_then(|entry| entry.bytes(self.chunks))
            .ok_or(MISSING)
    }
}

type Visit<'v> = dyn FnMut(Event<'_>) + 'v;

fn walk_program(
    program: &Program,
    platform: &Platform<'_>,
    types: &RangeInclusive<i8>,
    seen: &mut BTreeSet<(u32, u32)>,
    visit: &mut Visit<'_>,
) {
    let Ok(pairs) = program.variant_blobs() else {
        visit(Event::Unreadable("parameter lists do not pair".to_owned()));
        return;
    };
    for (variant, entry) in pairs {
        if !types.contains(&variant.gpu_program_type) || !seen.insert((variant.blob_index, entry)) {
            continue;
        }
        let decoded = platform
            .entry(variant.blob_index)
            .and_then(read_stages)
            .and_then(|stages| Ok((stages, read_parameters(platform.entry(entry)?)?)));
        match decoded {
            Ok((stages, parameters)) => visit(Event::Variant {
                parameters: &parameters,
                stages: &stages,
            }),
            Err(error) => visit(Event::Unreadable(cause(&error))),
        }
    }
}

fn walk_platform(
    shader: &Shader,
    kind: u32,
    chunks: &[Vec<u8>],
    visit: &mut Visit<'_>,
) -> Result<(), SerializedError> {
    let lengths: Vec<usize> = chunks.iter().map(Vec::len).collect();
    let types = platform_program_types(kind).ok_or(MISSING)?;
    let table = match chunks.first() {
        Some(first) => program_table(first, &lengths)?,
        None => Vec::new(),
    };
    let platform = Platform { chunks, table };
    let mut seen = BTreeSet::new();
    for program in shader_programs(shader) {
        walk_program(program, &platform, &types, &mut seen, visit);
    }
    Ok(())
}

/// Visits every Vulkan variant of the shaders in `paths`.
///
/// # Errors
/// Fails if a file cannot be read, a shader does not load, or the player has shaders and none
/// with a Vulkan platform; a variant that does not decode is an [`Event::Unreadable`], not an
/// error.
pub fn walk_variants(
    paths: &[PathBuf],
    visit: &mut impl FnMut(Event<'_>),
) -> Result<(), PlayerError> {
    let mut census = VulkanCensus::default();
    for path in paths {
        let mut open = OpenFile::open(path)?;
        let shaders: Vec<i64> = open
            .file
            .objects()
            .iter()
            .filter(|object| object.class_id == ClassId::SHADER)
            .map(|object| object.path_id)
            .collect();
        for path_id in shaders {
            let platforms = shader_platforms(&mut open, path_id)?;
            let Some(platform) = census.platform(&platforms) else {
                continue;
            };
            visit(Event::Shader);
            let (shader, kind, chunks) = load_shader(&mut open, path_id, platform)?;
            walk_platform(&shader, kind, &chunks, visit).map_err(|source| open.error(source))?;
        }
    }
    census.finish()
}
