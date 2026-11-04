//! Builds the render-program layout of every Vulkan variant of a player's shaders from the
//! variant's decoded parameters and its prepared stage modules, and counts what fails and why.
//! It reports counts only; no module bytes, names or strings leave the decoders.

use std::{collections::BTreeMap, fmt, path::PathBuf};

use worldrunner_render_program::{
    constants::pack_variant_buffer,
    layout::{LayoutError, ProgramLayout},
    module::{ModuleError, Reflected, StageKind, StageModule, StageProgram},
};
use worldrunner_unity_serialized::{
    shader_parameters::{
        BufferMember, ConstantBuffer, ResourceKind, StructLayout, VariantParameters,
    },
    spirv_prep::{PrepareError, PreparedModule, prepare_stages},
};

use crate::{
    PlayerError,
    player_parameters::Tally,
    player_variants::{Event, walk_variants},
};

pub use crate::player_variants::RawStage;

/// Counts from building a player's variant layouts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LayoutReport {
    /// Shaders with a Vulkan platform.
    pub shaders: usize,
    /// Variants whose record and parameters both decode, each pair counted once.
    pub variants: usize,
    /// Variants that failed before their parameters and modules could be read together.
    pub unreadable_variants: BTreeMap<String, usize>,
    pub stage_modules: usize,
    /// Stage modules that `spirv_prep::prepare_stages` accepted, which prepares the stages of
    /// a variant together.
    pub prepared: usize,
    /// Prepared stage modules that naga parsed and validated.
    pub stage_programs: usize,
    /// Variants whose stages all became programs and whose layout was built.
    pub layouts: usize,
    /// Parameters a built layout reports as declared for a stage but unused by its entry point.
    pub unused_parameters: usize,
    /// Built stages whose constant buffer and texture targets all resolve.
    pub targets: Tally,
    pub stage_failures: BTreeMap<String, usize>,
    /// Variants whose stages could not be prepared, once for each variant: the stages are
    /// prepared together, so one failure does not belong to one module.
    pub prepare_failures: BTreeMap<String, usize>,
    pub module_failures: BTreeMap<String, usize>,
    pub layout_failures: BTreeMap<String, usize>,
    pub target_failures: BTreeMap<String, usize>,
    /// Variants with two or more stage programs, and how many of them prepared their stages
    /// to the same numbering: every prepared binding two stages share is the same Unity
    /// resource in both, and every Unity binding two stages share is at the same prepared
    /// binding.
    pub agreeing_numbering: Tally,
    /// Layouts that failed with stages declaring a binding differently although their
    /// numbering agrees.
    pub conflicts_despite_agreement: usize,
    /// Constant buffer members of the variants' bound buffers, by scalar kind, shape and whether
    /// the renderer packs them.
    pub member_shapes: BTreeMap<String, usize>,
    /// Texture bindings by dimension code and the image the module declares there.
    pub texture_codes: BTreeMap<String, usize>,
}

fn count(failures: &mut BTreeMap<String, usize>, cause: impl Into<String>) {
    *failures.entry(cause.into()).or_default() += 1;
}

fn stage_kind(slot: usize) -> Option<StageKind> {
    match slot {
        0 => Some(StageKind::Vertex),
        1 => Some(StageKind::Fragment),
        _ => None,
    }
}

const OP_ENTRY_POINT: u32 = 15;
const FIRST_INSTRUCTION: usize = 5;

/// The execution model and name of the module's only entry point.
fn entry_point(words: &[u32]) -> Option<(u32, String)> {
    let mut found = None;
    let mut at = FIRST_INSTRUCTION;
    while let Some(&head) = words.get(at) {
        let length = usize::try_from(head >> 16).ok().filter(|&n| n > 0)?;
        let instruction = words.get(at..at + length)?;
        if head & 0xffff == OP_ENTRY_POINT {
            let model = *instruction.get(1)?;
            let literal = instruction.get(3..)?;
            let bytes: Vec<u8> = literal
                .iter()
                .flat_map(|word| word.to_le_bytes())
                .take_while(|&byte| byte != 0)
                .collect();
            if found
                .replace((model, String::from_utf8_lossy(&bytes).into_owned()))
                .is_some()
            {
                return None;
            }
        }
        at += length;
    }
    found
}

