//! Preparation of a Unity Vulkan stage module for naga.
//!
//! Unity's SPIR-V uses three things naga 30 cannot read. Each has a rewrite here, applied in
//! this order:
//! 1. Combined image samplers, which naga's SPIR-V frontend does not parse
//!    (gfx-rs/wgpu#4323), are split into separate images and samplers with
//!    `spirv-webgpu-transform`. Each sampler gets a binding of its own, recorded in
//!    [`PreparedModule::split_samplers`] so a Unity texture can be bound to both.
//! 2. Specialization constants become plain constants with their default values, which is
//!    what a pipeline gets when it supplies no specialization data; naga cannot size arrays by
//!    specialization constants.
//! 3. `OpSNegate` with an unsigned result, which SPIR-V defines as a wrapping `0 - x`, becomes
//!    `OpISub` from a null constant; naga turns it into a unary minus that its own validator
//!    rejects. With a signed result, an unsigned operand is bitcast first.

use std::{
    collections::{BTreeMap, BTreeSet},
    panic::{AssertUnwindSafe, catch_unwind},
};

use worldrunner_unity_serialized::{SerializedError, smolv};

/// A stage module ready for naga.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedModule {
    pub words: Vec<u32>,
    /// Each combined image sampler that was split, as its descriptor set and binding. The
    /// sampler is bound right after the image.
    pub split_samplers: Vec<(u32, u32)>,
}

/// Why a module could not be prepared or checked.
#[derive(Debug, thiserror::Error)]
pub enum PrepareError {
    #[error(transparent)]
    Module(#[from] SerializedError),
    #[error("the combined image samplers could not be split")]
    SplitSamplers,
    #[error("the module needs more IDs than SPIR-V allows")]
    IdBound,
    #[error("naga cannot parse the module: {cause}: {message}")]
    Parse { cause: String, message: String },
    #[error("naga rejects the module: {cause}: {message}")]
    Validate { cause: String, message: String },
    #[error("{stage} panicked: {message}")]
    Panicked {
        stage: &'static str,
        message: String,
    },
}

impl PrepareError {
    /// A short name for the failure, for counting failures by cause.
    #[must_use]
    pub fn cause(&self) -> String {
        match self {
            Self::Module(_) => "invalid module".to_owned(),
            Self::SplitSamplers => "sampler split".to_owned(),
            Self::IdBound => "ID bound".to_owned(),
            Self::Parse { cause, .. } => format!("parse {cause}"),
            Self::Validate { cause, .. } => format!("validate {cause}"),
            Self::Panicked { stage, .. } => format!("{stage} panic"),
        }
    }
}

/// Applies the three rewrites to a stage module.
///
/// # Errors
/// Fails when the module is not structurally valid SPIR-V, the sampler split fails, or the
/// rewrites would need more IDs than SPIR-V allows.
pub fn prepare(words: &[u32]) -> Result<PreparedModule, PrepareError> {
    smolv::entry_points(words)?;
    let split = split_samplers(words)?;
    smolv::entry_points(&split.words)?;
    Ok(PreparedModule {
        words: signed_negate(&spec_defaults(&split.words))?,
        split_samplers: split.split_samplers,
    })
}

/// The text of a panic payload.
fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|text| (*text).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a panic without a message".to_owned())
}

/// Runs third-party code on game data, turning a panic into an error.
fn guarded<T>(
    stage: &'static str,
    run: impl FnOnce() -> Result<T, PrepareError>,
) -> Result<T, PrepareError> {
    catch_unwind(AssertUnwindSafe(run)).unwrap_or_else(|payload| {
        Err(PrepareError::Panicked {
            stage,
            message: panic_message(payload.as_ref()),
        })
    })
}

fn split_samplers(words: &[u32]) -> Result<PreparedModule, PrepareError> {
    guarded("sampler split", || {
        let mut corrections = spirv_webgpu_transform::CorrectionMap::default();
        let words = spirv_webgpu_transform::combimgsampsplitter(words, &mut corrections)
            .map_err(|()| PrepareError::SplitSamplers)?;
        let mut bindings: Vec<(u32, u32)> = corrections
            .sets
            .iter()
            .flatten()
            .flat_map(|(&set, bindings)| bindings.bindings.keys().map(move |&b| (set, b)))
            .collect();
        bindings.sort_unstable();
        Ok(PreparedModule {
            words,
            split_samplers: bindings,
        })
    })
}

/// The name of an error's variant, from its `Debug` form.
fn variant_name(error: &impl std::fmt::Debug) -> String {
    let text = format!("{error:?}");
    let end = text.find(['(', ' ', '{']).unwrap_or(text.len());
    text[..end].to_owned()
}

/// The innermost cause of an error.
fn innermost<'e>(
    error: &'e (dyn std::error::Error + 'static),
) -> &'e (dyn std::error::Error + 'static) {
    let mut current = error;
    while let Some(source) = current.source() {
        current = source;
    }
    current
}

/// Parses and validates a prepared module with naga, with every validation capability
/// enabled.
///
/// # Errors
/// Fails with naga's parse or validation error, named by its innermost cause, or when naga
/// panics.
pub fn check_with_naga(words: &[u32]) -> Result<naga::Module, PrepareError> {
    guarded("naga", || {
        let bytes: Vec<u8> = words.iter().flat_map(|word| word.to_le_bytes()).collect();
        let options = naga::front::spv::Options::default();
        let module = naga::front::spv::parse_u8_slice(&bytes, &options).map_err(|error| {
            PrepareError::Parse {
                cause: variant_name(&error),
                message: error.to_string(),
            }
        })?;
        let flags = naga::valid::ValidationFlags::all();
        let capabilities = naga::valid::Capabilities::all();
        naga::valid::Validator::new(flags, capabilities)
            .validate(&module)
            .map_err(|error| {
                let error = error.into_inner();
                PrepareError::Validate {
                    cause: variant_name(&innermost(&error)),
                    message: format!("{error:?}"),
                }
            })?;
        Ok(module)
    })
}

