//! A player shader: choosing a pass and a variant of its Vulkan platform and building the stage
//! programs of the variant.
//!
//! The walk follows `unity-import`'s variant walk, which this crate cannot depend on: the
//! platform's table of records, the program record of a variant, its parameters in a record of
//! their own and its vertex input table in the tail of the program record.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use worldrunner_unity_serialized::shader::{Pass, Shader};
use worldrunner_unity_serialized::shader_parameters::{
    VariantParameters, read_bind_channels, read_parameters,
};
use worldrunner_unity_serialized::shader_program::BindChannels;
use worldrunner_unity_serialized::shader_records::{
    ProgramEntry, SPIRV_PROGRAM, program_table, read_record, vulkan_stages,
};
use worldrunner_unity_serialized::shader_state::ShaderState;
use worldrunner_unity_serialized::spirv_prep::prepare_stages;
use worldrunner_unity_serialized::{SerializedError, smolv};

use crate::module::entry_point_names;
use crate::unity::module::{ModuleError, StageKind, StageModule, StageProgram};
use crate::unity::variant::{declared_keywords, select_variant};

/// Which pass of the shader a draw runs. The crate names no tag key and no tag value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PassSelector {
    /// The pass at this position in the first sub-shader, as a blit names it.
    Index(usize),
    /// The first pass whose tag `key` has the first of `values` that any pass has. Tag keys are
    /// compared without regard to ASCII case, because Unity stores some of them in upper case.
    /// Values are compared exactly.
    Tag { key: String, values: Vec<String> },
    /// As `Tag`, with a pass that has no tag `key` counted as having `untagged` for that key.
    /// A pass that has the tag keeps its own value.
    TagWithUntagged {
        key: String,
        values: Vec<String>,
        untagged: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ResolveError {
    #[error("the shader has no sub-shader")]
    NoSubShader,
    #[error("the first sub-shader has no pass selected by {selector}")]
    NoPass { selector: String },
    #[error("pass {pass} has no Vulkan variant for the keywords {keywords:?}: {reason}")]
    NoVariant {
        pass: usize,
        keywords: Vec<String>,
        reason: String,
    },
    #[error("the Vulkan platform of the shader cannot be read: {reason}")]
    Platform { reason: String },
    #[error("variant {blob} of pass {pass} cannot be built: {reason}")]
    Variant {
        pass: usize,
        blob: u32,
        reason: String,
    },
    #[error(transparent)]
    Module(#[from] ModuleError),
}

/// The stage programs of one variant, prepared together, with its vertex input table.
pub struct Variant {
    pub vertex: StageProgram,
    pub fragment: StageProgram,
    /// Pairs of a source vertex channel and the shader input it feeds.
    pub channels: BindChannels,
    pub parameters: VariantParameters,
}

/// A pass and variant chosen for a draw.
#[derive(Clone)]
pub struct Resolved {
    /// The position of the pass in the first sub-shader.
    pub pass: usize,
    /// The variant's record in the platform's table, which names it within the shader.
    pub blob: u32,
    pub state: Arc<ShaderState>,
    pub variant: Arc<Variant>,
}

/// A decoded shader and the decompressed chunks of its Vulkan platform. Variants are decoded and
/// prepared when first resolved, then kept.
pub struct ProgramShader {
    shader: Shader,
    chunks: Vec<Vec<u8>>,
    table: Vec<ProgramEntry>,
    states: Vec<Arc<ShaderState>>,
    variants: HashMap<(usize, u32), Arc<Variant>>,
}

fn platform(error: &SerializedError) -> ResolveError {
    ResolveError::Platform {
        reason: error.to_string(),
    }
}

impl ProgramShader {
    /// # Errors
    /// Fails when the chunks' table of records is malformed.
    pub fn new(shader: Shader, chunks: Vec<Vec<u8>>) -> Result<Self, ResolveError> {
        let lengths: Vec<usize> = chunks.iter().map(Vec::len).collect();
        let table = match chunks.first() {
            Some(first) => program_table(first, &lengths).map_err(|error| platform(&error))?,
            None => Vec::new(),
        };
        let states = shader
            .parsed
            .sub_shaders
            .iter()
            .flat_map(|sub| &sub.passes)
            .map(|pass| Arc::new(pass.state.clone()))
            .collect();
        Ok(Self {
            shader,
            chunks,
            table,
            states,
            variants: HashMap::new(),
        })
    }

    /// The passes of the first sub-shader.
    #[must_use]
    pub fn pass_count(&self) -> usize {
        self.shader
            .parsed
            .sub_shaders
            .first()
            .map_or(0, |sub| sub.passes.len())
    }

    /// The number of passes of the first sub-shader and the values of tag `key` that they have,
    /// in pass order; a pass without the tag adds none.
    #[must_use]
    pub fn pass_tags(&self, key: &str) -> (usize, Vec<String>) {
        let passes = self
            .shader
            .parsed
            .sub_shaders
            .first()
            .map_or(&[][..], |sub| sub.passes.as_slice());
        let tags = passes
            .iter()
            .filter_map(|pass| tag(pass, key).map(str::to_owned))
            .collect();
        (passes.len(), tags)
    }

    /// The value of tag `key` on pass `pass` of the first sub-shader.
    #[must_use]
    pub fn pass_tag(&self, pass: usize, key: &str) -> Option<String> {
        let passes = self.passes().ok()?;
        passes
            .get(pass)
            .and_then(|pass| tag(pass, key))
            .map(str::to_owned)
    }

    /// Variants decoded so far.
    #[must_use]
    pub fn cached_variants(&self) -> usize {
        self.variants.len()
    }

    /// Chooses the pass `selector` names and its variant whose keywords are exactly `keywords`,
    /// among those of the shader's keywords that the pass's program declares, and builds the
    /// variant on first use. A keyword the shader does not list cannot change a variant and is
    /// ignored.
    ///
    /// # Errors
    /// Fails with `NoPass` or `NoVariant` when nothing matches, never with another pass or
    /// variant, and when the variant cannot be decoded or prepared.
    pub fn resolve(
        &mut self,
        selector: &PassSelector,
        keywords: &[&str],
    ) -> Result<Resolved, ResolveError> {
        let pass = self.pass_index(selector)?;
        let (blob, parameters) = self.choose_variant(pass, keywords)?;
        let variant = if let Some(variant) = self.variants.get(&(pass, blob)) {
            variant.clone()
        } else {
            let built = Arc::new(self.build(pass, blob, parameters)?);
            self.variants.insert((pass, blob), built.clone());
            built
        };
        Ok(Resolved {
            pass,
            blob,
            state: self.states[pass].clone(),
            variant,
        })
    }

    fn passes(&self) -> Result<&[Pass], ResolveError> {
        self.shader
            .parsed
            .sub_shaders
            .first()
            .map(|sub| sub.passes.as_slice())
            .ok_or(ResolveError::NoSubShader)
    }

    fn pass_index(&self, selector: &PassSelector) -> Result<usize, ResolveError> {
        let passes = self.passes()?;
        let found = match selector {
            PassSelector::Index(index) => (*index < passes.len()).then_some(*index),
            PassSelector::Tag { key, values } => values.iter().find_map(|value| {
                passes
                    .iter()
                    .position(|pass| tag(pass, key) == Some(value.as_str()))
            }),
            PassSelector::TagWithUntagged {
                key,
                values,
                untagged,
            } => values.iter().find_map(|value| {
                passes
                    .iter()
                    .position(|pass| tag(pass, key).unwrap_or(untagged.as_str()) == value.as_str())
            }),
        };
        found.ok_or_else(|| ResolveError::NoPass {
            selector: format!("{selector:?}"),
        })
    }

    /// The record of the variant and the record of its parameters.
    fn choose_variant(&self, pass: usize, keywords: &[&str]) -> Result<(u32, u32), ResolveError> {
        let program = &self.passes()?[pass].programs.vertex;
        let no_variant = |reason: String| ResolveError::NoVariant {
            pass,
            keywords: keywords.iter().map(|&k| k.to_owned()).collect(),
            reason,
        };
        let names = &self.shader.parsed.keyword_names;
        let enabled: BTreeSet<u16> = keywords
            .iter()
            .filter_map(|keyword| names.iter().position(|name| name == keyword))
            .filter_map(|index| u16::try_from(index).ok())
            .collect();
        let pairs: Vec<_> = program
            .variant_blobs()
            .map_err(|error| no_variant(error.to_string()))?
            .into_iter()
            .filter(|(variant, _)| variant.gpu_program_type == SPIRV_PROGRAM)
            .collect();
        let (variant, parameters) = select_variant(
            &pairs,
            |(variant, _)| variant.keyword_indices.as_slice(),
            &declared_keywords(program),
            &enabled,
        )
        .map_err(|error| no_variant(error.to_string()))?;
        Ok((variant.blob_index, *parameters))
    }

    fn entry(&self, index: u32) -> Option<&[u8]> {
        usize::try_from(index)
            .ok()
            .and_then(|index| self.table.get(index))
            .and_then(|entry| entry.bytes(&self.chunks))
    }

    fn build(&self, pass: usize, blob: u32, parameters: u32) -> Result<Variant, ResolveError> {
        let fail = |reason: String| ResolveError::Variant { pass, blob, reason };
        let record = self
            .entry(blob)
            .ok_or_else(|| fail("the table has no such record".to_owned()))?;
        let parameters = self
            .entry(parameters)
            .ok_or_else(|| fail("the table has no such parameters".to_owned()))
            .and_then(|bytes| read_parameters(bytes).map_err(|error| fail(error.to_string())))?;
        let stages = stage_words(record).map_err(|error| fail(error.to_string()))?;
        let prepared = prepare_stages(&[&stages.vertex, &stages.fragment])
            .map_err(|error| fail(error.to_string()))?;
        let [vertex, fragment] = <[_; 2]>::try_from(prepared.modules)
            .map_err(|_| fail("preparing two stages gave another count".to_owned()))?;
        let stage = |module: worldrunner_unity_serialized::spirv_prep::PreparedModule,
                     kind: StageKind| {
            let entry_point = entry_point(&module.words)
                .ok_or_else(|| fail(format!("the {kind:?} module has no single entry point")))?;
            Ok::<_, ResolveError>(StageProgram::new(
                StageModule {
                    words: module.words,
                    entry_point,
                    kind,
                    bindings: module.bindings,
                },
                parameters.clone(),
            )?)
        };
        Ok(Variant {
            vertex: stage(vertex, StageKind::Vertex)?,
            fragment: stage(fragment, StageKind::Fragment)?,
            channels: stages.channels,
            parameters,
        })
    }
}

fn tag<'a>(pass: &'a Pass, key: &str) -> Option<&'a str> {
    pass.state
        .tags
        .iter()
        .chain(&pass.tags)
        .find(|(name, _)| name.eq_ignore_ascii_case(key))
        .map(|(_, value)| value.as_str())
}

