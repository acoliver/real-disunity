//! Checks the per-variant parameters of a player's Vulkan shaders: that every parameter entry
//! and program record tail decodes with exact consumption, and that what the parameters bind
//! is what the variant's stage modules declare. It reports counts only.

use std::{
    collections::{BTreeMap, BTreeSet, btree_map::Entry},
    fmt,
    ops::RangeInclusive,
    path::PathBuf,
};

use worldrunner_unity_serialized::{
    ClassId, SerializedError,
    shader::Shader,
    shader_parameters::{
        ResourceBinding, ResourceKind, VariantParameters, read_bind_channels, read_parameters,
    },
    shader_program::{BindChannels, Program},
    shader_records::{
        ProgramEntry, platform_program_types, program_table, read_record, vulkan_stages,
    },
    smolv,
};

use crate::{
    PlayerError,
    player_module_resources::{ModuleResource, ResourceClass, module_resources},
    player_objects::{OpenFile, load_shader, shader_platforms},
    player_shaders::VULKAN,
};

/// How many of some things pass a check.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tally {
    pub total: usize,
    pub matched: usize,
}

impl Tally {
    fn add(&mut self, matched: bool) {
        self.total += 1;
        self.matched += usize::from(matched);
    }
}

impl fmt::Display for Tally {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} of {}", self.matched, self.total)
    }
}

/// A stage module's resources, with the stage bit its slot corresponds to in a
/// [`worldrunner_unity_serialized::shader_parameters::DescriptorSlot`].
pub struct StageModule {
    pub slot: usize,
    pub resources: Vec<ModuleResource>,
}

impl StageModule {
    fn stage(&self) -> u8 {
        u32::try_from(self.slot)
            .ok()
            .and_then(|slot| 1u8.checked_shl(slot))
            .unwrap_or_default()
    }

    fn keys(&self) -> BTreeSet<Key> {
        self.resources
            .iter()
            .map(|r| (r.set, r.binding, r.class))
            .collect()
    }
}

/// A resource's descriptor set, binding and class.
type Key = (u32, u32, ResourceClass);

/// Counts from checking a player's Vulkan variant parameters.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ParameterReport {
    /// Shaders with a Vulkan platform.
    pub shaders: usize,
    /// Entries in those platforms' blob tables.
    pub table_entries: usize,
    /// Distinct entries Vulkan variants name as program records.
    pub program_entries: usize,
    /// Distinct entries Vulkan variants name as parameters.
    pub parameter_entries: usize,
    /// Table entries no Vulkan variant names.
    pub unreferenced_entries: usize,
    /// Programs with Vulkan variants whose parameter lists pair with their variants.
    pub paired_programs: Tally,
    /// Parameter entries that decode with exact consumption.
    pub parameters: Tally,
    pub parameter_failures: BTreeMap<String, usize>,
    /// Program records that decode, with their stage modules.
    pub records: Tally,
    pub record_failures: BTreeMap<String, usize>,
    /// Record tails that decode as a vertex input table with exact consumption.
    pub tails: Tally,
    pub tail_failures: BTreeMap<String, usize>,
    /// Decoded tails with one pair per set bit of the source mask, on that bit.
    pub tails_consistent: Tally,
    /// Distinct pairs of program record and parameter entry.
    pub variants: usize,
    /// Stage modules whose resources are exactly the ones the variant's parameters bind there.
    pub stage_modules: Tally,
    /// Module resources that the parameters bind in that stage.
    pub module_resources: Tally,
    /// Bindings by kind that land on a module resource in every stage they name.
    pub bindings: BTreeMap<&'static str, Tally>,
    /// Textures' sampler slots that land on a module sampler in every stage they name.
    pub sampler_slots: Tally,
    /// Constant buffer bindings with a layout of the same name.
    pub buffer_layouts: Tally,
    /// Members of those layouts whose offset is one of the bound block's member offsets in a
    /// stage the binding names.
    pub buffer_members: Tally,
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

/// Counts a result in `tally`, and its failure by cause.
fn settle<T>(
    tally: &mut Tally,
    failures: &mut BTreeMap<String, usize>,
    result: Result<T, SerializedError>,
) -> Option<T> {
    tally.add(result.is_ok());
    match result {
        Ok(value) => Some(value),
        Err(error) => {
            *failures.entry(cause(&error)).or_default() += 1;
            None
        }
    }
}