const HEADER_WORDS: usize = 5;
const BOUND_WORD: usize = 3;

/// The instructions of a module that [`smolv::entry_points`] has accepted.
fn instructions(words: &[u32]) -> impl Iterator<Item = &[u32]> {
    let mut rest = words.get(HEADER_WORDS..).unwrap_or_default();
    std::iter::from_fn(move || {
        let count = ((*rest.first()? >> 16) as usize).clamp(1, rest.len());
        let (instruction, tail) = rest.split_at(count);
        rest = tail;
        Some(instruction)
    })
}

fn opcode(instruction: &[u32]) -> u32 {
    instruction[0] & 0xffff
}

const OP_DECORATE: u32 = 71;
const DECORATION_SPEC_ID: u32 = 1;

/// Specialization constant opcodes and the plain constants they become.
const SPEC_TO_PLAIN: [(u32, u32); 4] = [(48, 41), (49, 42), (50, 43), (51, 44)];

/// Replaces specialization constants by plain constants with their default values, and drops
/// their `SpecId` decorations, since naga treats a decorated constant as an override.
fn spec_defaults(words: &[u32]) -> Vec<u32> {
    let mut out = words[..HEADER_WORDS].to_vec();
    for instruction in instructions(words) {
        let op = opcode(instruction);
        if op == OP_DECORATE && instruction.get(2) == Some(&DECORATION_SPEC_ID) {
            continue;
        }
        let start = out.len();
        out.extend_from_slice(instruction);
        if let Some(&(_, plain)) = SPEC_TO_PLAIN.iter().find(|&&(spec, _)| spec == op) {
            out[start] = (out[start] & 0xffff_0000) | plain;
        }
    }
    out
}

const OP_TYPE_INT: u32 = 21;
const OP_TYPE_VECTOR: u32 = 23;
const OP_BITCAST: u32 = 124;
const OP_SNEGATE: u32 = 126;
const OP_ISUB: u32 = 130;
const OP_CONSTANT_NULL: u32 = 46;
const OP_FUNCTION: u32 = 54;

/// Unsigned integer scalar and vector type IDs.
fn unsigned_types(words: &[u32]) -> BTreeSet<u32> {
    let mut unsigned = BTreeSet::new();
    for instruction in instructions(words) {
        match (opcode(instruction), instruction) {
            (OP_TYPE_INT, &[_, id, _, 0]) => {
                unsigned.insert(id);
            }
            (OP_TYPE_VECTOR, &[_, id, component, _]) if unsigned.contains(&component) => {
                unsigned.insert(id);
            }
            _ => {}
        }
    }
    unsigned
}

/// Hands out new result IDs above a module's bound.
struct Ids {
    bound: u32,
}

impl Ids {
    fn next(&mut self) -> Result<u32, PrepareError> {
        let id = self.bound;
        self.bound = id.checked_add(1).ok_or(PrepareError::IdBound)?;
        Ok(id)
    }
}

/// Lowers each `OpSNegate` as described in the module documentation.
fn signed_negate(words: &[u32]) -> Result<Vec<u32>, PrepareError> {
    let unsigned = unsigned_types(words);
    let mut ids = Ids {
        bound: words[BOUND_WORD],
    };
    let mut zeros = BTreeMap::<u32, u32>::new();
    let mut body = Vec::with_capacity(words.len());
    for instruction in instructions(words) {
        let &[_, result_type, result, operand] = instruction else {
            body.extend_from_slice(instruction);
            continue;
        };
        if opcode(instruction) != OP_SNEGATE {
            body.extend_from_slice(instruction);
        } else if unsigned.contains(&result_type) {
            let zero = if let Some(&zero) = zeros.get(&result_type) {
                zero
            } else {
                let zero = ids.next()?;
                zeros.insert(result_type, zero);
                zero
            };
            body.extend([(5 << 16) | OP_ISUB, result_type, result, zero, operand]);
        } else {
            let cast = ids.next()?;
            body.extend([(4 << 16) | OP_BITCAST, result_type, cast, operand]);
            body.extend([(4 << 16) | OP_SNEGATE, result_type, result, cast]);
        }
    }
    let mut out = words[..HEADER_WORDS].to_vec();
    out[BOUND_WORD] = ids.bound;
    let nulls: Vec<u32> = zeros
        .iter()
        .flat_map(|(&ty, &zero)| [(3 << 16) | OP_CONSTANT_NULL, ty, zero])
        .collect();
    // Constants belong before the first function.
    let (globals, functions) = body.split_at(instructions_offset(&body, OP_FUNCTION));
    out.extend_from_slice(globals);
    out.extend(nulls);
    out.extend_from_slice(functions);
    Ok(out)
}

/// The word offset in a header-less instruction stream of the first instruction with an
/// opcode, or the stream's length.
fn instructions_offset(body: &[u32], op: u32) -> usize {
    let mut at = 0;
    while let Some(&word) = body.get(at) {
        if word & 0xffff == op {
            return at;
        }
        at += ((word >> 16) as usize).max(1);
    }
    body.len()
}