fn module_cause(error: &ModuleError) -> String {
    match error {
        ModuleError::Parse(_) => "naga cannot parse".to_owned(),
        ModuleError::Validate(_) => "naga rejects".to_owned(),
        ModuleError::Panicked(_) => "naga panicked".to_owned(),
        ModuleError::EntryPointMissing { .. } => "entry point missing".to_owned(),
        ModuleError::UnsupportedResource { what, .. } => format!("unsupported resource: {what}"),
    }
}

fn layout_cause(error: &LayoutError) -> String {
    match error {
        LayoutError::UnsupportedParameter { kind } => format!("unsupported parameter: {kind}"),
        LayoutError::UnboundResource { .. } | LayoutError::UnboundSlot { .. } => {
            "binding outside the module's table".to_owned()
        }
        LayoutError::AmbiguousBinding { .. } => "ambiguous binding".to_owned(),
        LayoutError::UndescribedConstantBuffer { .. } | LayoutError::UndescribedBuffer { .. } => {
            "constant buffer without a layout".to_owned()
        }
        LayoutError::UndeclaredResource { .. } => "module resource without a binding".to_owned(),
        LayoutError::Mismatch { declared, .. } => format!("mismatch: declared {declared}"),
        LayoutError::Conflict { first, second, .. } => {
            format!("stages declare a binding differently: {first} and {second}")
        }
        LayoutError::MultisampledTexture { code } => {
            format!("multisampled texture, code {code}")
        }
        LayoutError::StageMismatch { .. } => "stage mismatch".to_owned(),
    }
}

/// Where a stage places each Unity resource in its prepared module: the prepared binding of
/// an image or buffer, and of the sampler split from a combined image sampler.
type Placed = BTreeMap<(u32, u32), (u32, bool)>;

fn placed(program: &StageProgram) -> Placed {
    let mut placed = Placed::new();
    for entry in &program.module().bindings {
        placed.insert((entry.set, entry.binding), (entry.unity_binding, false));
        if let Some(sampler) = entry.sampler_binding {
            placed.insert((entry.set, sampler), (entry.unity_binding, true));
        }
    }
    placed
}

/// Whether the stages share one numbering: a prepared binding in two stages is one Unity
/// resource, and a Unity resource in two stages is at one prepared binding.
fn numbering_agrees(programs: &[StageProgram]) -> bool {
    let tables: Vec<Placed> = programs.iter().map(placed).collect();
    let from_unity = |table: &Placed| -> BTreeMap<(u32, u32, bool), u32> {
        table
            .iter()
            .map(|(&(set, binding), &(unity, sampler))| ((set, unity, sampler), binding))
            .collect()
    };
    let reverse: Vec<_> = tables.iter().map(from_unity).collect();
    let same = |left: usize, right: usize| {
        let shared_slots = tables[left]
            .iter()
            .all(|(slot, found)| tables[right].get(slot).is_none_or(|other| other == found));
        let shared_resources = reverse[left].iter().all(|(resource, binding)| {
            reverse[right]
                .get(resource)
                .is_none_or(|other| other == binding)
        });
        shared_slots && shared_resources
    };
    (0..programs.len()).all(|left| (left + 1..programs.len()).all(|right| same(left, right)))
}

fn packs(accepted: bool) -> &'static str {
    if accepted { "packs" } else { "refused" }
}

/// A buffer with only `member`, at offset 0.
fn alone_in_buffer(member: &BufferMember) -> ConstantBuffer {
    ConstantBuffer {
        name: String::new(),
        size: 1 << 16,
        members: vec![BufferMember {
            name: String::new(),
            offset: 0,
            ..member.clone()
        }],
        structs: Vec::new(),
    }
}

/// A buffer with a single struct that holds only `member`, at offset 0.
fn alone_in_struct(member: &BufferMember) -> ConstantBuffer {
    ConstantBuffer {
        name: String::new(),
        size: 1 << 16,
        members: Vec::new(),
        structs: vec![StructLayout {
            name: String::new(),
            index: 0,
            array_size: 0,
            size: 1 << 12,
            members: vec![BufferMember {
                name: String::new(),
                offset: 0,
                ..member.clone()
            }],
        }],
    }
}