fn class_of(kind: &ResourceKind) -> ResourceClass {
    match kind {
        ResourceKind::Texture { .. } | ResourceKind::StorageImage => ResourceClass::Image,
        ResourceKind::Sampler { .. } => ResourceClass::Sampler,
        ResourceKind::ConstantBuffer | ResourceKind::Buffer => ResourceClass::Block,
    }
}

const fn label(kind: &ResourceKind) -> &'static str {
    match kind {
        ResourceKind::Texture { .. } => "texture",
        ResourceKind::ConstantBuffer => "constant buffer",
        ResourceKind::Buffer => "buffer",
        ResourceKind::StorageImage => "storage image",
        ResourceKind::Sampler { .. } => "sampler",
    }
}

fn key_of(binding: &ResourceBinding) -> Key {
    let slot = binding.slot;
    (
        u32::from(slot.set),
        u32::from(slot.binding),
        class_of(&binding.kind),
    )
}

/// The key of the sampler a texture binding is read with.
fn sampler_key(binding: &ResourceBinding) -> Option<(u8, Key)> {
    match binding.kind {
        ResourceKind::Texture {
            sampler: Some(slot),
            ..
        } => Some((
            slot.stages,
            (
                u32::from(slot.set),
                u32::from(slot.binding),
                ResourceClass::Sampler,
            ),
        )),
        _ => None,
    }
}

/// Whether `key` is in the module of every stage in `stages`, and every one of those stages has
/// a module.
fn in_every_stage(stages: u8, key: Key, modules: &[StageModule]) -> bool {
    let named: Vec<&StageModule> = modules.iter().filter(|m| m.stage() & stages != 0).collect();
    let found = named.iter().fold(0, |found, m| found | m.stage());
    stages != 0 && stages & !found == 0 && named.iter().all(|m| m.keys().contains(&key))
}

/// The keys the parameters bind in one stage.
fn declared(parameters: &VariantParameters, stage: u8) -> BTreeSet<Key> {
    let mut keys = BTreeSet::new();
    for binding in &parameters.bindings {
        if binding.slot.stages & stage != 0 {
            keys.insert(key_of(binding));
        }
        if let Some((_, key)) = sampler_key(binding).filter(|(stages, _)| stages & stage != 0) {
            keys.insert(key);
        }
    }
    keys
}

fn consistent(channels: &BindChannels) -> bool {
    let bits = usize::try_from(channels.source_map.count_ones()).unwrap_or(usize::MAX);
    let on_a_set_bit = |&[source, _]: &[i8; 2]| {
        u32::try_from(source)
            .ok()
            .and_then(|bit| 1i32.checked_shl(bit))
            .is_some_and(|mask| channels.source_map & mask != 0)
    };
    bits == channels.channels.len() && channels.channels.iter().all(on_a_set_bit)
}

/// A Vulkan platform's blob table with the chunks it indexes.
struct Platform<'a> {
    chunks: &'a [Vec<u8>],
    table: Vec<ProgramEntry>,
    types: RangeInclusive<i8>,
}

impl<'a> Platform<'a> {
    fn entry(&self, index: u32) -> Option<&'a [u8]> {
        usize::try_from(index)
            .ok()
            .and_then(|index| self.table.get(index))
            .and_then(|entry| entry.bytes(self.chunks))
    }
}

/// What the shader's variants have been seen to name, so each is counted once.
#[derive(Default)]
struct Seen {
    records: BTreeSet<u32>,
    parameters: BTreeMap<u32, Option<VariantParameters>>,
    variants: BTreeSet<(u32, u32)>,
}

const MISSING: SerializedError = SerializedError::InvalidShaderProgram {
    offset: 0,
    reason: "a variant names an entry that does not exist",
};

/// A program record's stage modules, and its tail.
fn read_modules(record: &[u8]) -> Result<(Vec<StageModule>, &[u8]), SerializedError> {
    let read = read_record(record)?;
    let outside = || SerializedError::InvalidShaderProgram {
        offset: 0,
        reason: "the record's code lies outside it",
    };
    let code = record.get(read.code.clone()).ok_or_else(outside)?;
    let mut modules = Vec::new();
    for stage in vulkan_stages(code)? {
        let bytes = code.get(stage.range.clone()).ok_or_else(outside)?;
        let words = smolv::module_words(stage.encoding, bytes)?;
        modules.push(StageModule {
            slot: stage.slot,
            resources: module_resources(&words),
        });
    }
    let tail = record.get(read.rest).ok_or_else(outside)?;
    Ok((modules, tail))
}