/// The two stage modules of a program record and its vertex input table.
struct RecordStages {
    vertex: Vec<u32>,
    fragment: Vec<u32>,
    channels: BindChannels,
}

/// Slot 0 of a Vulkan program holds the vertex module and slot 1 the fragment module, as the
/// entry points of every variant in the player checked show.
fn stage_words(record: &[u8]) -> Result<RecordStages, SerializedError> {
    let parsed = read_record(record)?;
    let outside = || SerializedError::InvalidShaderProgram {
        offset: 0,
        reason: "the record's code lies outside it",
    };
    let code = record.get(parsed.code.clone()).ok_or_else(outside)?;
    let rest = record.get(parsed.rest.clone()).ok_or_else(outside)?;
    let mut vertex = None;
    let mut fragment = None;
    for stage in vulkan_stages(code)? {
        let bytes = code.get(stage.range.clone()).ok_or_else(outside)?;
        let words = smolv::module_words(stage.encoding, bytes)?;
        match stage.slot {
            0 => vertex = Some(words),
            1 => fragment = Some(words),
            _ => {}
        }
    }
    let missing = |reason| SerializedError::InvalidShaderProgram { offset: 0, reason };
    Ok(RecordStages {
        vertex: vertex.ok_or_else(|| missing("the variant has no vertex module"))?,
        fragment: fragment.ok_or_else(|| missing("the variant has no fragment module"))?,
        channels: read_bind_channels(rest)?,
    })
}

/// The name of the module's only entry point.
fn entry_point(words: &[u32]) -> Option<String> {
    let mut names = entry_point_names(words);
    (names.len() == 1).then(|| names.remove(0))
}