impl LayoutReport {
    /// Prepares `stages`, builds their programs from `parameters` and lays the variant out.
    pub fn add_variant(&mut self, parameters: &VariantParameters, stages: &[RawStage]) {
        self.variants += 1;
        self.add_members(parameters);
        let Some(programs) = self.stage_programs(parameters, stages) else {
            return;
        };
        for program in &programs {
            self.add_targets(program);
        }
        let agree = numbering_agrees(&programs);
        if programs.len() > 1 {
            self.agreeing_numbering.total += 1;
            self.agreeing_numbering.matched += usize::from(agree);
        }
        let refs: Vec<&StageProgram> = programs.iter().collect();
        match ProgramLayout::build(&refs) {
            Ok(layout) => {
                self.layouts += 1;
                self.unused_parameters += layout.unused().len();
            }
            Err(error) => {
                if agree && matches!(error, LayoutError::Conflict { .. }) {
                    self.conflicts_despite_agreement += 1;
                }
                count(&mut self.layout_failures, layout_cause(&error));
            }
        }
    }

    /// The programs of a variant's stages, prepared together, or `None` if any stage has none.
    fn stage_programs(
        &mut self,
        parameters: &VariantParameters,
        stages: &[RawStage],
    ) -> Option<Vec<StageProgram>> {
        self.stage_modules += stages.len();
        let supported: Vec<(StageKind, &RawStage)> = stages
            .iter()
            .filter_map(|stage| Some((stage_kind(stage.slot)?, stage)))
            .collect();
        for _ in supported.len()..stages.len() {
            count(
                &mut self.stage_failures,
                "a stage other than vertex or fragment",
            );
        }
        let words: Vec<&[u32]> = supported
            .iter()
            .map(|(_, stage)| stage.words.as_slice())
            .collect();
        let prepared = match prepare_stages(&words) {
            Ok(prepared) => prepared.modules,
            Err(error) => {
                count(&mut self.prepare_failures, PrepareError::cause(&error));
                return None;
            }
        };
        self.prepared += prepared.len();
        let programs: Vec<Option<StageProgram>> = supported
            .iter()
            .zip(prepared)
            .map(|(&(kind, _), prepared)| self.stage_program(parameters, kind, prepared))
            .collect();
        let programs = programs.into_iter().collect::<Option<Vec<_>>>()?;
        (programs.len() == stages.len()).then_some(programs)
    }

    fn stage_program(
        &mut self,
        parameters: &VariantParameters,
        kind: StageKind,
        prepared: PreparedModule,
    ) -> Option<StageProgram> {
        let Some((model, entry_point)) = entry_point(&prepared.words) else {
            count(&mut self.stage_failures, "not exactly one entry point");
            return None;
        };
        let expected = if kind == StageKind::Vertex { 0 } else { 4 };
        if model != expected {
            count(&mut self.stage_failures, "entry point of another stage");
            return None;
        }
        let module = StageModule {
            words: prepared.words,
            entry_point,
            kind,
            bindings: prepared.bindings,
        };
        match StageProgram::new(module, parameters.clone()) {
            Ok(program) => {
                self.stage_programs += 1;
                self.add_texture_codes(&program, parameters);
                Some(program)
            }
            Err(error) => {
                count(&mut self.module_failures, module_cause(&error));
                None
            }
        }
    }

    /// Counts the members of the constant buffers the variant binds by shape, and whether the
    /// packer accepts a member of that shape. A buffer with structs counts as one, and the
    /// members of its structs by shape as well.
    fn add_members(&mut self, parameters: &VariantParameters) {
        let bound = parameters
            .bindings
            .iter()
            .filter(|binding| binding.kind == ResourceKind::ConstantBuffer);
        for binding in bound {
            let layouts = parameters.constant_buffers.iter();
            for buffer in layouts.filter(|buffer| buffer.name == binding.name) {
                if !buffer.structs.is_empty() {
                    let packs = packs(pack_variant_buffer(buffer, &[], 0).is_ok());
                    count(
                        &mut self.member_shapes,
                        format!("buffer with structs: {packs}"),
                    );
                }
                for member in &buffer.members {
                    let alone = alone_in_buffer(member);
                    self.add_member("", member, &alone);
                }
                for layout in &buffer.structs {
                    for member in &layout.members {
                        let alone = alone_in_struct(member);
                        self.add_member("struct member ", member, &alone);
                    }
                }
            }
        }
    }

    /// Counts `member` by its shape, and whether `alone`, a buffer holding only it, packs.
    fn add_member(&mut self, prefix: &str, member: &BufferMember, alone: &ConstantBuffer) {
        let shape = format!(
            "kind {} {}x{}{}{}",
            member.kind,
            member.rows,
            member.columns,
            if member.is_matrix { " matrix" } else { "" },
            if member.array_size > 0 { " array" } else { "" },
        );
        let packs = packs(pack_variant_buffer(alone, &[], 0).is_ok());
        count(&mut self.member_shapes, format!("{prefix}{shape}: {packs}"));
    }