impl ParameterReport {
    /// Counts what the parameters bind against the stage modules of the same variant.
    pub fn add_variant(&mut self, parameters: &VariantParameters, modules: &[StageModule]) {
        self.variants += 1;
        for module in modules {
            let keys = module.keys();
            let bound = declared(parameters, module.stage());
            self.stage_modules.add(keys == bound);
            for key in &keys {
                self.module_resources.add(bound.contains(key));
            }
        }
        for binding in &parameters.bindings {
            self.add_binding(binding, parameters, modules);
        }
    }

    fn add_binding(
        &mut self,
        binding: &ResourceBinding,
        parameters: &VariantParameters,
        modules: &[StageModule],
    ) {
        let matched = in_every_stage(binding.slot.stages, key_of(binding), modules);
        self.bindings
            .entry(label(&binding.kind))
            .or_default()
            .add(matched);
        if let Some((stages, key)) = sampler_key(binding) {
            self.sampler_slots.add(in_every_stage(stages, key, modules));
        }
        if binding.kind == ResourceKind::ConstantBuffer {
            self.add_buffer(binding, parameters, modules);
        }
    }

    fn add_buffer(
        &mut self,
        binding: &ResourceBinding,
        parameters: &VariantParameters,
        modules: &[StageModule],
    ) {
        let layout = parameters
            .constant_buffers
            .iter()
            .find(|buffer| buffer.name == binding.name);
        self.buffer_layouts.add(layout.is_some());
        let (set, index) = (u32::from(binding.slot.set), u32::from(binding.slot.binding));
        let offsets: BTreeSet<u32> = modules
            .iter()
            .filter(|m| m.stage() & binding.slot.stages != 0)
            .flat_map(|m| &m.resources)
            .filter(|r| r.set == set && r.binding == index && r.class == ResourceClass::Block)
            .flat_map(|r| r.member_offsets.iter().copied())
            .collect();
        for member in layout.iter().flat_map(|buffer| &buffer.members) {
            let on_offset = u32::try_from(member.offset).is_ok_and(|o| offsets.contains(&o));
            self.buffer_members.add(on_offset);
        }
    }

    fn add_tail(&mut self, tail: &[u8]) {
        let channels = settle(
            &mut self.tails,
            &mut self.tail_failures,
            read_bind_channels(tail),
        );
        if let Some(channels) = channels {
            self.tails_consistent.add(consistent(&channels));
        }
    }

    /// Decodes a variant's parameter entry the first time a shader names it.
    fn parameters_of<'s>(
        &mut self,
        index: u32,
        platform: &Platform<'_>,
        seen: &'s mut Seen,
    ) -> Option<&'s VariantParameters> {
        let parsed = match seen.parameters.entry(index) {
            Entry::Occupied(known) => known.into_mut(),
            Entry::Vacant(unknown) => {
                let result = platform
                    .entry(index)
                    .ok_or(MISSING)
                    .and_then(read_parameters);
                let parsed = settle(&mut self.parameters, &mut self.parameter_failures, result);
                unknown.insert(parsed)
            }
        };
        parsed.as_ref()
    }

    /// Decodes a variant's program record, counting it and its tail the first time.
    fn modules_of(
        &mut self,
        index: u32,
        platform: &Platform<'_>,
        seen: &mut Seen,
    ) -> Option<Vec<StageModule>> {
        let first = seen.records.insert(index);
        let result = platform.entry(index).ok_or(MISSING).and_then(read_modules);
        match result {
            Ok((modules, tail)) => {
                if first {
                    self.records.add(true);
                    self.add_tail(tail);
                }
                Some(modules)
            }
            Err(error) => {
                if first {
                    self.records.add(false);
                    *self.record_failures.entry(cause(&error)).or_default() += 1;
                }
                None
            }
        }
    }

    fn add_program(&mut self, program: &Program, platform: &Platform<'_>, seen: &mut Seen) {
        let pairs = program.variant_blobs();
        if pairs.as_ref().is_ok_and(Vec::is_empty) {
            return;
        }
        self.paired_programs.add(pairs.is_ok());
        for (variant, entry) in pairs.unwrap_or_default() {
            if !platform.types.contains(&variant.gpu_program_type)
                || !seen.variants.insert((variant.blob_index, entry))
            {
                continue;
            }
            let modules = self.modules_of(variant.blob_index, platform, seen);
            let parameters = self.parameters_of(entry, platform, seen).cloned();
            if let Some((modules, parameters)) = modules.zip(parameters) {
                self.add_variant(&parameters, &modules);
            }
        }
    }

    fn add_platform(
        &mut self,
        shader: &Shader,
        kind: u32,
        chunks: &[Vec<u8>],
    ) -> Result<(), SerializedError> {
        let lengths: Vec<usize> = chunks.iter().map(Vec::len).collect();
        let types = platform_program_types(kind).ok_or(MISSING)?;
        let table = match chunks.first() {
            Some(first) => program_table(first, &lengths)?,
            None => Vec::new(),
        };
        let platform = Platform {
            chunks,
            table,
            types,
        };
        let mut seen = Seen::default();
        for program in shader_programs(shader) {
            self.add_program(program, &platform, &mut seen);
        }
        let named: BTreeSet<u32> = seen
            .records
            .iter()
            .chain(seen.parameters.keys())
            .copied()
            .collect();
        self.table_entries += platform.table.len();
        self.program_entries += seen.records.len();
        self.parameter_entries += seen.parameters.len();
        self.unreferenced_entries += platform.table.len().saturating_sub(named.len());
        Ok(())
    }
}

