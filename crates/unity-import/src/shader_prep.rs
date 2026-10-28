//! Preparation of a Unity Vulkan stage module for naga.
//!
//! Unity's SPIR-V uses four things naga 30 cannot read. Each has a rewrite here, applied in
//! this order:
//! 1. Combined image samplers, which naga's SPIR-V frontend does not parse
//!    (gfx-rs/wgpu#4323), are split into separate images and samplers with
//!    `spirv-webgpu-transform`. Each sampler gets a binding of its own, and later bindings in
//!    its descriptor set move up to make room, so [`PreparedModule::bindings`] maps every
//!    resource's binding in Unity's module to its binding in the prepared one.
//! 2. Specialization constants become plain constants with their default values, which is
//!    what a pipeline gets when it supplies no specialization data; naga cannot size arrays by
//!    specialization constants.
//! 3. `OpSNegate` with an unsigned result, which SPIR-V defines as a wrapping `0 - x`, becomes
//!    `OpISub` from a null constant; naga turns it into a unary minus that its own validator
//!    rejects.
//! 4. A GLSL.std.450 bit search (`FindILsb`, `FindSMsb`, `FindUMsb`) whose result differs in
//!    signedness from its operand runs in the operand's type and is bitcast to the result
//!    type; naga keeps the operand's type, so storing the result fails validation. naga also
//!    reads an Msb search's operand as its type says, so a `FindSMsb` of an unsigned operand
//!    or a `FindUMsb` of a signed one is an error.

use std::{
    collections::{BTreeMap, BTreeSet},
    panic::{AssertUnwindSafe, catch_unwind},
};

use worldrunner_unity_serialized::{SerializedError, smolv};

/// A stage module ready for naga.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedModule {
    pub words: Vec<u32>,
    /// Every resource with a descriptor set and binding, ordered by set and Unity binding.
    pub bindings: Vec<ResourceBinding>,
}

/// Where one of a module's resources is bound before and after preparation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ResourceBinding {
    pub set: u32,
    /// The binding in Unity's module.
    pub unity_binding: u32,
    /// The binding in the prepared module; for a split combined sampler, the image's.
    pub binding: u32,
    /// For a split combined sampler, the sampler's binding in the prepared module.
    pub sampler_binding: Option<u32>,
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
    #[error("after the sampler split, {0}")]
    Bindings(&'static str),
    #[error(
        "a FindSMsb or FindUMsb operand's signedness contradicts the search, so naga would search it the other way"
    )]
    MsbSignedness,
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
            Self::Bindings(_) => "bindings".to_owned(),
            Self::MsbSignedness => "Msb signedness".to_owned(),
            Self::Parse { cause, .. } => format!("parse {cause}"),
            Self::Validate { cause, .. } => format!("validate {cause}"),
            Self::Panicked { stage, .. } => format!("{stage} panic"),
        }
    }
}

/// Applies the four rewrites to a stage module.
///
/// # Errors
/// Fails when the module is not structurally valid SPIR-V, the sampler split fails, or the
/// rewrites would need more IDs than SPIR-V allows.
pub fn prepare(words: &[u32]) -> Result<PreparedModule, PrepareError> {
    smolv::entry_points(words)?;
    let split = split_samplers(words)?;
    smolv::entry_points(&split.words)?;
    let bindings = resource_bindings(words, &split)?;
    Ok(PreparedModule {
        words: bit_search_signedness(&signed_negate(&spec_defaults(&split.words))?)?,
        bindings,
    })
}

/// A module after the sampler split, with the descriptor set and Unity binding of each
/// combined sampler that was split.
struct Split {
    words: Vec<u32>,
    combined: BTreeSet<(u32, u32)>,
}

/// The text of a panic payload.
fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|text| (*text).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a panic without a message".to_owned())
}

/// Runs third-party code on game data, turning a panic into an error. The panic hook still
/// runs, and a build with `panic = "abort"` aborts instead.
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

fn split_samplers(words: &[u32]) -> Result<Split, PrepareError> {
    guarded("sampler split", || {
        use spirv_webgpu_transform::{CorrectionMap, CorrectionType, combimgsampsplitter};
        let mut corrections = CorrectionMap::default();
        let split = combimgsampsplitter(words, &mut corrections)
            .map_err(|()| PrepareError::SplitSamplers)?;
        let words = sort_new_decorations(&split, words[BOUND_WORD]);
        // The map lists every decorated resource; split ones carry a correction.
        let combined = corrections
            .sets
            .iter()
            .flatten()
            .flat_map(|(&set, bindings)| {
                let split = bindings.bindings.iter().filter(|(_, binding)| {
                    binding.corrections.contains(&CorrectionType::SplitCombined)
                });
                split.map(move |(&binding, _)| (set, binding))
            })
            .collect();
        Ok(Split { words, combined })
    })
}