    /// Counts each texture binding the stage uses by its dimension code and the module's image.
    fn add_texture_codes(&mut self, program: &StageProgram, parameters: &VariantParameters) {
        let Some(bit) = program.module().kind.stage_bit() else {
            return;
        };
        for binding in &parameters.bindings {
            let ResourceKind::Texture { dimension, .. } = binding.kind else {
                continue;
            };
            if binding.slot.stages & bit == 0 {
                continue;
            }
            let (set, unity) = (u32::from(binding.slot.set), u32::from(binding.slot.binding));
            let image = program
                .module()
                .bindings
                .iter()
                .find(|b| b.set == set && b.unity_binding == unity)
                .and_then(|placed| {
                    program
                        .resources()
                        .iter()
                        .find(|r| r.set == placed.set && r.binding == placed.binding)
                });
            let found = match image.map(Reflected::texture) {
                Some(Some((view_dimension, multisampled))) => {
                    let suffix = if multisampled { " multisampled" } else { "" };
                    format!("{view_dimension:?}{suffix}")
                }
                Some(None) => "not a texture".to_owned(),
                None => "unused by the entry point".to_owned(),
            };
            count(
                &mut self.texture_codes,
                format!("code {dimension}: {found}"),
            );
        }
    }

    fn add_targets(&mut self, program: &StageProgram) {
        let buffers = program.constant_buffer_targets().map(|_| ());
        let textures = program.texture_targets().map(|_| ());
        self.targets.total += 1;
        match buffers.and(textures) {
            Ok(()) => self.targets.matched += 1,
            Err(error) => count(&mut self.target_failures, layout_cause(&error)),
        }
    }
}

fn failures(
    f: &mut fmt::Formatter<'_>,
    title: &str,
    failures: &BTreeMap<String, usize>,
) -> fmt::Result {
    if failures.is_empty() {
        return Ok(());
    }
    writeln!(f, "{title}:")?;
    for (cause, count) in failures {
        writeln!(f, "  {count} {cause}")?;
    }
    Ok(())
}

impl fmt::Display for LayoutReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{} shaders with a Vulkan platform", self.shaders)?;
        writeln!(
            f,
            "{} variants whose record and parameters decode, of which {} have a layout built",
            self.variants, self.layouts
        )?;
        failures(
            f,
            "variants that could not be read",
            &self.unreadable_variants,
        )?;
        writeln!(
            f,
            "{} stage modules: {} prepared, {} parsed and validated as programs",
            self.stage_modules, self.prepared, self.stage_programs
        )?;
        failures(
            f,
            "stage modules of an unsupported shape",
            &self.stage_failures,
        )?;
        failures(
            f,
            "stage modules that could not be prepared",
            &self.prepare_failures,
        )?;
        failures(
            f,
            "stage programs naga or the renderer refused",
            &self.module_failures,
        )?;
        writeln!(
            f,
            "variants with two stages whose prepared numbering agrees: {}",
            self.agreeing_numbering
        )?;
        failures(f, "layouts that could not be built", &self.layout_failures)?;
        writeln!(
            f,
            "layouts that conflicted although the numbering agrees: {}",
            self.conflicts_despite_agreement
        )?;
        failures(f, "constant buffer members by shape", &self.member_shapes)?;
        failures(
            f,
            "texture bindings by dimension code and the module's image",
            &self.texture_codes,
        )?;
        writeln!(
            f,
            "parameters declared for a stage that its entry point does not use: {}",
            self.unused_parameters
        )?;
        writeln!(
            f,
            "stage programs whose buffer and texture targets resolve: {}",
            self.targets
        )?;
        failures(f, "targets that did not resolve", &self.target_failures)
    }
}

/// Builds the layout of every Vulkan variant of the shaders in `paths`.
///
/// # Errors
/// Fails if a file cannot be read or a shader does not load; a variant that does not decode
/// or lay out is counted, not an error.
pub fn layout_report(paths: &[PathBuf]) -> Result<LayoutReport, PlayerError> {
    let mut report = LayoutReport::default();
    walk_variants(paths, &mut |event| match event {
        Event::Shader => report.shaders += 1,
        Event::Unreadable(cause) => count(&mut report.unreadable_variants, cause),
        Event::Variant {
            parameters, stages, ..
        } => report.add_variant(parameters, stages),
    })?;
    Ok(report)
}