pub(crate) fn shader_programs(shader: &Shader) -> Vec<&Program> {
    let passes = shader.parsed.sub_shaders.iter().flat_map(|sub| &sub.passes);
    passes
        .flat_map(|pass| {
            let p = &pass.programs;
            [
                &p.vertex,
                &p.fragment,
                &p.geometry,
                &p.hull,
                &p.domain,
                &p.ray_tracing,
            ]
        })
        .collect()
}

fn failures(f: &mut fmt::Formatter<'_>, failures: &BTreeMap<String, usize>) -> fmt::Result {
    for (cause, count) in failures {
        writeln!(f, "  {count} {cause}")?;
    }
    Ok(())
}

fn counted(f: &mut fmt::Formatter<'_>, what: &str, tally: Tally) -> fmt::Result {
    writeln!(f, "{what}: {tally}")
}

impl ParameterReport {
    /// The counts of what decodes.
    fn write_decoding(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "{} shaders with a Vulkan platform: {} blob entries, of which {} are program \
             records and {} parameter entries named by variants, and {} are named by no variant",
            self.shaders,
            self.table_entries,
            self.program_entries,
            self.parameter_entries,
            self.unreferenced_entries
        )?;
        let what = "programs whose parameter lists pair with their variants";
        counted(f, what, self.paired_programs)?;
        let what = "parameter entries that decode with exact consumption";
        counted(f, what, self.parameters)?;
        failures(f, &self.parameter_failures)?;
        counted(f, "program records that decode", self.records)?;
        failures(f, &self.record_failures)?;
        let what = "record tails that decode with exact consumption";
        counted(f, what, self.tails)?;
        failures(f, &self.tail_failures)?;
        let what = "decoded tails with one pair on each set source bit";
        counted(f, what, self.tails_consistent)
    }

    /// The counts of what the parameters bind against the modules.
    fn write_matching(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let variants = "variants (distinct program record and parameter entry pairs)";
        writeln!(f, "{} {variants}", self.variants)?;
        let what = "stage modules whose resources are exactly the ones bound";
        counted(f, what, self.stage_modules)?;
        counted(
            f,
            "module resources the parameters bind",
            self.module_resources,
        )?;
        for (kind, tally) in &self.bindings {
            let what = format!("{kind} bindings on a module resource in every stage they name");
            counted(f, &what, *tally)?;
        }
        let what = "texture sampler slots on a module sampler";
        counted(f, what, self.sampler_slots)?;
        counted(
            f,
            "constant buffer bindings with a layout",
            self.buffer_layouts,
        )?;
        let what = "layout members at an offset of the bound block";
        counted(f, what, self.buffer_members)
    }
}

impl fmt::Display for ParameterReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write_decoding(f)?;
        self.write_matching(f)
    }
}

/// Decodes every Vulkan variant's parameter entry and record of the shaders in `paths` and
/// checks them against the variants' stage modules.
///
/// # Errors
/// Fails if a file cannot be read or a shader does not load; an entry that does not decode is
/// counted, not an error.
pub fn parameter_report(paths: &[PathBuf]) -> Result<ParameterReport, PlayerError> {
    let mut report = ParameterReport::default();
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
            let Some(platform) = platforms.iter().position(|&kind| kind == VULKAN) else {
                continue;
            };
            report.shaders += 1;
            let (shader, kind, chunks) = load_shader(&mut open, path_id, platform)?;
            report
                .add_platform(&shader, kind, &chunks)
                .map_err(|source| open.error(source))?;
        }
    }
    Ok(report)
}