/// Puts the decorations of the IDs the sampler split created, those at or above the input's
/// bound, in order of target and decoration. `spirv-webgpu-transform` emits them in hash map
/// order, which differs from run to run; decoration order does not change what a module
/// means.
fn sort_new_decorations(words: &[u32], first_new: u32) -> Vec<u32> {
    let is_new = |instruction: &[u32]| {
        opcode(instruction) == OP_DECORATE && instruction.get(1).is_some_and(|&id| id >= first_new)
    };
    let mut sorted: Vec<&[u32]> = instructions(words).filter(|i| is_new(i)).collect();
    sorted.sort_unstable_by(|a, b| a[1..].cmp(&b[1..]));
    let mut sorted = sorted.into_iter();
    let mut out = words[..HEADER_WORDS].to_vec();
    for instruction in instructions(words) {
        let next = if is_new(instruction) {
            sorted.next().unwrap_or(instruction)
        } else {
            instruction
        };
        out.extend_from_slice(next);
    }
    out
}

const OP_VARIABLE: u32 = 59;
const DECORATION_BINDING: u32 = 33;
const DECORATION_DESCRIPTOR_SET: u32 = 34;

/// The descriptor set and binding of each variable that has both.
fn decorated_bindings(words: &[u32]) -> BTreeMap<u32, (u32, u32)> {
    let mut sets = BTreeMap::new();
    let mut bindings = BTreeMap::new();
    for instruction in instructions(words) {
        if let (OP_DECORATE, &[_, id, decoration, value]) = (opcode(instruction), instruction) {
            match decoration {
                DECORATION_DESCRIPTOR_SET => sets.insert(id, value),
                DECORATION_BINDING => bindings.insert(id, value),
                _ => None,
            };
        }
    }
    let variables: BTreeSet<u32> = instructions(words)
        .filter(|instruction| opcode(instruction) == OP_VARIABLE)
        .filter_map(|instruction| instruction.get(2).copied())
        .collect();
    variables
        .into_iter()
        .filter_map(|id| Some((id, (*sets.get(&id)?, *bindings.get(&id)?))))
        .collect()
}

/// Maps each resource's Unity binding to its prepared binding, read from the decorations of
/// the module before and after the split. A split sampler is the new variable in its set at
/// the binding after its image's.
fn resource_bindings(words: &[u32], split: &Split) -> Result<Vec<ResourceBinding>, PrepareError> {
    let before = decorated_bindings(words);
    let after = decorated_bindings(&split.words);
    let added: BTreeSet<(u32, u32)> = after
        .iter()
        .filter(|(id, _)| !before.contains_key(id))
        .map(|(_, &place)| place)
        .collect();
    let mut resources = Vec::with_capacity(before.len());
    for (id, &(set, unity_binding)) in &before {
        let &(after_set, binding) = after
            .get(id)
            .filter(|(after_set, _)| *after_set == set)
            .ok_or(PrepareError::Bindings("a resource lost its binding"))?;
        let sampler_binding = if split.combined.contains(&(set, unity_binding)) {
            let sampler = binding
                .checked_add(1)
                .filter(|&b| added.contains(&(after_set, b)));
            Some(sampler.ok_or(PrepareError::Bindings("a split sampler has no binding"))?)
        } else {
            None
        };
        resources.push(ResourceBinding {
            set,
            unity_binding,
            binding,
            sampler_binding,
        });
    }
    if added.len() != split.combined.len() {
        return Err(PrepareError::Bindings(
            "new bindings do not match the split samplers",
        ));
    }
    resources.sort_unstable();
    Ok(resources)
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
        // A signed negation stays: naga converts an unsigned operand to the result type.
        if opcode(instruction) != OP_SNEGATE || !unsigned.contains(&result_type) {
            body.extend_from_slice(instruction);
            continue;
        }
        let zero = if let Some(&zero) = zeros.get(&result_type) {
            zero
        } else {
            let zero = ids.next()?;
            zeros.insert(result_type, zero);
            zero
        };
        body.extend([(5 << 16) | OP_ISUB, result_type, result, zero, operand]);
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

const OP_EXT_INST_IMPORT: u32 = 11;
const OP_EXT_INST: u32 = 12;
const OP_BITCAST: u32 = 124;
/// `GLSL.std.450`, as the words of a SPIR-V literal string.
const GLSL_STD_450: [u32; 4] = [0x4c53_4c47, 0x6474_732e, 0x3035_342e, 0];
const FIND_I_LSB: u32 = 73;
const FIND_S_MSB: u32 = 74;
const FIND_U_MSB: u32 = 75;

/// The result type of each ID that has one.
fn result_types(words: &[u32]) -> BTreeMap<u32, u32> {
    instructions(words)
        .filter(|instruction| smolv::has_result_type(opcode(instruction)))
        .filter_map(|instruction| Some((*instruction.get(2)?, *instruction.get(1)?)))
        .collect()
}

/// An integer scalar or vector type's component width and whether it is signed.
fn integer_types(words: &[u32]) -> BTreeMap<u32, (u32, bool)> {
    let mut integers = BTreeMap::new();
    for instruction in instructions(words) {
        match (opcode(instruction), instruction) {
            (OP_TYPE_INT, &[_, id, width, signedness]) => {
                integers.insert(id, (width, signedness != 0));
            }
            (OP_TYPE_VECTOR, &[_, id, component, _]) => {
                if let Some(&component) = integers.get(&component) {
                    integers.insert(id, component);
                }
            }
            _ => {}
        }
    }
    integers
}

/// What a module's types say about its GLSL.std.450 bit searches.
struct BitSearches {
    glsl: BTreeSet<u32>,
    types: BTreeMap<u32, u32>,
    integers: BTreeMap<u32, (u32, bool)>,
}

impl BitSearches {
    fn new(words: &[u32]) -> Self {
        let glsl = instructions(words)
            .filter(|instruction| opcode(instruction) == OP_EXT_INST_IMPORT)
            .filter(|instruction| instruction.get(2..) == Some(&GLSL_STD_450[..]))
            .map(|instruction| instruction[1])
            .collect();
        Self {
            glsl,
            types: result_types(words),
            integers: integer_types(words),
        }
    }

    /// For a bit search whose result needs a cast, the operand's type to run it in.
    ///
    /// # Errors
    /// Fails for `FindSMsb` on an unsigned operand or `FindUMsb` on a signed one: naga
    /// searches the operand as its type says, so the module would search the wrong way.
    fn operand_type(&self, instruction: &[u32; 6]) -> Result<Option<u32>, PrepareError> {
        let &[_, result_type, _, set, search, operand] = instruction;
        let searches = [FIND_I_LSB, FIND_S_MSB, FIND_U_MSB];
        if opcode(instruction) != OP_EXT_INST
            || !self.glsl.contains(&set)
            || !searches.contains(&search)
        {
            return Ok(None);
        }
        let Some(&operand_type) = self.types.get(&operand) else {
            return Ok(None);
        };
        let (Some(&(width, signed)), Some(&(result_width, result_signed))) = (
            self.integers.get(&operand_type),
            self.integers.get(&result_type),
        ) else {
            return Ok(None);
        };
        if (search == FIND_S_MSB && !signed) || (search == FIND_U_MSB && signed) {
            return Err(PrepareError::MsbSignedness);
        }
        Ok((width == result_width && signed != result_signed).then_some(operand_type))
    }
}

/// Runs each GLSL.std.450 bit search whose result type differs in signedness from its
/// operand's, which SPIR-V allows, in the operand's type, followed by a bitcast to the
/// declared result type. naga gives the search the operand's type without a cast, so storing
/// it where the declared type is expected fails validation. The bit index is the same either
/// way, provided the operand is read as the search says, which [`BitSearches::operand_type`]
/// checks.
fn bit_search_signedness(words: &[u32]) -> Result<Vec<u32>, PrepareError> {
    let searches = BitSearches::new(words);
    let mut ids = Ids {
        bound: words[BOUND_WORD],
    };
    let mut out = words[..HEADER_WORDS].to_vec();
    for instruction in instructions(words) {
        let rewrite = match <&[u32; 6]>::try_from(instruction) {
            Ok(six) => searches.operand_type(six)?.map(|ty| (six, ty)),
            Err(_) => None,
        };
        let Some((six, operand_type)) = rewrite else {
            out.extend_from_slice(instruction);
            continue;
        };
        let &[length, result_type, result, set, search, operand] = six;
        let found = ids.next()?;
        out.extend([length, operand_type, found, set, search, operand]);
        out.extend([(4 << 16) | OP_BITCAST, result_type, result, found]);
    }
    out[BOUND_WORD] = ids.bound;
    Ok(out)
}
